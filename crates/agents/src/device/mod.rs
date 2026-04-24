//! Device Module
//!
//! Device control and automation for Android and iOS devices.
//! Provides a unified interface for device automation similar to Appium/MAUI.
//!
//! # Features
//! - Android device control via ADB
//! - iOS device control via WebDriverAgent/XCUITest
//! - Screen gestures (tap, swipe, long press)
//! - Element location and interaction
//! - Screenshot capture
//! - App installation and lifecycle management
//!
//! # Example
//! ```rust,no_run
//! use beebotos_agents::device::{AndroidDevice, DeviceAutomation, ElementLocator, LocatorType};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let device = AndroidDevice::new("emulator-5554");
//! device.connect().await?;
//!
//! // Tap on element
//! let locator = ElementLocator::new(LocatorType::Id, "com.example.app:id/button");
//! device.tap_element(&locator).await?;
//!
//! // Swipe gesture
//! device.swipe(300, 800, 300, 200, 500).await?;
//! # Ok(())
//! # }
//! ```

use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{AgentError, Result};

pub mod android;
pub mod ios;
pub mod node;

pub use android::{AndroidController, AndroidDevice};
pub use ios::{IosController, IosDevice};
pub use node::DeviceNode;

/// Device error types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceError {
    /// Device not found or not connected
    DeviceNotFound(String),
    /// ADB command failed
    AdbError(String),
    /// iOS specific error
    IosError(String),
    /// Element not found
    ElementNotFound(String),
    /// Operation timeout
    Timeout(String),
    /// Invalid operation
    InvalidOperation(String),
    /// Connection error
    ConnectionError(String),
    /// Screenshot error
    ScreenshotError(String),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::DeviceNotFound(msg) => write!(f, "Device not found: {}", msg),
            DeviceError::AdbError(msg) => write!(f, "ADB error: {}", msg),
            DeviceError::IosError(msg) => write!(f, "iOS error: {}", msg),
            DeviceError::ElementNotFound(msg) => write!(f, "Element not found: {}", msg),
            DeviceError::Timeout(msg) => write!(f, "Operation timeout: {}", msg),
            DeviceError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
            DeviceError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            DeviceError::ScreenshotError(msg) => write!(f, "Screenshot error: {}", msg),
        }
    }
}

impl std::error::Error for DeviceError {}

impl From<DeviceError> for AgentError {
    fn from(e: DeviceError) -> Self {
        AgentError::Execution(e.to_string())
    }
}

/// Device capability
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceCapability {
    /// Touch screen support
    Touchscreen,
    /// Multi-touch support
    MultiTouch,
    /// Screenshot capability
    Screenshot,
    /// GPS location
    Gps,
    /// Camera
    Camera,
    /// Biometric authentication
    Biometric,
    /// NFC
    Nfc,
    /// Bluetooth
    Bluetooth,
    /// WiFi
    Wifi,
    /// Mobile data
    MobileData,
    /// Custom capability
    Custom(String),
}

/// Device status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum DeviceStatus {
    /// Device is disconnected
    Disconnected,
    /// Device is connecting
    Connecting,
    /// Device is ready
    Ready,
    /// Device is busy
    Busy,
    /// Device error
    Error,
}

/// Device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device ID (serial for Android, UDID for iOS)
    pub id: String,
    /// Device name
    pub name: String,
    /// Device model
    pub model: String,
    /// OS version
    pub os_version: String,
    /// Device status
    pub status: DeviceStatus,
    /// Available capabilities
    pub capabilities: Vec<DeviceCapability>,
    /// Screen width
    pub screen_width: u32,
    /// Screen height
    pub screen_height: u32,
    /// DPI
    pub dpi: u32,
}

/// Point on screen
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Size of screen or element
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// Screen bounds
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ScreenBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenBounds {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center(&self) -> Point {
        Point::new(
            self.x + self.width as i32 / 2,
            self.y + self.height as i32 / 2,
        )
    }
}

