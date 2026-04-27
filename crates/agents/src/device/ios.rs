//! iOS Device Control
//!
//! Provides iOS device automation via WebDriverAgent (WDA).
//! Similar to XCUITest in Appium.
//!
//! # Requirements
//! - WebDriverAgent installed on the device
//! - Device connected via USB with developer mode enabled
//! - xcuitrunner or go-ios for USB communication
//!
//! # Example
//! ```rust,no_run
//! use beebotos_agents::device::{AppLifecycle, DeviceAutomation, IosDevice};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let device = IosDevice::new("00008030-001234567890ABCD");
//! device.connect().await?;
//!
//! // Install and launch app
//! device.install_app("/path/to/app.ipa").await?;
//! device.launch_app("com.example.app").await?;
//!
//! // Perform gesture
//! device.tap(200, 400).await?;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, info};

use super::{
    AppInfo, AppLifecycle, DeviceAutomation, DeviceCapability, DeviceInfo, DeviceStatus,
    ElementLocator, HardwareButton, LocatorType, ScreenBounds, Size, SwipeDirection, UiElement,
};
use crate::error::{AgentError, Result};

/// WebDriverAgent session response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WdaSession {
    #[serde(rename = "sessionId")]
    session_id: String,
}

/// WebDriverAgent element response
#[derive(Debug, Deserialize)]
struct WdaElement {
    #[serde(rename = "ELEMENT")]
    element_id: String,
}

/// WebDriverAgent elements response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WdaElements {
    value: Vec<WdaElement>,
}

/// WebDriverAgent value response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WdaValue<T> {
    value: T,
}

/// iOS device controller
///
/// Provides comprehensive iOS device automation using WebDriverAgent.
#[derive(Debug, Clone)]
pub struct IosDevice {
    /// Device UDID
    udid: String,
    /// WebDriverAgent URL
    wda_url: String,
    /// Active session ID
    session_id: Arc<Mutex<Option<String>>>,
    /// Connection state
    connected: Arc<Mutex<bool>>,
    /// Device info cache
    device_info: Arc<Mutex<Option<DeviceInfo>>>,
    /// HTTP client
    client: Client,
}

impl IosDevice {
    /// Create new iOS device controller
    pub fn new(udid: impl Into<String>) -> Self {
        Self {
            udid: udid.into(),
            wda_url: "http://localhost:8100".to_string(),
            session_id: Arc::new(Mutex::new(None)),
            connected: Arc::new(Mutex::new(false)),
            device_info: Arc::new(Mutex::new(None)),
            client: Client::new(),
        }
    }

    /// Create with custom WDA URL
    pub fn with_wda_url(mut self, url: impl Into<String>) -> Self {
        self.wda_url = url.into();
        self
    }

    /// Get device UDID
    pub fn udid(&self) -> &str {
        &self.udid
    }

    /// Get WDA URL
    pub fn wda_url(&self) -> &str {
        &self.wda_url
    }

    /// Make WDA request
    async fn wda_request(
        &self,
        method: &str,
        endpoint: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.wda_url, endpoint);

        let mut request = match method.to_uppercase().as_str() {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "DELETE" => self.client.delete(&url),
            _ => {
                return Err(AgentError::Execution(format!(
                    "Invalid HTTP method: {}",
                    method
                )))
            }
        };

        if let Some(body) = body {
            request = request.json(&body);
        }

        debug!("WDA request: {} {}", method, url);

