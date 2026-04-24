//! 真实安卓手机设备测试示例
//!
//! 前置条件：
//! 1. 安卓手机已开启 USB 调试并通过 ADB 连接
//! 2. `adb` 命令在 PATH 中可用
//!
//! 运行方式：
//! ```bash
//! cargo run -p beebotos-agents --example android_device_test
//! ```

use std::time::Duration;

use beebotos_agents::device::{
    AndroidController, AndroidDevice, AppLifecycle, DeviceAutomation, ElementLocator,
    HardwareButton, LocatorType, SwipeDirection,
};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BeeBotOS Android 真实设备测试 ===\n");

    // 1. 自动获取所有已连接的安卓设备
    let devices = get_connected_devices().await?;
    if devices.is_empty() {
        eprintln!("❌ 未检测到任何通过 ADB 连接的安卓设备。请检查:");
        eprintln!("   1. 手机是否已开启 USB 调试模式");
        eprintln!("   2. 数据线是否连接正常");
        eprintln!("   3. 是否在手机上允许了该电脑的调试权限");
        eprintln!("   4. adb 命令是否在系统 PATH 中可用");
        std::process::exit(1);
    }

    println!("检测到 {} 台已连接的设备:\n", devices.len());
    for (i, serial) in devices.iter().enumerate() {
        println!("  [{}] {}", i + 1, serial);
    }
    println!();

    // 2. 对每台设备顺序执行测试
    let total = devices.len();
    for (idx, serial) in devices.iter().enumerate() {
        println!("\n╔════════════════════════════════════════════════════════╗");
        println!("║  开始测试设备 [{}/{}]: {}  ║", idx + 1, total, serial);
        println!("╚════════════════════════════════════════════════════════╝\n");

        if let Err(e) = run_test_for_device(serial).await {
            eprintln!("\n❌ 设备 {} 测试失败: {}", serial, e);
        } else {
            println!("\n✅ 设备 {} 测试完成", serial);
        }

        if idx + 1 < total {
            println!("\n--- 等待 3 秒后切换下一台设备 ---");
            sleep(Duration::from_secs(3)).await;
        }
    }

    println!("\n\n=== 所有设备测试结束 ===");
    Ok(())
}

/// 执行 `adb devices -l`，解析并返回所有状态为 `device` 的设备序列号
async fn get_connected_devices() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = tokio::process::Command::new("adb")
        .args(&["devices", "-l"])
        .output()
        .await
        .map_err(|e| format!("无法执行 adb 命令: {}。请确认 adb 已安装并在 PATH 中。", e))?;

    if !output.status.success() {
        return Err(format!(
            "adb devices -l 执行失败: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of devices") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "device" {
            devices.push(parts[0].to_string());
        }
    }

    Ok(devices)
}