/// Swipe direction
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Gesture action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GestureAction {
    /// Tap at coordinates
    Tap { x: i32, y: i32 },
    /// Long press at coordinates
    LongPress { x: i32, y: i32, duration_ms: u64 },
    /// Swipe from one point to another
    Swipe {
        from: Point,
        to: Point,
        duration_ms: u64,
    },
    /// Multi-touch gesture
    MultiTouch { points: Vec<Point> },
    /// Pinch gesture
    Pinch {
        center: Point,
        scale: f64,
        duration_ms: u64,
    },
}

/// Locator type for finding elements
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LocatorType {
    /// XPath locator
    XPath,
    /// Accessibility ID
    AccessibilityId,
    /// Resource ID (Android)
    Id,
    /// Class name
    ClassName,
    /// Text content
    Text,
    /// Partial text match
    PartialText,
    /// CSS selector (for hybrid apps)
    CssSelector,
    /// UI Automation locator (iOS)
    UiAutomation,
}

/// Element locator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementLocator {
    /// Locator type
    pub locator_type: LocatorType,
    /// Locator value
    pub value: String,
    /// Optional index if multiple elements match
    pub index: Option<usize>,
    /// Wait timeout in milliseconds
    pub timeout_ms: u64,
}

impl ElementLocator {
    /// Create new element locator
    pub fn new(locator_type: LocatorType, value: impl Into<String>) -> Self {
        Self {
            locator_type,
            value: value.into(),
            index: None,
            timeout_ms: 10000, // Default 10 seconds
        }
    }

    /// Set index for multiple matching elements
    pub fn with_index(mut self, index: usize) -> Self {
        self.index = Some(index);
        self
    }

    /// Set wait timeout
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}

/// UI Element information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElement {
    /// Element ID
    pub id: Option<String>,
    /// Element text
    pub text: Option<String>,
    /// Element description/content description
    pub description: Option<String>,
    /// Element class/type
    pub class_name: Option<String>,
    /// Element bounds
    pub bounds: ScreenBounds,
    /// Is element enabled
    pub enabled: bool,
    /// Is element visible
    pub visible: bool,
    /// Is element clickable
    pub clickable: bool,
    /// Is element focusable
    pub focusable: bool,
}

/// Device abstraction enum
#[derive(Debug)]
pub enum Device {
    Node(node::DeviceNode),
    Ios(ios::IosDevice),
    Android(android::AndroidDevice),
}

/// Core device automation trait
///
/// This trait defines the common interface for device automation,
/// similar to Appium's WebDriver interface.
#[async_trait]
pub trait DeviceAutomation: Send + Sync {
    /// Connect to device
    async fn connect(&self) -> Result<()>;

    /// Disconnect from device
    async fn disconnect(&self) -> Result<()>;

    /// Check if device is connected
    async fn is_connected(&self) -> bool;

    /// Get device information
    async fn get_device_info(&self) -> Result<DeviceInfo>;

    /// Get device status
    async fn get_status(&self) -> DeviceStatus;

    /// Take screenshot
    async fn take_screenshot(&self) -> Result<Vec<u8>>;

    /// Tap at coordinates
    async fn tap(&self, x: i32, y: i32) -> Result<()>;

    /// Long press at coordinates
    async fn long_press(&self, x: i32, y: i32, duration_ms: u64) -> Result<()>;

    /// Swipe from one point to another
    async fn swipe(
        &self,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        duration_ms: u64,
    ) -> Result<()>;

    /// Swipe in direction
    async fn swipe_direction(
        &self,
        direction: SwipeDirection,
        distance: u32,
        duration_ms: u64,
    ) -> Result<()>;

    /// Pinch gesture
    async fn pinch(&self, center_x: i32, center_y: i32, scale: f64, duration_ms: u64)
        -> Result<()>;

    /// Find element by locator
    async fn find_element(&self, locator: &ElementLocator) -> Result<UiElement>;

    /// Find multiple elements
    async fn find_elements(&self, locator: &ElementLocator) -> Result<Vec<UiElement>>;

