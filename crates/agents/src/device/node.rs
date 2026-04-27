//! Device Node
//!
//! Generic device abstraction that can wrap any device type.
//! Provides a unified interface for device management and automation.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use super::{
    AppInfo, AppLifecycle, DeviceAutomation, DeviceCapability, DeviceInfo, DeviceStatus,
    ElementLocator, HardwareButton, Size, SwipeDirection, UiElement,
};
use crate::error::{AgentError, Result};

/// Generic device node that can wrap any device
///
/// This provides a unified interface for device management,
/// similar to a node in a device mesh.
#[derive(Clone)]
pub struct DeviceNode {
    /// Node ID
    id: String,
    /// Node name
    name: String,
    /// Device capabilities
    capabilities: Vec<DeviceCapability>,
    /// Device properties/metadata
    properties: Arc<Mutex<HashMap<String, String>>>,
    /// Connection state
    connected: Arc<Mutex<bool>>,
    /// Device status
    status: Arc<Mutex<DeviceStatus>>,
    /// Command handler (can be customized)
    command_handler: Option<Arc<dyn Fn(&str) -> Result<String> + Send + Sync>>,
}

impl std::fmt::Debug for DeviceNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceNode")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("capabilities", &self.capabilities)
            .field("command_handler", &self.command_handler.is_some())
            .finish()
    }
}

impl DeviceNode {
    /// Create new device node
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: "Generic Device".to_string(),
            capabilities: Vec::new(),
            properties: Arc::new(Mutex::new(HashMap::new())),
            connected: Arc::new(Mutex::new(false)),
            status: Arc::new(Mutex::new(DeviceStatus::Disconnected)),
            command_handler: None,
        }
    }

    /// Set device name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Add capability
    pub fn add_capability(&mut self, cap: DeviceCapability) {
        self.capabilities.push(cap);
    }

    /// Add multiple capabilities
    pub fn with_capabilities(mut self, caps: Vec<DeviceCapability>) -> Self {
        self.capabilities = caps;
        self
    }

    /// Set command handler
    pub fn with_command_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(&str) -> Result<String> + Send + Sync + 'static,
    {
        self.command_handler = Some(Arc::new(handler));
        self
    }

    /// Get device ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get device name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get capabilities
    pub fn capabilities(&self) -> &[DeviceCapability] {
        &self.capabilities
    }

    /// Check if device has capability
    pub fn has_capability(&self, cap: &DeviceCapability) -> bool {
        self.capabilities.contains(cap)
    }

    /// Set property
    pub async fn set_property(&self, key: impl Into<String>, value: impl Into<String>) {
        let mut props = self.properties.lock().await;
        props.insert(key.into(), value.into());
    }

    /// Get property
    pub async fn get_property(&self, key: &str) -> Option<String> {
        let props = self.properties.lock().await;
        props.get(key).cloned()
    }

    /// Get all properties
    pub async fn get_properties(&self) -> HashMap<String, String> {
        self.properties.lock().await.clone()
    }

    /// Execute command on device
    pub async fn execute(&self, command: &str) -> Result<String> {
        debug!("Executing command '{}' on device {}", command, self.id);

        if let Some(ref handler) = self.command_handler {
            handler(command)
        } else {
            // Default behavior: just log and echo
            info!("Device {} executed: {}", self.id, command);
            Ok(format!("Executed: {}", command))
        }
    }

    /// Execute multiple commands
    pub async fn execute_batch(&self, commands: &[&str]) -> Result<Vec<String>> {
        let mut results = Vec::new();
        for cmd in commands {
            match self.execute(cmd).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    error!("Command '{}' failed: {}", cmd, e);
                    results.push(format!("Error: {}", e));
                }
            }
        }
        Ok(results)
    }

    /// Update status
    async fn update_status(&self, status: DeviceStatus) {
        let mut s = self.status.lock().await;
        *s = status;
    }

    /// Get current status
    pub async fn get_current_status(&self) -> DeviceStatus {
        *self.status.lock().await
    }
}

#[async_trait]
impl DeviceAutomation for DeviceNode {
    async fn connect(&self) -> Result<()> {
        info!("Connecting to device node: {}", self.id);

        let mut connected = self.connected.lock().await;
        *connected = true;
        drop(connected);

        self.update_status(DeviceStatus::Ready).await;

        info!("Device node {} connected", self.id);
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting from device node: {}", self.id);

        let mut connected = self.connected.lock().await;
        *connected = false;
        drop(connected);

        self.update_status(DeviceStatus::Disconnected).await;

        Ok(())
    }

    async fn is_connected(&self) -> bool {
        *self.connected.lock().await
    }

    async fn get_device_info(&self) -> Result<DeviceInfo> {
        let props = self.properties.lock().await;

        let width = props
            .get("screen_width")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1080);