        let response = request
            .send()
            .await
            .map_err(|e| AgentError::Execution(format!("WDA request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AgentError::Execution(format!(
                "WDA error ({}): {}",
                status, text
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError::Execution(format!("Failed to parse WDA response: {}", e)))?;

        Ok(json)
    }

    /// Safely get session ID
    async fn get_session_id(&self) -> Result<String> {
        let guard = self.session_id.lock().await;
        guard.as_ref().cloned().ok_or_else(|| {
            AgentError::Execution("No active WDA session. Call connect() first.".to_string())
        })
    }

    /// Create new session
    async fn create_session(&self) -> Result<String> {
        let body = json!({
            "capabilities": {
                "bundleId": "com.apple.mobilesafari",
                "arguments": [],
                "environment": {},
                "shouldWaitForQuiescence": false,
                "shouldUseTestManagerForVisibilityDetection": false,
                "maxTypingFrequency": 60,
                "shouldUseSingletonTestManager": true,
                "shouldTerminateApp": false
            }
        });

        let response = self.wda_request("POST", "/session", Some(body)).await?;

        let session_id = response
            .get("sessionId")
            .and_then(|s| s.as_str())
            .ok_or_else(|| AgentError::Execution("Failed to get session ID".to_string()))?;

        Ok(session_id.to_string())
    }

    /// Delete session
    async fn delete_session(&self, session_id: &str) -> Result<()> {
        let endpoint = format!("/session/{}", session_id);
        self.wda_request("DELETE", &endpoint, None).await?;
        Ok(())
    }

    /// Find element using WDA
    async fn find_element_wda(&self, locator: &ElementLocator) -> Result<WdaElement> {
        let session_id = self.get_session_id().await?;
        let (strategy, value) = self.convert_locator(locator);

        let body = json!({
            "using": strategy,
            "value": value
        });

        let endpoint = format!("/session/{}/element", session_id);
        let response = self.wda_request("POST", &endpoint, Some(body)).await?;

        let element: WdaElement =
            serde_json::from_value(response.get("value").cloned().unwrap_or_default())
                .map_err(|e| AgentError::Execution(format!("Failed to parse element: {}", e)))?;

        Ok(element)
    }

    /// Find elements using WDA
    async fn find_elements_wda(&self, locator: &ElementLocator) -> Result<Vec<WdaElement>> {
        let session_id = self.get_session_id().await?;
        let (strategy, value) = self.convert_locator(locator);

        let body = json!({
            "using": strategy,
            "value": value
        });

        let endpoint = format!("/session/{}/elements", session_id);
        let response = self.wda_request("POST", &endpoint, Some(body)).await?;

        let elements: Vec<WdaElement> =
            serde_json::from_value(response.get("value").cloned().unwrap_or_default())
                .map_err(|e| AgentError::Execution(format!("Failed to parse elements: {}", e)))?;

        Ok(elements)
    }

    /// Convert ElementLocator to WDA strategy
    fn convert_locator(&self, locator: &ElementLocator) -> (String, String) {
        match locator.locator_type {
            LocatorType::AccessibilityId => ("accessibility id".to_string(), locator.value.clone()),
            LocatorType::XPath => ("xpath".to_string(), locator.value.clone()),
            LocatorType::Id => ("id".to_string(), locator.value.clone()),
            LocatorType::ClassName => ("class name".to_string(), locator.value.clone()),
            LocatorType::Text => ("name".to_string(), locator.value.clone()),
            LocatorType::PartialText => ("partial link text".to_string(), locator.value.clone()),
            _ => ("accessibility id".to_string(), locator.value.clone()),
        }
    }

    /// Get element attribute
    async fn get_element_attribute(&self, element_id: &str, attribute: &str) -> Result<String> {
        let session_id = self.get_session_id().await?;

        let endpoint = format!(
            "/session/{}/element/{}/attribute/{}",
            session_id, element_id, attribute
        );
        let response = self.wda_request("GET", &endpoint, None).await?;

        response
            .get("value")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::Execution(format!("Attribute {} not found", attribute)))
    }

    /// Get element rect
    async fn get_element_rect(&self, element_id: &str) -> Result<ScreenBounds> {
        let session_id = self.get_session_id().await?;

        let endpoint = format!("/session/{}/element/{}/rect", session_id, element_id);
        let response = self.wda_request("GET", &endpoint, None).await?;

        let value = response
            .get("value")
            .ok_or_else(|| AgentError::Execution("No value in rect response".to_string()))?;

        let x = value.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let y = value.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let width = value.get("width").and_then(|v| v.as_i64()).unwrap_or(0) as u32;
        let height = value.get("height").and_then(|v| v.as_i64()).unwrap_or(0) as u32;

        Ok(ScreenBounds::new(x, y, width, height))
    }

