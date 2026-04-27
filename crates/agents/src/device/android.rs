//! Android Device Control
//!
//! Provides Android device automation via ADB (Android Debug Bridge).
//! Similar to UIAutomator2 in Appium.
//!
//! # Requirements
//! - ADB installed and in PATH
//! - Device connected via USB or WiFi with debug mode enabled
//!
//! # Example
//! ```rust,no_run
//! use beebotos_agents::device::{AndroidDevice, AppLifecycle, DeviceAutomation};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let device = AndroidDevice::new("emulator-5554");
//! device.connect().await?;
//!
//! // Install and launch app
//! device.install_app("/path/to/app.apk").await?;
//! device.launch_app("com.example.app").await?;
//!
//! // Perform gesture
//! device.tap(500, 800).await?;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, info};

use super::{
    AppInfo, AppLifecycle, DeviceAutomation, DeviceCapability, DeviceInfo, DeviceStatus,
    ElementLocator, HardwareButton, LocatorType, ScreenBounds, Size, SwipeDirection, UiElement,
};
use crate::error::{AgentError, Result};

/// Android device controller
///
/// Provides comprehensive Android device automation using ADB commands.
#[derive(Debug, Clone)]
pub struct AndroidDevice {
    /// Device serial number
    serial: String,
    /// Connection state
    connected: Arc<Mutex<bool>>,
    /// Device info cache
    device_info: Arc<Mutex<Option<DeviceInfo>>>,
    /// ADB path
    adb_path: String,
}

impl AndroidDevice {
    /// Create new Android device controller
    pub fn new(serial: impl Into<String>) -> Self {
        Self {
            serial: serial.into(),
            connected: Arc::new(Mutex::new(false)),
            device_info: Arc::new(Mutex::new(None)),
            adb_path: "adb".to_string(),
        }
    }

    /// Create with custom ADB path
    pub fn with_adb_path(mut self, adb_path: impl Into<String>) -> Self {
        self.adb_path = adb_path.into();
        self
    }

    /// Get device serial
    pub fn serial(&self) -> &str {
        &self.serial
    }

    /// Execute ADB command and return stdout as String
    async fn adb(&self, args: &[&str]) -> Result<String> {
        let output = self.adb_raw(args).await?;
        Ok(String::from_utf8_lossy(&output).to_string())
    }