    /// Tap on element
    async fn tap_element(&self, locator: &ElementLocator) -> Result<()>;

    /// Long press on element
    async fn long_press_element(&self, locator: &ElementLocator, duration_ms: u64) -> Result<()>;

    /// Get element text
    async fn get_element_text(&self, locator: &ElementLocator) -> Result<String>;

    /// Set element text (input field)
    async fn set_element_text(&self, locator: &ElementLocator, text: &str) -> Result<()>;

    /// Clear element text
    async fn clear_element_text(&self, locator: &ElementLocator) -> Result<()>;

    /// Check if element exists
    async fn element_exists(&self, locator: &ElementLocator) -> Result<bool>;

    /// Wait for element to appear
    async fn wait_for_element(&self, locator: &ElementLocator) -> Result<UiElement>;

    /// Press hardware button
    async fn press_button(&self, button: HardwareButton) -> Result<()>;

    /// Type text (system-wide)
    async fn type_text(&self, text: &str) -> Result<()>;

    /// Get screen size
    async fn get_screen_size(&self) -> Result<Size>;

    /// Go back
    async fn go_back(&self) -> Result<()>;

    /// Go home
    async fn go_home(&self) -> Result<()>;

    /// Open app switcher/recents
    async fn open_recents(&self) -> Result<()>;
}

/// Hardware buttons
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum HardwareButton {
    Power,
    VolumeUp,
    VolumeDown,
    Home,
    Back,
    Menu,
    Search,
    Camera,
}

/// App information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    /// Package name (Android) or Bundle ID (iOS)
    pub package_name: String,
    /// App name
    pub app_name: String,
    /// Version name
    pub version_name: String,
    /// Version code
    pub version_code: String,
    /// Is app installed
    pub is_installed: bool,
    /// Is app running
    pub is_running: bool,
}

/// App lifecycle trait
#[async_trait]
pub trait AppLifecycle: Send + Sync {
    /// Install app
    async fn install_app(&self, app_path: &str) -> Result<()>;

    /// Uninstall app
    async fn uninstall_app(&self, package_name: &str) -> Result<()>;

    /// Launch app
    async fn launch_app(&self, package_name: &str) -> Result<()>;

    /// Launch app with activity (Android)
    async fn launch_app_with_activity(&self, package_name: &str, activity: &str) -> Result<()>;

    /// Close/terminate app
    async fn close_app(&self, package_name: &str) -> Result<()>;

    /// Check if app is installed
    async fn is_app_installed(&self, package_name: &str) -> Result<bool>;

    /// Check if app is running
    async fn is_app_running(&self, package_name: &str) -> Result<bool>;

    /// Get app info
    async fn get_app_info(&self, package_name: &str) -> Result<AppInfo>;

    /// Get list of installed apps
    async fn get_installed_apps(&self) -> Result<Vec<AppInfo>>;

    /// Clear app data
    async fn clear_app_data(&self, package_name: &str) -> Result<()>;

    /// Grant permission
    async fn grant_permission(&self, package_name: &str, permission: &str) -> Result<()>;

    /// Revoke permission
    async fn revoke_permission(&self, package_name: &str, permission: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_creation() {
        let point = Point::new(100, 200);
        assert_eq!(point.x, 100);
        assert_eq!(point.y, 200);
    }

    #[test]
    fn test_screen_bounds() {
        let bounds = ScreenBounds::new(0, 0, 1080, 1920);
        let center = bounds.center();
        assert_eq!(center.x, 540);
        assert_eq!(center.y, 960);
    }

    #[test]
    fn test_element_locator() {
        let locator = ElementLocator::new(LocatorType::Id, "button1")
            .with_index(0)
            .with_timeout(5000);

        assert_eq!(locator.locator_type, LocatorType::Id);
        assert_eq!(locator.value, "button1");
        assert_eq!(locator.index, Some(0));
        assert_eq!(locator.timeout_ms, 5000);
    }
}