    /// Get source (UI hierarchy)
    async fn get_source(&self) -> Result<String> {
        let session_id = self.get_session_id().await?;

        let endpoint = format!("/session/{}/source", session_id);
        let response = self.wda_request("GET", &endpoint, None).await?;

        response
            .get("value")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::Execution("No source in response".to_string()))
    }

    /// WDA element to UiElement
    async fn convert_to_ui_element(&self, wda_element: &WdaElement) -> Result<UiElement> {
        let id = self
            .get_element_attribute(&wda_element.element_id, "name")
            .await
            .ok();
        let text = self
            .get_element_attribute(&wda_element.element_id, "value")
            .await
            .ok();
        let description = self
            .get_element_attribute(&wda_element.element_id, "label")
            .await
            .ok();
        let class_name = self
            .get_element_attribute(&wda_element.element_id, "type")
            .await
            .ok();
        let bounds = self.get_element_rect(&wda_element.element_id).await?;

        // Get other attributes
        let enabled = self
            .get_element_attribute(&wda_element.element_id, "enabled")
            .await
            .map(|v| v == "true")
            .unwrap_or(true);

        let visible = self
            .get_element_attribute(&wda_element.element_id, "visible")
            .await
            .map(|v| v == "true")
            .unwrap_or(true);

        Ok(UiElement {
            id,
            text,
            description,
            class_name,
            bounds,
            enabled,
            visible,
            clickable: true, // iOS elements are generally clickable if enabled
            focusable: true,
        })
    }

    /// Execute command via go-ios or xcuitrunner
    async fn ios_deploy(&self, args: &[&str]) -> Result<String> {
        // Try go-ios first, then ios-deploy
        let tools = ["ios", "ios-deploy", "xcuitrunner"];

        for tool in &tools {
            let output = tokio::process::Command::new(tool)
                .args(args)
                .arg("--udid")
                .arg(&self.udid)
                .output()
                .await;

            match output {
                Ok(output) if output.status.success() => {
                    return Ok(String::from_utf8_lossy(&output.stdout).to_string());
                }
                _ => continue,
            }
        }

        Err(AgentError::Execution(
            "No iOS deployment tool found (tried: ios, ios-deploy, xcuitrunner)".to_string(),
        ))
    }
}

#[async_trait]
impl DeviceAutomation for IosDevice {
    async fn connect(&self) -> Result<()> {
        info!("Connecting to iOS device: {}", self.udid);

        // Check WDA health
        let health = self.wda_request("GET", "/health", None).await?;
        debug!("WDA health: {:?}", health);

        // Create session
        let session_id = self.create_session().await?;
        info!("Created WDA session: {}", session_id);

        // Store session
        let mut session_guard = self.session_id.lock().await;
        *session_guard = Some(session_id);
        drop(session_guard);

        // Set connected state
        let mut connected = self.connected.lock().await;
        *connected = true;
        drop(connected);

        // Cache device info
        let info = self.get_device_info().await?;
        let mut device_info = self.device_info.lock().await;
        *device_info = Some(info);

        info!("Successfully connected to iOS device: {}", self.udid);
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting from iOS device: {}", self.udid);

        // Delete session if exists
        if let Some(session_id) = self.session_id.lock().await.as_ref() {
            self.delete_session(session_id).await.ok();
        }

        let mut connected = self.connected.lock().await;
        *connected = false;

        let mut session = self.session_id.lock().await;
        *session = None;

        Ok(())
    }

    async fn is_connected(&self) -> bool {
        *self.connected.lock().await
    }