    /// Execute ADB command and return raw binary stdout
    async fn adb_raw(&self, args: &[&str]) -> Result<Vec<u8>> {
        let mut cmd = TokioCommand::new(&self.adb_path);
        cmd.arg("-s").arg(&self.serial);
        cmd.args(args);

        debug!("Executing ADB command: {:?}", cmd);

        let output = cmd
            .output()
            .await
            .map_err(|e| AgentError::Execution(format!("Failed to execute ADB: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AgentError::Execution(format!("ADB error: {}", stderr)));
        }

        Ok(output.stdout)
    }

    /// Execute ADB shell command
    async fn shell(&self, command: &str) -> Result<String> {
        self.adb(&["shell", command]).await
    }

    /// Get UI hierarchy dump
    async fn get_ui_hierarchy(&self) -> Result<String> {
        // Try uiautomator dump first
        let result = self
            .shell(
                "uiautomator dump /dev/tty 2>/dev/null || cat /sdcard/window_dump.xml 2>/dev/null",
            )
            .await;

        match result {
            Ok(xml) if !xml.is_empty() => Ok(xml),
            _ => {
                // Fallback: try to dump to file and pull
                self.shell("uiautomator dump /sdcard/window_dump.xml")
                    .await
                    .ok();
                sleep(Duration::from_millis(500)).await;

                // Pull the file content
                let content = self.shell("cat /sdcard/window_dump.xml").await?;
                Ok(content)
            }
        }
    }

    /// Parse element bounds from string like "[100,200][300,400]"
    fn parse_bounds(&self, bounds_str: &str) -> Option<ScreenBounds> {
        let parts: Vec<&str> = bounds_str.split(']').filter(|s| !s.is_empty()).collect();
        if parts.len() != 2 {
            return None;
        }

        let left: Vec<i32> = parts[0]
            .trim_start_matches('[')
            .split(',')
            .filter_map(|s| s.parse().ok())
            .collect();

        let right: Vec<i32> = parts[1]
            .trim_start_matches('[')
            .split(',')
            .filter_map(|s| s.parse().ok())
            .collect();

        if left.len() == 2 && right.len() == 2 {
            Some(ScreenBounds::new(
                left[0],
                left[1],
                (right[0] - left[0]) as u32,
                (right[1] - left[1]) as u32,
            ))
        } else {
            None
        }
    }

    /// Find element in UI hierarchy
    async fn find_element_in_hierarchy(&self, locator: &ElementLocator) -> Result<UiElement> {
        let hierarchy = self.get_ui_hierarchy().await?;

        // Parse XML and find element
        // This is a simplified implementation - in production, use proper XML parsing
        let elements = self.parse_elements(&hierarchy).await?;

        for element in elements {
            if self.matches_locator(&element, locator) {
                return Ok(element);
            }
        }

        Err(AgentError::Execution(format!(
            "Element not found: {:?} = {}",
            locator.locator_type, locator.value
        )))
    }

    /// Parse UI elements from hierarchy XML
    async fn parse_elements(&self, hierarchy: &str) -> Result<Vec<UiElement>> {
        let mut elements = Vec::new();

        // Simple regex-like parsing for node elements
        // In production, use a proper XML parser like quick-xml or serde_xml_rs
        for line in hierarchy.lines() {
            if line.contains("node") {
                let element = self.parse_node_line(line).await?;
                if let Some(elem) = element {
                    elements.push(elem);
                }
            }
        }

        Ok(elements)
    }

    /// Parse a single node line
    async fn parse_node_line(&self, line: &str) -> Result<Option<UiElement>> {
        // Extract attributes from the node line
        let id = self.extract_attr(line, "resource-id");
        let text = self.extract_attr(line, "text");
        let description = self.extract_attr(line, "content-desc");
        let class_name = self.extract_attr(line, "class");
        let bounds_str = self.extract_attr(line, "bounds").unwrap_or_default();
        let clickable = line.contains("clickable=\"true\"");
        let enabled = !line.contains("enabled=\"false\"");
        let visible = !line.contains("visibility=\"invisible\"");
        let focusable = line.contains("focusable=\"true\"");

        let bounds = if let Some(b) = self.parse_bounds(&bounds_str) {
            b
        } else {
            return Ok(None);
        };

        Ok(Some(UiElement {
            id,
            text,
            description,
            class_name,
            bounds,
            enabled,
            visible,
            clickable,
            focusable,
        }))
    }

    /// Extract attribute value from XML line
    fn extract_attr(&self, line: &str, attr: &str) -> Option<String> {
        let pattern = format!("{}=\"", attr);
        if let Some(start) = line.find(&pattern) {
            let start = start + pattern.len();
            if let Some(end) = line[start..].find('"') {
                return Some(line[start..start + end].to_string());
            }
        }
        None
    }

    /// Check if element matches locator
    fn matches_locator(&self, element: &UiElement, locator: &ElementLocator) -> bool {
        match locator.locator_type {
            LocatorType::Id => element.id.as_ref() == Some(&locator.value),
            LocatorType::Text => element.text.as_ref() == Some(&locator.value),
            LocatorType::PartialText => element
                .text
                .as_ref()
                .map(|t| t.contains(&locator.value))
                .unwrap_or(false),
            LocatorType::AccessibilityId => element.description.as_ref() == Some(&locator.value),
            LocatorType::ClassName => element.class_name.as_ref() == Some(&locator.value),
            LocatorType::XPath => {
                // Basic XPath pattern matching (simplified implementation)
                // Supports patterns like:
                // - //ClassName
                // - //*[contains(@text, "...")]
                // - //View[@resource-id="..."]
                self.matches_xpath(element, &locator.value)
            }
            _ => false,
        }
    }

    /// Basic XPath pattern matching
    fn matches_xpath(&self, element: &UiElement, xpath: &str) -> bool {
        let xpath = xpath.trim();

        // Handle //ClassName pattern
        if let Some(class_name) = xpath.strip_prefix("//") {
            if element.class_name.as_ref() == Some(&class_name.to_string()) {
                return true;
            }
        }

        // Handle //*[contains(@text, "...")] pattern
        if xpath.contains("contains(@text,") {
            if let Some(start) = xpath.find("\"") {
                if let Some(end) = xpath[start + 1..].find("\"") {
                    let text = &xpath[start + 1..start + 1 + end];
                    return element
                        .text
                        .as_ref()
                        .map(|t| t.contains(text))
                        .unwrap_or(false);
                }
            }
        }

        // Handle //View[@resource-id="..."] pattern
        if xpath.contains("[@resource-id=\"") {
            if let Some(start) = xpath.find("@resource-id=\"") {
                let start = start + 14; // Length of '@resource-id="'
                if let Some(end) = xpath[start..].find("\"") {
                    let id = &xpath[start..start + end];
                    return element.id.as_ref() == Some(&id.to_string());
                }
            }
        }

        // Handle //View[@text="..."] pattern
        if xpath.contains("[@text=\"") {
            if let Some(start) = xpath.find("[@text=\"") {
                let start = start + 8; // Length of '[@text="'
                if let Some(end) = xpath[start..].find("\"") {
                    let text = &xpath[start..start + end];
                    return element.text.as_ref() == Some(&text.to_string());
                }
            }
        }

        false
    }

    /// Wait for device to be ready
    async fn wait_for_device(&self, timeout_secs: u64) -> Result<()> {
        let result = timeout(
            Duration::from_secs(timeout_secs),
            self.adb(&["wait-for-device"]),
        )
        .await;

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AgentError::Execution("Device wait timeout".to_string())),
        }
    }