        let height = props
            .get("screen_height")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1920);

        Ok(DeviceInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            model: props.get("model").cloned().unwrap_or_default(),
            os_version: props.get("os_version").cloned().unwrap_or_default(),
            status: self.get_current_status().await,
            capabilities: self.capabilities.clone(),
            screen_width: width,
            screen_height: height,
            dpi: props.get("dpi").and_then(|s| s.parse().ok()).unwrap_or(320),
        })
    }

    async fn get_status(&self) -> DeviceStatus {
        self.get_current_status().await
    }

    async fn take_screenshot(&self) -> Result<Vec<u8>> {
        if !self.has_capability(&DeviceCapability::Screenshot) {
            return Err(AgentError::Execution(
                "Device does not support screenshots".to_string(),
            ));
        }

        // This is a placeholder - actual implementation would capture screen
        debug!("Taking screenshot on device node {}", self.id);

        // Return empty PNG (1x1 transparent pixel in base64)
        let empty_png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")
            .map_err(|e| AgentError::Execution(format!("Failed to decode placeholder: {}", e)))?;

        Ok(empty_png)
    }

    async fn tap(&self, x: i32, y: i32) -> Result<()> {
        if !self.has_capability(&DeviceCapability::Touchscreen) {
            return Err(AgentError::Execution(
                "Device does not support touch".to_string(),
            ));
        }

        self.execute(&format!("tap {} {}", x, y)).await?;
        Ok(())
    }

    async fn long_press(&self, x: i32, y: i32, duration_ms: u64) -> Result<()> {
        if !self.has_capability(&DeviceCapability::Touchscreen) {
            return Err(AgentError::Execution(
                "Device does not support touch".to_string(),
            ));
        }

        self.execute(&format!("long_press {} {} {}", x, y, duration_ms))
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
        if !self.has_capability(&DeviceCapability::Touchscreen) {
            return Err(AgentError::Execution(
                "Device does not support touch".to_string(),
            ));
        }

        self.execute(&format!(
            "swipe {} {} {} {} {}",
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
        self.execute(&format!(
            "swipe_direction {:?} {} {}",
            direction, distance, duration_ms
        ))
        .await?;
        Ok(())
    }

    async fn pinch(
        &self,
        center_x: i32,
        center_y: i32,
        scale: f64,
        duration_ms: u64,
    ) -> Result<()> {
        if !self.has_capability(&DeviceCapability::MultiTouch) {
            return Err(AgentError::Execution(
                "Device does not support multi-touch".to_string(),
            ));
        }

        self.execute(&format!(
            "pinch {} {} {} {}",
            center_x, center_y, scale, duration_ms
        ))
        .await?;
        Ok(())
    }

    async fn find_element(&self, _locator: &ElementLocator) -> Result<UiElement> {
        // This is a generic implementation
        // Specific implementations would override this
        Err(AgentError::Execution(
            "Element finding not implemented for generic device node".to_string(),
        ))
    }

    async fn find_elements(&self, _locator: &ElementLocator) -> Result<Vec<UiElement>> {
        // Generic implementation returns empty list
        Ok(Vec::new())
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

    async fn get_element_text(&self, _locator: &ElementLocator) -> Result<String> {
        Err(AgentError::Execution(
            "Element text retrieval not implemented".to_string(),
        ))
    }

    async fn set_element_text(&self, _locator: &ElementLocator, text: &str) -> Result<()> {
        self.execute(&format!("set_text '{}'", text))
            .await
            .map(|_| ())
    }

    async fn clear_element_text(&self, _locator: &ElementLocator) -> Result<()> {
        self.execute("clear_text").await.map(|_| ())
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
        self.execute(&format!("press_button {:?}", button))
            .await
            .map(|_| ())
    }

    async fn type_text(&self, text: &str) -> Result<()> {
        self.execute(&format!("type '{}'", text)).await.map(|_| ())
    }

    async fn get_screen_size(&self) -> Result<Size> {
        let props = self.properties.lock().await;
        let width = props
            .get("screen_width")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1080);
        let height = props
            .get("screen_height")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1920);
        Ok(Size::new(width, height))
    }

    async fn go_back(&self) -> Result<()> {
        self.execute("go_back").await.map(|_| ())
    }

    async fn go_home(&self) -> Result<()> {
        self.execute("go_home").await.map(|_| ())
    }

    async fn open_recents(&self) -> Result<()> {
        self.execute("open_recents").await.map(|_| ())
    }
}

#[async_trait]
impl AppLifecycle for DeviceNode {
    async fn install_app(&self, app_path: &str) -> Result<()> {
        self.execute(&format!("install_app {}", app_path))
            .await
            .map(|_| ())
    }

    async fn uninstall_app(&self, package_name: &str) -> Result<()> {
        self.execute(&format!("uninstall_app {}", package_name))
            .await
            .map(|_| ())
    }

    async fn launch_app(&self, package_name: &str) -> Result<()> {
        self.execute(&format!("launch_app {}", package_name))
            .await
            .map(|_| ())
    }

    async fn launch_app_with_activity(&self, package_name: &str, activity: &str) -> Result<()> {
        self.execute(&format!("launch_app {} {}", package_name, activity))
            .await
            .map(|_| ())
    }

