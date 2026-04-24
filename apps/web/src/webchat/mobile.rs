//! 移动端 Web 适配
//!
//! 提供 iOS 和 Android 平台特定的优化

use serde::{Deserialize, Serialize};

/// 移动平台类型
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MobilePlatform {
    Ios,
    Android,
    Desktop,
}

/// 移动配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobileConfig {
    pub platform: MobilePlatform,
    pub optimize_for_size: bool,
    pub enable_offline: bool,
    pub welcome_shown: bool,
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            platform: MobilePlatform::Desktop,
            optimize_for_size: false,
            enable_offline: true,
            welcome_shown: false,
        }
    }
}

/// iOS 特定功能
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IosFeatures {
    pub welcome_guide_enabled: bool,
    pub qr_pairing_enabled: bool,
    pub safari_optimizations: bool,
}

impl Default for IosFeatures {
    fn default() -> Self {
        Self {
            welcome_guide_enabled: true,
            qr_pairing_enabled: true,
            safari_optimizations: true,
        }
    }
}

/// Android 特定功能
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AndroidFeatures {
    pub app_size_mb: f32,
    pub chat_settings_redesign: bool,
    pub device_media_grouping: bool,
}

impl Default for AndroidFeatures {
    fn default() -> Self {
        Self {
            app_size_mb: 7.0,
            chat_settings_redesign: true,
            device_media_grouping: true,
        }
    }
}

/// 移动适配器
#[derive(Clone, Debug)]
pub struct MobileAdapter {
    config: MobileConfig,
    ios_features: IosFeatures,
    android_features: AndroidFeatures,
}

impl MobileAdapter {
    pub fn new() -> Self {
        let platform = Self::detect_platform();

        Self {
            config: MobileConfig {
                platform,
                ..Default::default()
            },
            ios_features: IosFeatures::default(),
            android_features: AndroidFeatures::default(),
        }
    }

    /// 检测平台
    fn detect_platform() -> MobilePlatform {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return MobilePlatform::Desktop;
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                let user_agent = navigator.user_agent().unwrap_or_default().to_lowercase();

                if user_agent.contains("iphone") || user_agent.contains("ipad") {
                    MobilePlatform::Ios
                } else if user_agent.contains("android") {
                    MobilePlatform::Android
                } else {
                    MobilePlatform::Desktop
                }
            } else {
                MobilePlatform::Desktop
            }
        }
    }

    /// 获取平台
    pub fn platform(&self) -> &MobilePlatform {
        &self.config.platform
    }

    /// 检查是否是移动端
    pub fn is_mobile(&self) -> bool {
        matches!(
            self.config.platform,
            MobilePlatform::Ios | MobilePlatform::Android
        )
    }

    /// 检查是否是 iOS
    pub fn is_ios(&self) -> bool {
        matches!(self.config.platform, MobilePlatform::Ios)
    }

    /// 检查是否是 Android
    pub fn is_android(&self) -> bool {
        matches!(self.config.platform, MobilePlatform::Android)
    }

    /// 获取 iOS 特性
    pub fn ios_features(&self) -> &IosFeatures {
        &self.ios_features
    }

    /// 获取 Android 特性
    pub fn android_features(&self) -> &AndroidFeatures {
        &self.android_features
    }

    /// 应用移动端优化
    pub fn apply_optimizations(&self) {
        if self.is_mobile() {
            // 添加移动端特定的 CSS 类
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                if let Some(body) = document.body() {
                    let _ = body.class_list().add_1("mobile-optimized");

                    match self.config.platform {
                        MobilePlatform::Ios => {
                            let _ = body.class_list().add_1("ios");
                        }
                        MobilePlatform::Android => {
                            let _ = body.class_list().add_1("android");
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

impl Default for MobileAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// 触摸手势支持
pub struct TouchGestureHandler;

impl TouchGestureHandler {
    /// 初始化触摸手势
    pub fn init() {
        // 在实际实现中，这里会添加触摸事件监听器
    }

    /// 处理滑动手势
    pub fn handle_swipe(direction: SwipeDirection) {
        match direction {
            SwipeDirection::Left => {
                // 关闭侧边栏或切换到下一个
            }
            SwipeDirection::Right => {
                // 打开侧边栏或切换到上一个
            }
            SwipeDirection::Up => {
                // 滚动到顶部或刷新
            }
            SwipeDirection::Down => {
                // 滚动到底部
            }
        }
    }
}

/// 滑动方向
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SwipeDirection {
    Left,
    Right,
    Up,
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mobile_platform_detection() {
        let adapter = MobileAdapter::new();
        // 在测试环境中应该是 Desktop
        assert_eq!(*adapter.platform(), MobilePlatform::Desktop);
    }

    #[test]
    fn test_ios_features() {
        let features = IosFeatures::default();
        assert!(features.welcome_guide_enabled);
        assert!(features.qr_pairing_enabled);
    }

    #[test]
    fn test_android_features() {
        let features = AndroidFeatures::default();
        assert_eq!(features.app_size_mb, 7.0);
        assert!(features.chat_settings_redesign);
    }
}