    /// Get screen dimensions
    async fn get_screen_dimensions(&self) -> Result<(u32, u32)> {
        let output = self.shell("wm size").await?;

        // Parse "Physical size: 1080x1920"
        if let Some(size_part) = output.split("Physical size:").nth(1) {
            let size_str = size_part.trim();
            let dims: Vec<&str> = size_str.split('x').collect();
            if dims.len() == 2 {
                let width = dims[0]
                    .parse()
                    .map_err(|_| AgentError::Execution("Invalid width".to_string()))?;
                let height = dims[1]
                    .parse()
                    .map_err(|_| AgentError::Execution("Invalid height".to_string()))?;
                return Ok((width, height));
            }
        }

        Err(AgentError::Execution(
            "Failed to get screen dimensions".to_string(),
        ))
    }

    /// Dump screen to file (for debugging)
    pub async fn dump_screen(&self, output_path: &str) -> Result<()> {
        let hierarchy = self.get_ui_hierarchy().await?;
        tokio::fs::write(output_path, hierarchy)
            .await
            .map_err(|e| AgentError::Execution(format!("Failed to write dump: {}", e)))?;
        Ok(())
    }
}

#[async_trait]
impl DeviceAutomation for AndroidDevice {
    async fn connect(&self) -> Result<()> {
        info!("Connecting to Android device: {}", self.serial);

        // Check if device is available
        let devices = self.adb(&["devices", "-l"]).await?;
        if !devices.contains(&self.serial) {
            return Err(AgentError::Execution(format!(
                "Device {} not found in ADB device list",
                self.serial
            )));
        }

        // Wait for device
        self.wait_for_device(30).await?;

        // Set connected state
        let mut connected = self.connected.lock().await;
        *connected = true;
        drop(connected);

        // Cache device info
        let info = self.get_device_info().await?;
        let mut device_info = self.device_info.lock().await;
        *device_info = Some(info);

        info!("Successfully connected to Android device: {}", self.serial);
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting from Android device: {}", self.serial);

        let mut connected = self.connected.lock().await;
        *connected = false;

        Ok(())
    }

