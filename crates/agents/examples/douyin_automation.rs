//! 抖音（Douyin）自动化测试示例
//!
//! 前置条件：手机上必须已安装抖音（包名: com.ss.android.ugc.aweme）
//!
//! 测试流程：
//! 1. 自动获取所有通过 ADB 连接且状态为 device 的安卓设备
//! 2. 检查抖音是否已安装，未安装则报错跳过
//! 3. 启动抖音，处理启动权限弹窗和常见弹窗
//! 4. 自动化浏览：上滑切换视频、截图、获取界面元素
//! 5. 导出 UI 层级和日志用于分析
//!
//! 运行方式：
//! ```bash
//! cargo run -p beebotos-agents --example douyin_automation
//! ```

use std::time::Duration;

use beebotos_agents::device::{
    AndroidController, AndroidDevice, AppLifecycle, DeviceAutomation, ElementLocator,
    HardwareButton, LocatorType,
};
use tokio::time::sleep;

// ===================== 配置区 =====================
/// 抖音包名
const DOUYIN_PKG: &str = "com.ss.android.ugc.aweme";
// =================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 抖音自动化测试 ===\n");

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

/// 对单台设备执行完整的抖音自动化测试
async fn run_test_for_device(serial: &str) -> Result<(), Box<dyn std::error::Error>> {
    let device = AndroidDevice::new(serial);
    let controller = AndroidController::new(device.clone());

    // 1. 连接设备
    println!("[1/5] 连接设备: {}", serial);
    device.connect().await?;
    println!("✅ 设备连接成功\n");

    // 2. 检查抖音是否已安装
    println!("[2/5] 检查抖音安装状态...");
    let installed = device.is_app_installed(DOUYIN_PKG).await?;
    println!("   抖音已安装: {}", installed);

    if !installed {
        return Err(format!(
            "抖音未安装，请先手动安装抖音（包名: {}）后再运行测试。",
            DOUYIN_PKG
        )
        .into());
    }
    println!("   抖音已安装，继续测试\n");

    // 3. 启动抖音
    println!("[3/5] 启动抖音...");
    device.launch_app(DOUYIN_PKG).await?;
    println!("   🚀 已发送启动命令");

    // 等待启动并处理启动弹窗
    sleep(Duration::from_secs(3)).await;
    handle_startup_popups(&device).await?;

    // 截图记录启动后状态
    let screenshot = device.take_screenshot().await?;
    let startup_name = format!("douyin_startup_{}.png", serial.replace(':', "_"));
    tokio::fs::write(&startup_name, &screenshot).await?;
    println!("   ✅ 启动后截图已保存: {}", startup_name);

    // 4. 自动化浏览测试
    println!("\n[4/5] 执行自动化浏览测试...");
    run_browse_test(&device, serial).await?;

    // 5. 导出调试信息并收尾
    println!("\n[5/5] 导出调试信息...");
    export_debug_info(&device, &controller, serial).await?;

    // 返回桌面并断开
    device.press_button(HardwareButton::Home).await?;
    device.disconnect().await?;

    Ok(())
}

