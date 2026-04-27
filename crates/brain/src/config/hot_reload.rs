//! Configuration Hot Reload
//!
//! Supports runtime configuration updates with change detection.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use super::{BrainConfig, ConfigBuilder, ConfigError};

/// Hot-reloadable configuration manager
#[derive(Debug)]
pub struct HotReloadConfig {
    current: Arc<RwLock<BrainConfig>>,
    source_path: Option<PathBuf>,
    last_modified: Option<SystemTime>,
    check_interval: Duration,
    last_check: SystemTime,
}

impl HotReloadConfig {
    /// Create new hot-reload manager with initial config
    pub fn new(config: BrainConfig) -> Self {
        Self {
            current: Arc::new(RwLock::new(config)),
            source_path: None,
            last_modified: None,
            check_interval: Duration::from_secs(5),
            last_check: SystemTime::now(),
        }
    }

    /// Create from file with hot-reload support
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref().to_path_buf();
        let config = ConfigBuilder::from_toml_file(&path)?.build()?;

        let metadata = std::fs::metadata(&path)?;
        let modified = metadata.modified().ok();

        Ok(Self {
            current: Arc::new(RwLock::new(config)),
            source_path: Some(path),
            last_modified: modified,
            check_interval: Duration::from_secs(5),
            last_check: SystemTime::now(),
        })
    }

    /// Get current configuration (read lock)
    pub fn get(&self) -> Result<BrainConfig, ConfigError> {
        self.current
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| ConfigError::Io("Lock poisoned".to_string()))
    }

    /// Get shared reference to current config
    pub fn get_arc(&self) -> Arc<RwLock<BrainConfig>> {
        Arc::clone(&self.current)
    }

    /// Check if configuration file has changed
    pub fn has_changed(&self) -> bool {
        let Some(ref path) = self.source_path else {
            return false;
        };

        let Ok(metadata) = std::fs::metadata(path) else {
            return false;
        };

        let Ok(modified) = metadata.modified() else {
            return false;
        };

        match self.last_modified {
            Some(last) => modified > last,
            None => true,
        }
    }

    /// Check if it's time to check for changes
    pub fn should_check(&self) -> bool {
        self.last_check.elapsed().unwrap_or(Duration::MAX) >= self.check_interval
    }

    /// Reload configuration from file
    pub fn reload(&mut self) -> Result<bool, ConfigError> {
        if !self.should_check() {
            return Ok(false);
        }

        self.last_check = SystemTime::now();

        if !self.has_changed() {
            return Ok(false);
        }

        let Some(ref path) = self.source_path else {
            return Ok(false);
        };

        // Load new config
        let new_config = ConfigBuilder::from_toml_file(path)?.build()?;

        // Update stored config
        {
            let mut guard = self
                .current
                .write()
                .map_err(|_| ConfigError::Io("Lock poisoned".to_string()))?;
            *guard = new_config;
        }

        // Update metadata
        if let Ok(metadata) = std::fs::metadata(path) {
            self.last_modified = metadata.modified().ok();
        }

        Ok(true)
    }

    /// Set check interval
    pub fn set_check_interval(&mut self, interval: Duration) {
        self.check_interval = interval;
    }

    /// Force reload regardless of change detection
    pub fn force_reload(&mut self) -> Result<(), ConfigError> {
        let Some(ref path) = self.source_path else {
            return Err(ConfigError::NotFound(
                "No source file configured".to_string(),
            ));
        };

        let new_config = ConfigBuilder::from_toml_file(path)?.build()?;

        {
            let mut guard = self
                .current
                .write()
                .map_err(|_| ConfigError::Io("Lock poisoned".to_string()))?;
            *guard = new_config;
        }

        if let Ok(metadata) = std::fs::metadata(path) {
            self.last_modified = metadata.modified().ok();
        }

        Ok(())
    }

    /// Update configuration programmatically
    pub fn update<F>(&self, updater: F) -> Result<(), ConfigError>
    where
        F: FnOnce(&mut BrainConfig),
    {
        let mut guard = self
            .current
            .write()
            .map_err(|_| ConfigError::Io("Lock poisoned".to_string()))?;
        updater(&mut guard);
        Ok(())
    }
}