    async fn is_connected(&self) -> bool {
        *self.connected.lock().await
    }

    async fn get_device_info(&self) -> Result<DeviceInfo> {
        // Check cache first
        if let Some(ref info) = *self.device_info.lock().await {
            return Ok(info.clone());
        }

        // Get device properties
        let model = self
            .shell("getprop ro.product.model")
            .await?
            .trim()
            .to_string();
        let version = self
            .shell("getprop ro.build.version.release")
            .await?
            .trim()
            .to_string();
        let (width, height) = self.get_screen_dimensions().await?;

        let info = DeviceInfo {
            id: self.serial.clone(),
            name: model.clone(),
            model,
            os_version: version,
            status: DeviceStatus::Ready,
            capabilities: vec![DeviceCapability::Touchscreen, DeviceCapability::Screenshot],
            screen_width: width,
            screen_height: height,
            dpi: 320, // Default DPI
        };

        Ok(info)
    }

    async fn get_status(&self) -> DeviceStatus {
        if !self.is_connected().await {
            return DeviceStatus::Disconnected;
        }

        match self.shell("echo ping").await {
            Ok(_) => DeviceStatus::Ready,
            Err(_) => DeviceStatus::Error,
        }
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        debug!("Taking screenshot on Android device {}", self.serial);

        // Use screencap command
        self.shell("screencap -p /sdcard/screenshot.png").await?;

        // Pull the screenshot as raw binary data
        let output = self
            .adb_raw(&["exec-out", "cat", "/sdcard/screenshot.png"])
            .await?;

        // Clean up temporary file (best effort, ignore errors)
        self.shell("rm /sdcard/screenshot.png").await.ok();

        Ok(output)
    }

    async fn tap(&self, x: i32, y: i32) -> Result<()> {
        debug!(
            "Tapping at ({}, {}) on Android device {}",
            x, y, self.serial
        );
        self.shell(&format!("input tap {} {}", x, y)).await?;
        Ok(())
    }

    async fn long_press(&self, x: i32, y: i32, duration_ms: u64) -> Result<()> {
        debug!("Long pressing at ({}, {}) for {}ms", x, y, duration_ms);
        // Use swipe with zero distance to simulate long press
        self.shell(&format!(
            "input swipe {} {} {} {} {}",
            x, y, x, y, duration_ms
        ))
        .await?;
        Ok(())
    }

    async fn swipe(
        &self,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        duration_ms: u64,
    ) -> Result<()> {
        debug!(
            "Swiping from ({}, {}) to ({}, {})",
            from_x, from_y, to_x, to_y
        );
        self.shell(&format!(
            "input swipe {} {} {} {} {}",
            from_x, from_y, to_x, to_y, duration_ms
        ))
        .await?;
        Ok(())
    }

    async fn swipe_direction(
        &self,
        direction: SwipeDirection,
        distance: u32,
        duration_ms: u64,
    ) -> Result<()> {
        let (width, height) = self.get_screen_dimensions().await?;
        let center_x = width as i32 / 2;
        let center_y = height as i32 / 2;
        let distance = distance as i32;

        let (from_x, from_y, to_x, to_y) = match direction {
            SwipeDirection::Up => (
                center_x,
                center_y + distance / 2,
                center_x,
                center_y - distance / 2,
            ),
            SwipeDirection::Down => (
                center_x,
                center_y - distance / 2,
                center_x,
                center_y + distance / 2,
            ),
            SwipeDirection::Left => (
                center_x + distance / 2,
                center_y,
                center_x - distance / 2,
                center_y,
            ),
            SwipeDirection::Right => (
                center_x - distance / 2,
                center_y,
                center_x + distance / 2,
                center_y,
            ),
        };

        self.swipe(from_x, from_y, to_x, to_y, duration_ms).await
    }

