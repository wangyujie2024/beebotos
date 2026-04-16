use gloo_storage::{LocalStorage, Storage};

pub mod security;
pub mod theme;
pub mod validation;

pub use security::{
    contains_dangerous_html, escape_html, escape_html_attribute, sanitize_filename, sanitize_url,
};
pub use theme::{provide_theme, use_theme, ThemeManager, ThemeSelector, ThemeToggle};
pub use validation::{
    combine, CollectionValidators, FormValidator, NumericValidators, StringValidators,
    ValidationError, ValidationResult,
};

/// 获取或创建持久化的用户 ID
pub fn get_user_id() -> String {
    LocalStorage::get("beebotos_webchat_user_id")
        .unwrap_or_else(|_| {
            let id = uuid::Uuid::new_v4().to_string();
            let _ = LocalStorage::set("beebotos_webchat_user_id", &id);
            id
        })
}