/// 对单台设备执行完整的测试
async fn run_test_for_device(serial: &str) -> Result<(), Box<dyn std::error::Error>> {
    let device = AndroidDevice::new(serial);
    let controller = AndroidController::new(device.clone());

    // 1. 创建设备控制器并连接
    println!("正在连接设备: {}", serial);
    device.connect().await?;
    println!("✅ 设备连接成功\n");

    // 2. 获取设备基本信息
    let info = device.get_device_info().await?;
    println!("设备信息:");
    println!("  名称: {}", info.name);
    println!("  型号: {}", info.model);
    println!("  OS 版本: {}", info.os_version);
    println!("  屏幕: {}x{}\n", info.screen_width, info.screen_height);

    // 3. 使用 AndroidController 获取电池电量
    match controller.get_battery_level().await {
        Ok(level) => println!("🔋 电池电量: {}%\n", level),
        Err(e) => println!("⚠️ 获取电池电量失败: {}\n", e),
    }

    // 4. 截图并保存到本地
    println!("正在截图...");
    let screenshot = device.take_screenshot().await?;
    let screenshot_path = format!("android_screenshot_{}.png", serial.replace(':', "_"));
    tokio::fs::write(&screenshot_path, &screenshot).await?;
    println!(
        "✅ 截图已保存到: {} ({} bytes)\n",
        screenshot_path,
        screenshot.len()
    );

    // 5. 基础手势测试
    println!("执行基础手势测试...");

    let center_x = info.screen_width as i32 / 2;
    let center_y = info.screen_height as i32 / 2;
    device.tap(center_x, center_y).await?;
    println!("  👆 点击屏幕中央 ({}, {})", center_x, center_y);

    sleep(Duration::from_millis(500)).await;
    device
        .swipe(
            center_x,
            info.screen_height as i32 * 3 / 4,
            center_x,
            info.screen_height as i32 / 4,
            300,
        )
        .await?;
    println!("  ⬆️ 向上滑动");

    sleep(Duration::from_millis(500)).await;
    device
        .swipe_direction(SwipeDirection::Down, 600, 300)
        .await?;
    println!("  ⬇️ 向下滑动\n");

    // 6. 按键测试
    println!("执行按键测试...");
    device.press_button(HardwareButton::Home).await?;
    println!("  🏠 按 Home 键");
    sleep(Duration::from_millis(800)).await;

    device.press_button(HardwareButton::Back).await?;
    println!("  🔙 按 Back 键\n");
    sleep(Duration::from_millis(800)).await;

    // 7. 应用生命周期测试（以系统设置为例）
    let settings_pkg = "com.android.settings";
    println!("应用生命周期测试（{}）:", settings_pkg);

    let app_info = device.get_app_info(settings_pkg).await?;
    println!("  已安装: {}", app_info.is_installed);

    if app_info.is_installed {
        device.launch_app(settings_pkg).await?;
        println!("  🚀 已启动设置应用");
        sleep(Duration::from_secs(2)).await;

        let running = device.is_app_running(settings_pkg).await?;
        println!("  运行中: {}", running);

        let screenshot2 = device.take_screenshot().await?;
        let settings_name = format!(
            "android_settings_screenshot_{}.png",
            serial.replace(':', "_")
        );
        tokio::fs::write(&settings_name, &screenshot2).await?;
        println!("  ✅ 设置应用截图已保存");

        // 8. UI 元素查找与交互测试
        println!("\nUI 元素查找测试...");

        // 先导出一次 UI 层级，帮助调试
        match controller.dump_ui_hierarchy().await {
            Ok(xml) => {
                // 简单提取 XML 中前 20 个带 text 属性的元素用于调试
                let texts: Vec<String> = xml
                    .lines()
                    .filter_map(|line| {
                        let start = line.find("text=\"")? + 6;
                        let end = line[start..].find("\"")?;
                        let text = &line[start..start + end];
                        if !text.is_empty() {
                            Some(text.to_string())
                        } else {
                            None
                        }
                    })
                    .take(20)
                    .collect();
                if !texts.is_empty() {
                    println!("  当前页面可见文本（前20个）: {:?}", texts);
                }
            }
            Err(e) => println!("  ⚠️ 导出 UI 层级失败: {}", e),
        }

        // 尝试多种常见关键词组合（覆盖不同厂商 ROM 的命名差异）
        let locators = vec![
            (
                "WLAN/Wi-Fi",
                ElementLocator::new(LocatorType::PartialText, "WLAN"),
            ),
            (
                "Wi-Fi",
                ElementLocator::new(LocatorType::PartialText, "Wi-Fi"),
            ),
            (
                "无线网络",
                ElementLocator::new(LocatorType::PartialText, "无线"),
            ),
            (
                "网络",
                ElementLocator::new(LocatorType::PartialText, "网络"),
            ),
            (
                "连接",
                ElementLocator::new(LocatorType::PartialText, "连接"),
            ),
            (
                "蓝牙",
                ElementLocator::new(LocatorType::PartialText, "蓝牙"),
            ),
            (
                "显示",
                ElementLocator::new(LocatorType::PartialText, "显示"),
            ),
            (
                "设置标题",
                ElementLocator::new(LocatorType::PartialText, "设置"),
            ),
        ];

        let mut clicked = false;
        for (name, locator) in locators {
            match device.element_exists(&locator).await {
                Ok(true) => {
                    println!("  ✅ 找到元素 [{}]", name);
                    if let Err(e) = device.tap_element(&locator).await {
                        println!("     点击失败: {}", e);
                    } else {
                        println!("     👆 已点击");
                        clicked = true;
                        sleep(Duration::from_millis(1000)).await;
                        break;
                    }
                }
                Ok(false) => {}
                Err(e) => println!("  ⚠️ 查找元素 [{}] 出错: {}", name, e),
            }
        }

        if !clicked {
            println!("  ⚠️ 未找到预设元素，改用坐标点击屏幕中央作为演示");
            device.tap(center_x, info.screen_height as i32 / 2).await?;
            println!(
                "     👆 已点击屏幕中央 ({}, {})",
                center_x,
                info.screen_height as i32 / 2
            );
            sleep(Duration::from_millis(1000)).await;
        }

        device.press_button(HardwareButton::Home).await?;
        println!("  🏠 返回桌面");
    }

    // 9. 导出当前 UI 层级结构（用于调试）
    println!("\n正在导出 UI 层级结构...");
    match controller.dump_ui_hierarchy().await {
        Ok(xml) => {
            let dump_path = format!("android_ui_dump_{}.xml", serial.replace(':', "_"));
            tokio::fs::write(&dump_path, xml).await?;
            println!("✅ UI 层级已保存到: {}", dump_path);
        }
        Err(e) => println!("⚠️ 导出 UI 层级失败: {}", e),
    }

    // 10. 断开连接
    device.disconnect().await?;
    println!("\n✅ 测试完成，设备已断开");

    Ok(())
}