    async fn pinch(
        &self,
        center_x: i32,
        center_y: i32,
        scale: f64,
        duration_ms: u64,
    ) -> Result<()> {
        // Android doesn't have native pinch gesture, simulate with two-finger swipe
        // For scale > 1.0 (zoom in): fingers move outward
        // For scale < 1.0 (zoom out): fingers move inward
        let base_distance = 150; // Base distance from center
        let distance = (base_distance as f64 * scale.abs()) as i32;

        // Calculate start and end positions for two fingers
        let (finger1_start, finger1_end, finger2_start, finger2_end) = if scale > 1.0 {
            // Zoom in: start close, end far
            let start_dist = base_distance / 2;
            let end_dist = distance;
            (
                (center_x - start_dist, center_y),
                (center_x - end_dist, center_y),
                (center_x + start_dist, center_y),
                (center_x + end_dist, center_y),
            )
        } else {
            // Zoom out: start far, end close
            let start_dist = distance;
            let end_dist = base_distance / 2;
            (
                (center_x - start_dist, center_y),
                (center_x - end_dist, center_y),
                (center_x + start_dist, center_y),
                (center_x + end_dist, center_y),
            )
        };

        // Use input swipe for both fingers (sequential, not simultaneous - limitation)
        debug!(
            "Pinch gesture at ({}, {}) with scale {} (duration: {}ms)",
            center_x, center_y, scale, duration_ms
        );

        // Execute first finger swipe
        self.shell(&format!(
            "input swipe {} {} {} {} {}",
            finger1_start.0, finger1_start.1, finger1_end.0, finger1_end.1, duration_ms
        ))
        .await?;

        // Small delay between fingers
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Execute second finger swipe
        self.shell(&format!(
            "input swipe {} {} {} {} {}",
            finger2_start.0, finger2_start.1, finger2_end.0, finger2_end.1, duration_ms
        ))
        .await?;

        Ok(())
    }

    async fn find_element(&self, locator: &ElementLocator) -> Result<UiElement> {
        debug!(
            "Finding element: {:?} = {}",
            locator.locator_type, locator.value
        );

        // Use timeout
        let result = timeout(
            Duration::from_millis(locator.timeout_ms),
            self.find_element_in_hierarchy(locator),
        )
        .await;

        match result {
            Ok(element) => element,
            Err(_) => Err(AgentError::Execution(format!(
                "Timeout finding element: {:?} = {}",
                locator.locator_type, locator.value
            ))),
        }
    }

    async fn find_elements(&self, locator: &ElementLocator) -> Result<Vec<UiElement>> {
        let hierarchy = self.get_ui_hierarchy().await?;
        let elements = self.parse_elements(&hierarchy).await?;

        let matching: Vec<UiElement> = elements
            .into_iter()
            .filter(|e| self.matches_locator(e, locator))
            .collect();

        Ok(matching)
    }

    async fn tap_element(&self, locator: &ElementLocator) -> Result<()> {
        let element = self.find_element(locator).await?;
        let center = element.bounds.center();
        self.tap(center.x, center.y).await
    }

    async fn long_press_element(&self, locator: &ElementLocator, duration_ms: u64) -> Result<()> {
        let element = self.find_element(locator).await?;
        let center = element.bounds.center();
        self.long_press(center.x, center.y, duration_ms).await
    }

    async fn get_element_text(&self, locator: &ElementLocator) -> Result<String> {
        let element = self.find_element(locator).await?;
        element
            .text
            .ok_or_else(|| AgentError::Execution("Element has no text".to_string()))
    }

    async fn set_element_text(&self, locator: &ElementLocator, text: &str) -> Result<()> {
        // First tap the element to focus
        self.tap_element(locator).await?;
        sleep(Duration::from_millis(200)).await;

        // Clear existing text
        self.shell("input keyevent KEYCODE_CLEAR").await.ok();

        // Type new text
        self.type_text(text).await
    }