    async fn get_device_info(&self) -> Result<DeviceInfo> {
        // Check cache
        if let Some(ref info) = *self.device_info.lock().await {
            return Ok(info.clone());
        }

        // Get device info from WDA status
        let _status = self.wda_request("GET", "/status", None).await?;

        // Get screen dimensions - try primary endpoint first, fallback to secondary
        let window_size = match self
            .wda_request("GET", "/session/active/window/size", None)
            .await
        {
            Ok(resp) => Ok(resp),
            Err(_) => self.wda_request("GET", "/window/size", None).await,
        };

        let (width, height) = match window_size {
            Ok(resp) => {
                let value = resp.get("value").cloned().unwrap_or_default();
                let w = value.get("width").and_then(|v| v.as_u64()).unwrap_or(375) as u32;
                let h = value.get("height").and_then(|v| v.as_u64()).unwrap_or(812) as u32;
                (w, h)
            }
            Err(_) => (375, 812), // Default iPhone X size
        };

        let info = DeviceInfo {
            id: self.udid.clone(),
            name: "iOS Device".to_string(),
            model: "iPhone".to_string(),
            os_version: "iOS 15.0".to_string(), // Could extract from WDA status
            status: DeviceStatus::Ready,
            capabilities: vec![
                DeviceCapability::Touchscreen,
                DeviceCapability::MultiTouch,
                DeviceCapability::Screenshot,
            ],
            screen_width: width,
            screen_height: height,
            dpi: 326, // Retina display
        };

        Ok(info)
    }

    async fn get_status(&self) -> DeviceStatus {
        if !self.is_connected().await {
            return DeviceStatus::Disconnected;
        }

        // Check WDA health
        match self.wda_request("GET", "/health", None).await {
            Ok(_) => DeviceStatus::Ready,
            Err(_) => DeviceStatus::Error,
        }
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        debug!("Taking screenshot on iOS device {}", self.udid);

        let response = self.wda_request("GET", "/screenshot", None).await?;

        let base64_data = response
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::Execution("No screenshot data".to_string()))?;