/// 处理抖音启动时的常见系统弹窗和协议弹窗
async fn handle_startup_popups(device: &AndroidDevice) -> Result<(), Box<dyn std::error::Error>> {
    println!("   处理启动弹窗...");

    let popup_handlers = vec![
        (
            "允许",
            vec!["允许", "同意", "确定", "好的", "我知道了", "进入"],
        ),
        (
            "协议",
            vec!["同意并继续", "同意协议", "已阅读并同意", "同意"],
        ),
        (
            "权限-位置",
            vec!["仅使用期间允许", "仅此次允许", "允许一次", "允许"],
        ),
        ("权限-存储", vec!["允许访问", "允许"]),
        ("权限-通知", vec!["允许", "开启", "确定"]),
        ("权限-相机", vec!["仅使用期间允许", "允许"]),
        ("权限-麦克风", vec!["仅使用期间允许", "允许"]),
        ("青少年模式", vec!["我知道了", "关闭", "暂不开启"]),
        ("更新提示", vec!["稍后", "取消", "以后再说", "忽略"]),
        ("登录提示", vec!["暂不登录", "跳过", "稍后", "取消"]),
    ];

    for attempt in 1..=4 {
        println!("   第 {} 轮弹窗检测...", attempt);
        let mut clicked = false;

        for (_popup_type, texts) in &popup_handlers {
            for text in texts {
                let locator =
                    ElementLocator::new(LocatorType::PartialText, *text).with_timeout(2000);
                if device.element_exists(&locator).await.unwrap_or(false) {
                    println!("     检测到按钮 \"{}\"，尝试点击...", text);
                    if let Err(e) = device.tap_element(&locator).await {
                        println!("     点击失败: {}", e);
                    } else {
                        println!("     ✅ 已点击 \"{}\"", text);
                        clicked = true;
                        sleep(Duration::from_millis(800)).await;
                    }
                }
            }
        }

        if !clicked {
            println!("   未检测到更多弹窗");
            break;
        }

        sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}

/// 自动化浏览：上滑浏览视频、截图
async fn run_browse_test(
    device: &AndroidDevice,
    serial: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let size = device.get_screen_size().await?;
    let center_x = size.width as i32 / 2;
    let start_y = size.height as i32 * 3 / 4;
    let end_y = size.height as i32 / 4;

    println!(
        "   屏幕尺寸: {}x{}，开始模拟上滑浏览视频",
        size.width, size.height
    );

    for i in 1..=5 {
        println!("\n   --- 视频 {} ---", i);

        match device.take_screenshot().await {
            Ok(img) => {
                let filename = format!("douyin_video_{}_{}.png", serial.replace(':', "_"), i);
                tokio::fs::write(&filename, &img).await?;
                println!("   ✅ 截图已保存: {}", filename);
            }
            Err(e) => println!("   ⚠️ 截图失败: {}", e),
        }

        for keyword in &["推荐", "关注", "朋友", "商城", "搜索"] {
            match device
                .element_exists(
                    &ElementLocator::new(LocatorType::PartialText, *keyword).with_timeout(1000),
                )
                .await
            {
                Ok(true) => {
                    println!("   📍 检测到\"{}\"标签页", keyword);
                    break;
                }
                _ => {}
            }
        }

        println!("   👆 上滑切换下一个视频");
        device
            .swipe(center_x, start_y, center_x, end_y, 300)
            .await?;

        let watch_time = 3000 + i * 500;
        sleep(Duration::from_millis(watch_time)).await;
    }

    println!("\n   浏览测试完成，共浏览 5 个视频");
    Ok(())
}

/// 导出 UI 层级和 logcat 用于后续分析
async fn export_debug_info(
    device: &AndroidDevice,
    controller: &AndroidController,
    serial: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match controller.dump_ui_hierarchy().await {
        Ok(xml) => {
            let filename = format!("douyin_ui_dump_{}.xml", serial.replace(':', "_"));
            tokio::fs::write(&filename, xml).await?;
            println!("   ✅ UI 层级已保存: {}", filename);
        }
        Err(e) => println!("   ⚠️ UI 层级导出失败: {}", e),
    }

    match controller.get_logcat(200).await {
        Ok(logs) => {
            let douyin_logs: String = logs
                .lines()
                .filter(|line| {
                    line.to_lowercase().contains("douyin")
                        || line.to_lowercase().contains("aweme")
                        || line.contains(DOUYIN_PKG)
                })
                .collect::<Vec<_>>()
                .join("\n");

            let log_content = if douyin_logs.is_empty() {
                format!(
                    "// 未过滤到抖音专属日志，以下是完整最近 200 行日志:\n{}",
                    logs
                )
            } else {
                douyin_logs
            };

            let filename = format!("douyin_logcat_{}.txt", serial.replace(':', "_"));
            tokio::fs::write(&filename, log_content).await?;
            println!("   ✅ Logcat 已保存: {}", filename);
        }
        Err(e) => println!("   ⚠️ Logcat 获取失败: {}", e),
    }

    match device.get_app_info(DOUYIN_PKG).await {
        Ok(info) => {
            println!(
                "   ℹ️ 抖音应用信息: 版本={}, 运行中={}",
                info.version_name, info.is_running
            );
        }
        Err(e) => println!("   ⚠️ 获取应用信息失败: {}", e),
    }

    Ok(())
}