    async fn clear_element_text(&self, locator: &ElementLocator) -> Result<()> {
        self.tap_element(locator).await?;
        sleep(Duration::from_millis(200)).await;

        // Select all and delete
        self.shell("input keyevent KEYCODE_CTRL_LEFT KEYCODE_A")
            .await
            .ok();
        self.shell("input keyevent KEYCODE_DEL").await?;

        Ok(())
    }

    async fn element_exists(&self, locator: &ElementLocator) -> Result<bool> {
        match self.find_element(locator).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn wait_for_element(&self, locator: &ElementLocator) -> Result<UiElement> {
        self.find_element(locator).await
    }

    async fn press_button(&self, button: HardwareButton) -> Result<()> {
        let keycode = match button {
            HardwareButton::Power => "KEYCODE_POWER",
            HardwareButton::VolumeUp => "KEYCODE_VOLUME_UP",
            HardwareButton::VolumeDown => "KEYCODE_VOLUME_DOWN",
            HardwareButton::Home => "KEYCODE_HOME",
            HardwareButton::Back => "KEYCODE_BACK",
            HardwareButton::Menu => "KEYCODE_MENU",
            HardwareButton::Search => "KEYCODE_SEARCH",
            HardwareButton::Camera => "KEYCODE_CAMERA",
        };

        self.shell(&format!("input keyevent {}", keycode)).await?;
        Ok(())
    }

    async fn type_text(&self, text: &str) -> Result<()> {
        // Escape special characters for shell
        let escaped = text.replace('"', "\\\"");
        self.shell(&format!("input text \"{}\"", escaped)).await?;
        Ok(())
    }

    async fn get_screen_size(&self) -> Result<Size> {
        let (width, height) = self.get_screen_dimensions().await?;
        Ok(Size::new(width, height))
    }

    async fn go_back(&self) -> Result<()> {
        self.press_button(HardwareButton::Back).await
    }

    async fn go_home(&self) -> Result<()> {
        self.press_button(HardwareButton::Home).await
    }

    async fn open_recents(&self) -> Result<()> {
        self.shell("input keyevent KEYCODE_APP_SWITCH").await?;
        Ok(())
    }
}

#[async_trait]
impl AppLifecycle for AndroidDevice {
    async fn install_app(&self, app_path: &str) -> Result<()> {
        info!("Installing APK: {}", app_path);
        self.adb(&["install", "-r", app_path]).await?;
        Ok(())
    }

    async fn uninstall_app(&self, package_name: &str) -> Result<()> {
        info!("Uninstalling app: {}", package_name);
        self.adb(&["uninstall", package_name]).await?;
        Ok(())
    }

    async fn launch_app(&self, package_name: &str) -> Result<()> {
        info!("Launching app: {}", package_name);
        // Launch the main activity
        self.shell(&format!(
            "monkey -p {} -c android.intent.category.LAUNCHER 1",
            package_name
        ))
        .await?;
        Ok(())
    }

    async fn launch_app_with_activity(&self, package_name: &str, activity: &str) -> Result<()> {
        info!("Launching activity: {}/{}", package_name, activity);
        self.shell(&format!("am start -n {}/{}", package_name, activity))
            .await?;
        Ok(())
    }

    async fn close_app(&self, package_name: &str) -> Result<()> {
        info!("Closing app: {}", package_name);
        self.shell(&format!("am force-stop {}", package_name))
            .await?;
        Ok(())
    }

    async fn is_app_installed(&self, package_name: &str) -> Result<bool> {
        // Use `|| true` to prevent grep from returning non-zero exit code when no match
        // is found, which would cause the adb shell command to fail.
        let output = self
            .shell(&format!("pm list packages | grep {} || true", package_name))
            .await?;
        Ok(output.contains(package_name))
    }

    async fn is_app_running(&self, package_name: &str) -> Result<bool> {
        // Use `|| true` to prevent grep from returning non-zero exit code when no match
        // is found.
        let output = self
            .shell(&format!("ps | grep {} || true", package_name))
            .await?;
        Ok(output.contains(package_name))
    }