        // Decode base64
        decode_base64(base64_data)
            .map_err(|e| AgentError::Execution(format!("Failed to decode screenshot: {}", e)))
    }

    async fn tap(&self, x: i32, y: i32) -> Result<()> {
        debug!("Tapping at ({}, {}) on iOS device {}", x, y, self.udid);

        let session_id = self.get_session_id().await?;

        let body = json!({
            "x": x,
            "y": y
        });

        let endpoint = format!("/session/{}/wda/tap/0", session_id);
        self.wda_request("POST", &endpoint, Some(body)).await?;

        Ok(())
    }

    async fn long_press(&self, x: i32, y: i32, duration_ms: u64) -> Result<()> {
        debug!("Long pressing at ({}, {}) for {}ms", x, y, duration_ms);

        let session_id = self.get_session_id().await?;

        let body = json!({
            "x": x,
            "y": y,
            "duration": duration_ms as f64 / 1000.0
        });

        let endpoint = format!("/session/{}/wda/touchAndHold", session_id);
        self.wda_request("POST", &endpoint, Some(body)).await?;

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

        let session_id = self.get_session_id().await?;

        let body = json!({
            "actions": [
                {
                    "action": "press",
                    "options": { "x": from_x, "y": from_y }
                },
                {
                    "action": "wait",
                    "options": { "ms": duration_ms }
                },
                {
                    "action": "moveTo",
                    "options": { "x": to_x, "y": to_y }
                },
                {
                    "action": "release"
                }
            ]
        });

        let endpoint = format!("/session/{}/wda/touch/perform", session_id);
        self.wda_request("POST", &endpoint, Some(body)).await?;

        Ok(())
    }

    async fn swipe_direction(
        &self,
        direction: SwipeDirection,
        distance: u32,
        duration_ms: u64,
    ) -> Result<()> {
        let info = self.get_device_info().await?;
        let center_x = info.screen_width as i32 / 2;
        let center_y = info.screen_height as i32 / 2;
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
        _duration_ms: u64,
    ) -> Result<()> {
        debug!(
            "Pinch gesture at ({}, {}) with scale {}",
            center_x, center_y, scale
        );

        let session_id = self.get_session_id().await?;

        let body = json!({
            "scale": scale,
            "velocity": if scale > 1.0 { 1.0 } else { -1.0 },
            "x": center_x,
            "y": center_y
        });

        let endpoint = format!("/session/{}/wda/pinch", session_id);
        self.wda_request("POST", &endpoint, Some(body)).await?;

        Ok(())
    }

    async fn find_element(&self, locator: &ElementLocator) -> Result<UiElement> {
        debug!(
            "Finding element: {:?} = {}",
            locator.locator_type, locator.value
        );

        let result = timeout(
            Duration::from_millis(locator.timeout_ms),
            self.find_element_wda(locator),
        )
        .await;

        match result {
            Ok(Ok(wda_element)) => self.convert_to_ui_element(&wda_element).await,
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AgentError::Execution(format!(
                "Timeout finding element: {:?} = {}",
                locator.locator_type, locator.value
            ))),
        }
    }

    async fn find_elements(&self, locator: &ElementLocator) -> Result<Vec<UiElement>> {
        let wda_elements = self.find_elements_wda(locator).await?;

        let mut elements = Vec::new();
        for wda_element in wda_elements {
            if let Ok(element) = self.convert_to_ui_element(&wda_element).await {
                elements.push(element);
            }
        }

        Ok(elements)
    }

    async fn tap_element(&self, locator: &ElementLocator) -> Result<()> {
        let wda_element = self.find_element_wda(locator).await?;

        let session_id = self.get_session_id().await?;

        let endpoint = format!(
            "/session/{}/element/{}/click",
            session_id, wda_element.element_id
        );
        self.wda_request("POST", &endpoint, Some(json!({}))).await?;

        Ok(())
    }

    async fn long_press_element(&self, locator: &ElementLocator, duration_ms: u64) -> Result<()> {
        let element = self.find_element(locator).await?;
        let center = element.bounds.center();
        self.long_press(center.x, center.y, duration_ms).await
    }

    async fn get_element_text(&self, locator: &ElementLocator) -> Result<String> {
        let wda_element = self.find_element_wda(locator).await?;

        let session_id = self.get_session_id().await?;

        let endpoint = format!(
            "/session/{}/element/{}/text",
            session_id, wda_element.element_id
        );
        let response = self.wda_request("GET", &endpoint, None).await?;

        response
            .get("value")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::Execution("Element has no text".to_string()))
    }

    async fn set_element_text(&self, locator: &ElementLocator, text: &str) -> Result<()> {
        // First tap to focus
        self.tap_element(locator).await?;
        sleep(Duration::from_millis(200)).await;

        // Clear existing text
        self.clear_element_text(locator).await?;

        // Type new text
        self.type_text(text).await
    }

    async fn clear_element_text(&self, locator: &ElementLocator) -> Result<()> {
        let wda_element = self.find_element_wda(locator).await?;

        let session_id = self.get_session_id().await?;

        let endpoint = format!(
            "/session/{}/element/{}/clear",
            session_id, wda_element.element_id
        );
        self.wda_request("POST", &endpoint, Some(json!({}))).await?;

        Ok(())
    }

    async fn element_exists(&self, locator: &ElementLocator) -> Result<bool> {
        match self.find_element_wda(locator).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn wait_for_element(&self, locator: &ElementLocator) -> Result<UiElement> {
        self.find_element(locator).await
    }

    async fn press_button(&self, button: HardwareButton) -> Result<()> {
        let keycode: i64 = match button {
            HardwareButton::Power => 0x400000006_i64, // iOS power button
            HardwareButton::VolumeUp => 0x400000007_i64,
            HardwareButton::VolumeDown => 0x400000008_i64,
            HardwareButton::Home => 0x400000001_i64,
            _ => {
                return Err(AgentError::Execution(format!(
                    "Button {:?} not supported on iOS",
                    button
                )))
            }
        };

        let session_id = self.get_session_id().await?;

        let body = json!({
            "key": keycode
        });

        let endpoint = format!("/session/{}/wda/pressButton", session_id);
        self.wda_request("POST", &endpoint, Some(body)).await?;

        Ok(())
    }

    async fn type_text(&self, text: &str) -> Result<()> {
        let session_id = self.get_session_id().await?;

        let body = json!({
            "value": text.chars().map(|c| c.to_string()).collect::<Vec<_>>()
        });

        let endpoint = format!("/session/{}/keys", session_id);
        self.wda_request("POST", &endpoint, Some(body)).await?;

        Ok(())
    }

    async fn get_screen_size(&self) -> Result<Size> {
        let info = self.get_device_info().await?;
        Ok(Size::new(info.screen_width, info.screen_height))
    }

    async fn go_back(&self) -> Result<()> {
        // iOS back gesture - swipe from left edge
        let size = self.get_screen_size().await?;
        self.swipe(
            10,
            size.height as i32 / 2,
            size.width as i32 / 2,
            size.height as i32 / 2,
            300,
        )
        .await
    }

    async fn go_home(&self) -> Result<()> {
        self.press_button(HardwareButton::Home).await
    }

    async fn open_recents(&self) -> Result<()> {
        // Double click home button
        let session_id = self.get_session_id().await?;

        let body = json!({
            "key": 0x400000001_i64 // Home button
        });

        // WDA doesn't have direct recents, use home button double press
        self.wda_request(
            "POST",
            &format!("/session/{}/wda/pressButton", session_id),
            Some(body.clone()),
        )
        .await?;
        sleep(Duration::from_millis(100)).await;
        self.wda_request(
            "POST",
            &format!("/session/{}/wda/pressButton", session_id),
            Some(body),
        )
        .await?;

        Ok(())
    }
}