/// Configuration change callback
pub type ConfigChangeCallback = Box<dyn Fn(&BrainConfig) + Send + Sync>;

/// Auto-reloading configuration with callbacks
pub struct AutoReloadConfig {
    inner: HotReloadConfig,
    callbacks: Vec<ConfigChangeCallback>,
}

impl std::fmt::Debug for AutoReloadConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutoReloadConfig")
            .field("inner", &self.inner)
            .field("callbacks", &self.callbacks.len())
            .finish()
    }
}

impl AutoReloadConfig {
    pub fn new(config: BrainConfig) -> Self {
        Self {
            inner: HotReloadConfig::new(config),
            callbacks: Vec::new(),
        }
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        Ok(Self {
            inner: HotReloadConfig::from_file(path)?,
            callbacks: Vec::new(),
        })
    }

    /// Register callback for configuration changes
    pub fn on_change<F>(&mut self, callback: F)
    where
        F: Fn(&BrainConfig) + Send + Sync + 'static,
    {
        self.callbacks.push(Box::new(callback));
    }

    /// Check and reload if changed, triggering callbacks
    pub fn check_and_reload(&mut self) -> Result<bool, ConfigError> {
        let changed = self.inner.reload()?;

        if changed {
            let config = self.inner.get()?;
            for callback in &self.callbacks {
                callback(&config);
            }
        }

        Ok(changed)
    }

    /// Get current configuration
    pub fn get(&self) -> Result<BrainConfig, ConfigError> {
        self.inner.get()
    }
}

/// Background configuration watcher
#[allow(dead_code)]
pub struct ConfigWatcher {
    config: Arc<RwLock<BrainConfig>>,
    path: PathBuf,
    running: std::sync::atomic::AtomicBool,
}

impl ConfigWatcher {
    /// Start watching configuration file in background
    pub fn start<P: AsRef<Path>>(
        path: P,
        interval: Duration,
    ) -> Result<Arc<RwLock<BrainConfig>>, ConfigError> {
        let path = path.as_ref().to_path_buf();
        let initial_config = ConfigBuilder::from_toml_file(&path)?.build()?;

        let config = Arc::new(RwLock::new(initial_config));
        let config_clone = Arc::clone(&config);
        let path_clone = path.clone();

        std::thread::spawn(move || {
            let mut last_modified = std::fs::metadata(&path_clone)
                .ok()
                .and_then(|m| m.modified().ok());

            loop {
                std::thread::sleep(interval);

                if let Ok(metadata) = std::fs::metadata(&path_clone) {
                    if let Ok(modified) = metadata.modified() {
                        if Some(modified) > last_modified {
                            // File changed, try to reload
                            if let Ok(new_config) = ConfigBuilder::from_toml_file(&path_clone) {
                                if let Ok(new_config) = new_config.build() {
                                    if let Ok(mut guard) = config_clone.write() {
                                        *guard = new_config;
                                        tracing::info!(
                                            "Configuration reloaded from {:?}",
                                            path_clone
                                        );
                                    }
                                }
                            }
                            last_modified = Some(modified);
                        }
                    }
                }
            }
        });

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hot_reload_new() {
        let config = BrainConfig::default();
        let hot = HotReloadConfig::new(config);

        let retrieved = hot.get().unwrap();
        assert!(retrieved.memory.enabled);
    }

    #[test]
    fn test_hot_reload_update() {
        let config = BrainConfig::default();
        let hot = HotReloadConfig::new(config);

        hot.update(|c| c.memory.enabled = false).unwrap();

        let retrieved = hot.get().unwrap();
        assert!(!retrieved.memory.enabled);
    }

    #[test]
    fn test_auto_reload_callbacks() {
        let config = BrainConfig::default();
        let mut auto = AutoReloadConfig::new(config);

        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();

        auto.on_change(move |_config| {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        // Note: Without actual file changes, callback won't be triggered
        // This just tests the callback registration
    }

    #[test]
    fn test_has_changed_no_file() {
        let config = BrainConfig::default();
        let hot = HotReloadConfig::new(config);

        assert!(!hot.has_changed());
    }
}