    async fn get_app_info(&self, package_name: &str) -> Result<AppInfo> {
        // Get version name
        let version = self
            .shell(&format!(
                "dumpsys package {} | grep versionName",
                package_name
            ))
            .await
            .unwrap_or_default();

        let is_installed = self.is_app_installed(package_name).await?;
        let is_running = self.is_app_running(package_name).await?;

        Ok(AppInfo {
            package_name: package_name.to_string(),
            app_name: package_name.to_string(), // Could extract from dumpsys
            version_name: version.trim().to_string(),
            version_code: "unknown".to_string(),
            is_installed,
            is_running,
        })
    }

    async fn get_installed_apps(&self) -> Result<Vec<AppInfo>> {
        let output = self.shell("pm list packages").await?;
        let mut apps = Vec::new();

        for line in output.lines() {
            if let Some(pkg) = line.strip_prefix("package:") {
                if let Ok(info) = self.get_app_info(pkg).await {
                    apps.push(info);
                }
            }
        }

        Ok(apps)
    }

    async fn clear_app_data(&self, package_name: &str) -> Result<()> {
        info!("Clearing app data: {}", package_name);
        self.shell(&format!("pm clear {}", package_name)).await?;
        Ok(())
    }

    async fn grant_permission(&self, package_name: &str, permission: &str) -> Result<()> {
        info!("Granting permission {} to {}", permission, package_name);
        self.shell(&format!("pm grant {} {}", package_name, permission))
            .await?;
        Ok(())
    }

    async fn revoke_permission(&self, package_name: &str, permission: &str) -> Result<()> {
        info!("Revoking permission {} from {}", permission, package_name);
        self.shell(&format!("pm revoke {} {}", package_name, permission))
            .await?;
        Ok(())
    }
}

/// Android-specific controller with additional features
pub struct AndroidController {
    device: AndroidDevice,
}

impl AndroidController {
    /// Create new Android controller
    pub fn new(device: AndroidDevice) -> Self {
        Self { device }
    }

    /// Get underlying device
    pub fn device(&self) -> &AndroidDevice {
        &self.device
    }

    /// Input key event
    pub async fn input_keyevent(&self, keycode: &str) -> Result<()> {
        self.device
            .shell(&format!("input keyevent {}", keycode))
            .await
            .map(|_| ())
    }

    /// Input text
    pub async fn input_text(&self, text: &str) -> Result<()> {
        self.device.type_text(text).await
    }

    /// Start activity
    pub async fn start_activity(&self, package: &str, activity: &str) -> Result<()> {
        self.device
            .launch_app_with_activity(package, activity)
            .await
    }

    /// Broadcast intent
    pub async fn broadcast_intent(&self, action: &str) -> Result<()> {
        self.device
            .shell(&format!("am broadcast -a {}", action))
            .await
            .map(|_| ())
    }

    /// Open a URL or deep link via ACTION_VIEW
    pub async fn open_url(&self, url: &str) -> Result<()> {
        self.device
            .shell(&format!(
                "am start -a android.intent.action.VIEW -d '{}'",
                url
            ))
            .await
            .map(|_| ())
    }

    /// Get logcat
    pub async fn get_logcat(&self, lines: usize) -> Result<String> {
        self.device
            .adb(&["logcat", "-d", "-t", &lines.to_string()])
            .await
    }

    /// Clear logcat
    pub async fn clear_logcat(&self) -> Result<()> {
        self.device.adb(&["logcat", "-c"]).await.map(|_| ())
    }

    /// Push file to device
    pub async fn push_file(&self, local: &str, remote: &str) -> Result<()> {
        self.device.adb(&["push", local, remote]).await.map(|_| ())
    }

    /// Pull file from device
    pub async fn pull_file(&self, remote: &str, local: &str) -> Result<()> {
        self.device.adb(&["pull", remote, local]).await.map(|_| ())
    }