#[async_trait]
impl AppLifecycle for IosDevice {
    async fn install_app(&self, app_path: &str) -> Result<()> {
        info!("Installing IPA: {}", app_path);
        self.ios_deploy(&["install", "--path", app_path]).await?;
        Ok(())
    }

    async fn uninstall_app(&self, bundle_id: &str) -> Result<()> {
        info!("Uninstalling app: {}", bundle_id);
        self.ios_deploy(&["uninstall", bundle_id]).await?;
        Ok(())
    }

    async fn launch_app(&self, bundle_id: &str) -> Result<()> {
        info!("Launching app: {}", bundle_id);

        // Use WDA to launch app
        let session_id = self.get_session_id().await?;

        let body = json!({
            "bundleId": bundle_id,
            "arguments": [],
            "environment": {}
        });

        let endpoint = format!("/session/{}/wda/apps/launch", session_id);
        self.wda_request("POST", &endpoint, Some(body)).await?;

        Ok(())
    }

    async fn launch_app_with_activity(&self, bundle_id: &str, _activity: &str) -> Result<()> {
        // iOS doesn't use activities like Android
        self.launch_app(bundle_id).await
    }

    async fn close_app(&self, bundle_id: &str) -> Result<()> {
        info!("Closing app: {}", bundle_id);

        let session_id = self.get_session_id().await?;

        let body = json!({
            "bundleId": bundle_id
        });

        let endpoint = format!("/session/{}/wda/apps/terminate", session_id);
        self.wda_request("POST", &endpoint, Some(body)).await?;

        Ok(())
    }

    async fn is_app_installed(&self, bundle_id: &str) -> Result<bool> {
        match self.ios_deploy(&["apps", "list"]).await {
            Ok(output) => Ok(output.contains(bundle_id)),
            Err(_) => Ok(false),
        }
    }