    async fn close_app(&self, package_name: &str) -> Result<()> {
        self.execute(&format!("close_app {}", package_name))
            .await
            .map(|_| ())
    }

    async fn is_app_installed(&self, package_name: &str) -> Result<bool> {
        let result = self
            .execute(&format!("is_app_installed {}", package_name))
            .await?;
        Ok(result.contains("true"))
    }

    async fn is_app_running(&self, package_name: &str) -> Result<bool> {
        let result = self
            .execute(&format!("is_app_running {}", package_name))
            .await?;
        Ok(result.contains("true"))
    }

    async fn get_app_info(&self, package_name: &str) -> Result<AppInfo> {
        let installed = self.is_app_installed(package_name).await?;
        let running = self.is_app_running(package_name).await?;

        Ok(AppInfo {
            package_name: package_name.to_string(),
            app_name: package_name.to_string(),
            version_name: "unknown".to_string(),
            version_code: "unknown".to_string(),
            is_installed: installed,
            is_running: running,
        })
    }

    async fn get_installed_apps(&self) -> Result<Vec<AppInfo>> {
        // Generic device node doesn't support listing apps
        Ok(vec![])
    }

    async fn clear_app_data(&self, package_name: &str) -> Result<()> {
        self.execute(&format!("clear_app_data {}", package_name))
            .await
            .map(|_| ())
    }

    async fn grant_permission(&self, package_name: &str, permission: &str) -> Result<()> {
        self.execute(&format!("grant_permission {} {}", package_name, permission))
            .await
            .map(|_| ())
    }

    async fn revoke_permission(&self, package_name: &str, permission: &str) -> Result<()> {
        self.execute(&format!(
            "revoke_permission {} {}",
            package_name, permission
        ))
        .await
        .map(|_| ())
    }
}

/// Device node builder for easy construction
pub struct DeviceNodeBuilder {
    node: DeviceNode,
}

impl DeviceNodeBuilder {
    /// Create new builder
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            node: DeviceNode::new(id),
        }
    }

    /// Set name
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.node.name = name.into();
        self
    }

    /// Add capability
    pub fn capability(mut self, cap: DeviceCapability) -> Self {
        self.node.capabilities.push(cap);
        self
    }

    /// Set screen size
    pub fn screen_size(self, width: u32, height: u32) -> Self {
        let node = self.node.clone();
        tokio::spawn(async move {
            node.set_property("screen_width", width.to_string()).await;
            node.set_property("screen_height", height.to_string()).await;
        });
        self
    }

    /// Set DPI
    pub fn dpi(self, dpi: u32) -> Self {
        let node = self.node.clone();
        tokio::spawn(async move {
            node.set_property("dpi", dpi.to_string()).await;
        });
        self
    }

    /// Set model
    pub fn model(self, model: impl Into<String>) -> Self {
        let node = self.node.clone();
        let model = model.into();
        tokio::spawn(async move {
            node.set_property("model", model).await;
        });
        self
    }

    /// Build the device node
    pub fn build(self) -> DeviceNode {
        self.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_device_node_creation() {
        let node = DeviceNode::new("test-device")
            .with_name("Test Device")
            .with_capabilities(vec![
                DeviceCapability::Touchscreen,
                DeviceCapability::Screenshot,
            ]);

        assert_eq!(node.id(), "test-device");
        assert_eq!(node.name(), "Test Device");
        assert!(node.has_capability(&DeviceCapability::Touchscreen));
        assert!(node.has_capability(&DeviceCapability::Screenshot));
        assert!(!node.has_capability(&DeviceCapability::Camera));
    }

    #[tokio::test]
    async fn test_device_node_properties() {
        let node = DeviceNode::new("test");

        node.set_property("version", "1.0.0").await;
        node.set_property("platform", "test-platform").await;

        assert_eq!(
            node.get_property("version").await,
            Some("1.0.0".to_string())
        );
        assert_eq!(
            node.get_property("platform").await,
            Some("test-platform".to_string())
        );
        assert_eq!(node.get_property("missing").await, None);

        let props = node.get_properties().await;
        assert_eq!(props.len(), 2);
    }

    #[tokio::test]
    async fn test_device_node_execute() {
        let node = DeviceNode::new("test");

        let result = node.execute("test_command").await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Executed"));
    }

    #[tokio::test]
    async fn test_device_node_connect() {
        let node = DeviceNode::new("test");

        assert!(!node.is_connected().await);

        node.connect().await.unwrap();
        assert!(node.is_connected().await);
        assert_eq!(node.get_status().await, DeviceStatus::Ready);

        node.disconnect().await.unwrap();
        assert!(!node.is_connected().await);
    }

    #[test]
    fn test_device_node_builder() {
        let node = DeviceNodeBuilder::new("builder-test")
            .name("Built Device")
            .capability(DeviceCapability::Touchscreen)
            .build();

        assert_eq!(node.id(), "builder-test");
        assert_eq!(node.name(), "Built Device");
        assert!(node.has_capability(&DeviceCapability::Touchscreen));
    }
}