    /// Enable/disable WiFi
    pub async fn set_wifi(&self, enabled: bool) -> Result<()> {
        let state = if enabled { "enable" } else { "disable" };
        self.device
            .shell(&format!("svc wifi {}", state))
            .await
            .map(|_| ())
    }

    /// Enable/disable mobile data
    pub async fn set_mobile_data(&self, enabled: bool) -> Result<()> {
        let state = if enabled { "enable" } else { "disable" };
        self.device
            .shell(&format!("svc data {}", state))
            .await
            .map(|_| ())
    }

    /// Set airplane mode
    pub async fn set_airplane_mode(&self, enabled: bool) -> Result<()> {
        let state = if enabled { "enable" } else { "disable" };
        self.device
            .shell(&format!("cmd connectivity airplane-mode {}", state))
            .await
            .map(|_| ())
    }

    /// Get battery level
    pub async fn get_battery_level(&self) -> Result<u8> {
        let output = self.device.shell("dumpsys battery | grep level").await?;
        // Parse "level: 85"
        if let Some(level_str) = output.split(':').nth(1) {
            let level: u8 = level_str
                .trim()
                .parse()
                .map_err(|_| AgentError::Execution("Invalid battery level".to_string()))?;
            return Ok(level);
        }
        Err(AgentError::Execution(
            "Failed to parse battery level".to_string(),
        ))
    }

    /// Is device charging
    pub async fn is_charging(&self) -> Result<bool> {
        let output = self.device.shell("dumpsys battery | grep status").await?;
        Ok(output.contains("2") || output.contains("CHARGING"))
    }

    /// Get device orientation
    pub async fn get_orientation(&self) -> Result<u8> {
        let output = self
            .device
            .shell("dumpsys input | grep 'SurfaceOrientation'")
            .await?;
        // Parse orientation
        if let Some(ori_str) = output.split_whitespace().last() {
            let ori: u8 = ori_str
                .parse()
                .map_err(|_| AgentError::Execution("Invalid orientation".to_string()))?;
            return Ok(ori);
        }
        Ok(0) // Default portrait
    }

    /// Set device orientation
    pub async fn set_orientation(&self, orientation: u8) -> Result<()> {
        self.device
            .shell(&format!(
                "content insert --uri content://settings/system --bind name:s:user_rotation \
                 --bind value:i:{}",
                orientation
            ))
            .await
            .map(|_| ())
    }

    /// Dump UI hierarchy for debugging
    pub async fn dump_ui_hierarchy(&self) -> Result<String> {
        self.device.get_ui_hierarchy().await
    }

    /// Execute arbitrary shell command on the device
    pub async fn shell_command(&self, command: &str) -> Result<String> {
        self.device.shell(command).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_android_device_creation() {
        let device = AndroidDevice::new("emulator-5554");
        assert_eq!(device.serial(), "emulator-5554");
    }

    #[test]
    fn test_android_device_with_adb_path() {
        let device = AndroidDevice::new("emulator-5554").with_adb_path("/usr/local/bin/adb");
        assert_eq!(device.adb_path, "/usr/local/bin/adb");
    }

    #[tokio::test]
    async fn test_parse_bounds() {
        let device = AndroidDevice::new("test");

        // Valid bounds
        let bounds = device.parse_bounds("[100,200][300,400]");
        assert!(bounds.is_some());
        let b = bounds.unwrap();
        assert_eq!(b.x, 100);
        assert_eq!(b.y, 200);
        assert_eq!(b.width, 200);
        assert_eq!(b.height, 200);

        // Invalid bounds
        assert!(device.parse_bounds("invalid").is_none());
    }

    #[test]
    fn test_extract_attr() {
        let device = AndroidDevice::new("test");
        let line = r#"<node text="Hello" resource-id="btn1" />"#;

        assert_eq!(device.extract_attr(line, "text"), Some("Hello".to_string()));
        assert_eq!(
            device.extract_attr(line, "resource-id"),
            Some("btn1".to_string())
        );
        assert_eq!(device.extract_attr(line, "nonexistent"), None);
    }
}