    async fn is_app_running(&self, bundle_id: &str) -> Result<bool> {
        info!("Checking if app is running: {}", bundle_id);

        let session_id = self.get_session_id().await?;

        let body = json!({
            "bundleId": bundle_id
        });

        let endpoint = format!("/session/{}/wda/apps/state", session_id);
        match self.wda_request("POST", &endpoint, Some(body)).await {
            Ok(response) => {
                // Parse response to determine if app is running
                if let Some(value) = response.get("value") {
                    // Check for running state (4 = running in foreground)
                    if let Some(state) = value.get("state").and_then(|s| s.as_i64()) {
                        return Ok(state == 4); // 4 = running in foreground
                    }
                }
                Ok(false)
            }
            Err(_) => {
                // If request fails, assume app is not running
                Ok(false)
            }
        }
    }

    async fn get_app_info(&self, bundle_id: &str) -> Result<AppInfo> {
        let is_installed = self.is_app_installed(bundle_id).await?;
        let is_running = self.is_app_running(bundle_id).await?;

        Ok(AppInfo {
            package_name: bundle_id.to_string(),
            app_name: bundle_id.to_string(),
            version_name: "unknown".to_string(),
            version_code: "unknown".to_string(),
            is_installed,
            is_running,
        })
    }

    async fn get_installed_apps(&self) -> Result<Vec<AppInfo>> {
        let output = self.ios_deploy(&["apps", "list"]).await?;
        let mut apps = Vec::new();

        // Parse output for bundle IDs
        for line in output.lines() {
            if line.contains("bundleId") {
                if let Some(bundle_id) = line.split('"').nth(3) {
                    if let Ok(info) = self.get_app_info(bundle_id).await {
                        apps.push(info);
                    }
                }
            }
        }

        Ok(apps)
    }

    async fn clear_app_data(&self, bundle_id: &str) -> Result<()> {
        info!("Clearing app data: {}", bundle_id);
        // iOS doesn't support clearing app data via command line
        // Would need to uninstall and reinstall
        Err(AgentError::Execution(
            "Clearing app data not supported on iOS".to_string(),
        ))
    }

    async fn grant_permission(&self, _bundle_id: &str, _permission: &str) -> Result<()> {
        info!("Granting permissions on iOS requires manual user interaction or specific APIs");
        Ok(()) // Placeholder - iOS permissions are handled differently
    }

    async fn revoke_permission(&self, _bundle_id: &str, _permission: &str) -> Result<()> {
        info!("Revoking permissions on iOS requires manual user interaction or specific APIs");
        Ok(()) // Placeholder
    }
}

/// iOS-specific controller with additional features
pub struct IosController {
    device: IosDevice,
}

impl IosController {
    /// Create new iOS controller
    pub fn new(device: IosDevice) -> Self {
        Self { device }
    }

    /// Get underlying device
    pub fn device(&self) -> &IosDevice {
        &self.device
    }

    /// Safely get session ID
    async fn get_session_id(&self) -> Result<String> {
        self.device.get_session_id().await
    }

    /// Perform shake gesture
    pub async fn shake(&self) -> Result<()> {
        let session_id = self.get_session_id().await?;

        let endpoint = format!("/session/{}/wda/shake", session_id);
        self.device
            .wda_request("POST", &endpoint, Some(json!({})))
            .await?;
        Ok(())
    }

    /// Perform two-finger tap
    pub async fn two_finger_tap(&self, x: i32, y: i32) -> Result<()> {
        let session_id = self.get_session_id().await?;

        let body = json!({
            "x": x,
            "y": y
        });

        let endpoint = format!("/session/{}/wda/touchAndHold", session_id);
        self.device
            .wda_request("POST", &endpoint, Some(body))
            .await?;
        Ok(())
    }

    /// Get battery level
    pub async fn get_battery_level(&self) -> Result<f64> {
        let response = self
            .device
            .wda_request("GET", "/wda/batteryInfo", None)
            .await?;

        response
            .get("value")
            .and_then(|v| v.get("level"))
            .and_then(|l| l.as_f64())
            .ok_or_else(|| AgentError::Execution("Failed to get battery level".to_string()))
    }

