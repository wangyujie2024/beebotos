use gloo_storage::Storage;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::Theme;

/// Theme manager
#[derive(Clone)]
pub struct ThemeManager {
    current_theme: RwSignal<Theme>,
}

impl ThemeManager {
    pub fn new() -> Self {
        let initial_theme = Self::detect_initial_theme();

        let manager = Self {
            current_theme: RwSignal::new(initial_theme.clone()),
        };

        manager.apply_theme(initial_theme.clone());
        manager.setup_system_theme_listener();

        manager
    }

    pub fn current(&self) -> RwSignal<Theme> {
        self.current_theme
    }

    pub fn set_theme(&self, theme: Theme) {
        self.current_theme.set(theme.clone());
        self.apply_theme(theme.clone());
        self.save_to_storage(theme);
    }

    pub fn toggle(&self) {
        let new_theme = match self.current_theme.get() {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
            Theme::System => {
                if Self::get_system_theme() == Theme::Dark {
                    Theme::Light
                } else {
                    Theme::Dark
                }
            }
        };
        self.set_theme(new_theme);
    }

    fn detect_initial_theme() -> Theme {
        // Try to load from localStorage
        if let Ok(Some(storage)) = gloo_storage::LocalStorage::raw().get_item("theme") {
            match storage.as_str() {
                "dark" => return Theme::Dark,
                "light" => return Theme::Light,
                "system" => return Theme::System,
                _ => {}
            }
        }

        // Check system preference
        Self::get_system_theme()
    }

    fn get_system_theme() -> Theme {
        if let Some(window) = web_sys::window() {
            if let Ok(media_query) = window.match_media("(prefers-color-scheme: dark)") {
                if let Some(mq) = media_query {
                    if mq.matches() {
                        return Theme::Dark;
                    }
                }
            }
        }
        Theme::Light
    }

    fn apply_theme(&self, theme: Theme) {
        let effective_theme = match theme {
            Theme::System => Self::get_system_theme(),
            _ => theme,
        };

        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Some(html) = document.document_element() {
                let class_name = match effective_theme {
                    Theme::Dark => "theme-dark",
                    Theme::Light => "theme-light",
                    _ => "theme-dark",
                };

                // Remove existing theme classes
                let _ = html.class_list().remove_1("theme-dark");
                let _ = html.class_list().remove_1("theme-light");

                // Add new theme class
                let _ = html.class_list().add_1(class_name);

                // Set data attribute
                let _ = html.set_attribute("data-theme", class_name);
            }
        }
    }

    fn save_to_storage(&self, theme: Theme) {
        let theme_str = match theme {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::System => "system",
        };
        let _ = gloo_storage::LocalStorage::raw().set_item("theme", theme_str);
    }

    fn setup_system_theme_listener(&self) {
        if let Some(window) = web_sys::window() {
            if let Ok(media_query) = window.match_media("(prefers-color-scheme: dark)") {
                if let Some(mq) = media_query {
                    let manager = self.clone();
                    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(
                        move |_event: web_sys::MediaQueryListEvent| {
                            if manager.current_theme.get() == Theme::System {
                                manager.apply_theme(Theme::System);
                            }
                        },
                    )
                        as Box<dyn FnMut(_)>);

                    let _ = mq.add_event_listener_with_callback(
                        "change",
                        closure.as_ref().unchecked_ref(),
                    );
                    closure.forget();
                }
            }
        }
    }
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Provide theme context
pub fn provide_theme() {
    provide_context(ThemeManager::new());
}

/// Use theme context
pub fn use_theme() -> ThemeManager {
    use_context::<ThemeManager>().expect("ThemeManager not provided")
}

/// Theme toggle button component
#[component]
pub fn ThemeToggle() -> impl IntoView {
    let theme = use_theme();
    let theme_for_click = theme.clone();

    view! {
        <button
            class="theme-toggle"
            on:click=move |_| theme_for_click.toggle()
            title="Toggle theme"
        >
            {move || match theme.current().get() {
                Theme::Dark | Theme::System => view! { "🌙" },
                Theme::Light => view! { "☀️" },
            }}
        </button>
    }
}

/// Theme selector dropdown
#[component]
pub fn ThemeSelector() -> impl IntoView {
    let theme = use_theme();

    view! {
        <div class="theme-selector">
            <label>"Theme:"</label>
            <select
                prop:value={
                    let theme = theme.clone();
                    move || format!("{:?}", theme.current().get()).to_lowercase()
                }
                on:change=move |e| {
                    let value = event_target_value(&e);
                    let new_theme = match value.as_str() {
                        "dark" => Theme::Dark,
                        "light" => Theme::Light,
                        _ => Theme::System,
                    };
                    theme.set_theme(new_theme);
                }
            >
                <option value="dark">"Dark"</option>
                <option value="light">"Light"</option>
                <option value="system">"System"</option>
            </select>
        </div>
    }
}