    /// Get device orientation
    pub async fn get_orientation(&self) -> Result<String> {
        let response = self.device.wda_request("GET", "/orientation", None).await?;

        response
            .get("value")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::Execution("Failed to get orientation".to_string()))
    }

    /// Set device orientation
    pub async fn set_orientation(&self, orientation: &str) -> Result<()> {
        let body = json!({
            "orientation": orientation
        });

        self.device
            .wda_request("POST", "/orientation", Some(body))
            .await?;
        Ok(())
    }

    /// Lock device
    pub async fn lock(&self) -> Result<()> {
        let session_id = self.get_session_id().await?;

        let endpoint = format!("/session/{}/wda/lock", session_id);
        self.device
            .wda_request("POST", &endpoint, Some(json!({})))
            .await?;
        Ok(())
    }

    /// Unlock device
    pub async fn unlock(&self) -> Result<()> {
        let session_id = self.get_session_id().await?;

        let endpoint = format!("/session/{}/wda/unlock", session_id);
        self.device
            .wda_request("POST", &endpoint, Some(json!({})))
            .await?;
        Ok(())
    }

    /// Check if device is locked
    pub async fn is_locked(&self) -> Result<bool> {
        let response = self.device.wda_request("GET", "/wda/locked", None).await?;

        response
            .get("value")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| AgentError::Execution("Failed to get lock state".to_string()))
    }

    /// Get alert text
    pub async fn get_alert_text(&self) -> Result<String> {
        let session_id = self.get_session_id().await?;

        let endpoint = format!("/session/{}/alert/text", session_id);
        let response = self.device.wda_request("GET", &endpoint, None).await?;

        response
            .get("value")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::Execution("No alert text".to_string()))
    }

    /// Accept alert
    pub async fn accept_alert(&self) -> Result<()> {
        let session_id = self.get_session_id().await?;

        let endpoint = format!("/session/{}/alert/accept", session_id);
        self.device
            .wda_request("POST", &endpoint, Some(json!({})))
            .await?;
        Ok(())
    }

    /// Dismiss alert
    pub async fn dismiss_alert(&self) -> Result<()> {
        let session_id = self.get_session_id().await?;

        let endpoint = format!("/session/{}/alert/dismiss", session_id);
        self.device
            .wda_request("POST", &endpoint, Some(json!({})))
            .await?;
        Ok(())
    }

    /// Get device logs
    pub async fn get_logs(&self) -> Result<Vec<String>> {
        let response = self
            .device
            .wda_request("POST", "/wda/getLogs", Some(json!({})))
            .await?;

        let logs = response
            .get("value")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(logs)
    }

    /// Clear device logs
    pub async fn clear_logs(&self) -> Result<()> {
        self.device
            .wda_request("POST", "/wda/clearLogs", Some(json!({})))
            .await?;
        Ok(())
    }

    /// Dump UI hierarchy
    pub async fn dump_ui_hierarchy(&self) -> Result<String> {
        self.device.get_source().await
    }
}

// Base64 decoding helper
fn decode_base64(input: &str) -> std::result::Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ios_device_creation() {
        let device = IosDevice::new("00008030-001234567890ABCD");
        assert_eq!(device.udid(), "00008030-001234567890ABCD");
        assert_eq!(device.wda_url(), "http://localhost:8100");
    }

    #[test]
    fn test_ios_device_with_custom_url() {
        let device = IosDevice::new("test-udid").with_wda_url("http://192.168.1.100:8100");
        assert_eq!(device.wda_url(), "http://192.168.1.100:8100");
    }

    #[test]
    fn test_convert_locator() {
        let device = IosDevice::new("test");

        let locator = ElementLocator::new(LocatorType::AccessibilityId, "button1");
        let (strategy, value) = device.convert_locator(&locator);
        assert_eq!(strategy, "accessibility id");
        assert_eq!(value, "button1");

        let locator = ElementLocator::new(LocatorType::XPath, "//Button[1]");
        let (strategy, value) = device.convert_locator(&locator);
        assert_eq!(strategy, "xpath");
        assert_eq!(value, "//Button[1]");
    }
}
