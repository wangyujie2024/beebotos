use crate::api::{Settings as ApiSettings, SettingsService, Theme};
use crate::state::use_app_state;
use crate::utils::{event_target_checked, event_target_value, use_theme, FormValidator, StringValidators};
use gloo_storage::Storage;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;

#[component]
pub fn SettingsPage() -> impl IntoView {
    let app_state = use_app_state();
    let theme_manager = use_theme();
    let settings = app_state.settings();
    let saving = RwSignal::new(false);
    let save_message = RwSignal::new(None::<String>);
    let validator = RwSignal::new(FormValidator::new());
    let error_message = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);

    // Form signals — local frontend-only settings
    let theme = RwSignal::new(Theme::Dark);
    let language = RwSignal::new("en".to_string());
    let notifications_enabled = RwSignal::new(true);
    let auto_update = RwSignal::new(true);
    let api_endpoint = RwSignal::new(String::new());
    let wallet_address = RwSignal::new(String::new());

    // Load from backend or localStorage fallback
    let app_state_for_load = app_state.clone();
    Effect::new(move |_| {
        let app_state = app_state_for_load.clone();
        loading.set(true);
        spawn_local(async move {
            let service = SettingsService::new(app_state.api_client());
            match service.get().await {
                Ok(s) => {
                    app_state.settings.set(s);
                    loading.set(false);
                }
                Err(_) => {
                    // Fallback to localStorage
                    let result: Result<String, _> = gloo_storage::LocalStorage::get("beebotos_settings");
                    if let Ok(stored) = result {
                        if let Ok(parsed) = serde_json::from_str::<ApiSettings>(&stored) {
                            app_state.settings.set(parsed);
                        }
                    }
                    loading.set(false);
                }
            }
        });
    });

    // Sync form signals from app state
    Effect::new(move |_| {
        let s = settings.get();
        theme.set(s.theme.clone());
        language.set(s.language.clone());
        notifications_enabled.set(s.notifications_enabled);
        auto_update.set(s.auto_update);
        api_endpoint.set(s.api_endpoint.clone().unwrap_or_default());
        wallet_address.set(s.wallet_address.clone().unwrap_or_default());
    });

    // Apply theme when changed
    Effect::new(move |_| {
        let t = theme.get();
        theme_manager.set_theme(t);
    });

    let validate = move || {
        let mut v = FormValidator::new();

        // Validate API endpoint if provided
        if !api_endpoint.get().is_empty() {
            v.validate(StringValidators::url("api_endpoint", &api_endpoint.get()));
        }

        // Validate wallet address if provided
        if !wallet_address.get().is_empty() {
            v.validate(StringValidators::ethereum_address(
                "wallet_address",
                &wallet_address.get(),
            ));
        }

        validator.set(v.clone());
        v.is_valid()
    };

    let on_save = move || {
        if !validate() {
            return;
        }

        let new_settings = ApiSettings {
            theme: theme.get(),
            language: language.get(),
            notifications_enabled: notifications_enabled.get(),
            auto_update: auto_update.get(),
            api_endpoint: if api_endpoint.get().is_empty() {
                None
            } else {
                Some(api_endpoint.get())
            },
            wallet_address: if wallet_address.get().is_empty() {
                None
            } else {
                Some(wallet_address.get())
            },
        };

        saving.set(true);
        save_message.set(None);
        error_message.set(None);

        // Save to backend and localStorage
        let app_state = app_state.clone();
        let settings_for_storage = new_settings.clone();
        spawn_local(async move {
            let service = SettingsService::new(app_state.api_client());
            match service.update(&settings_for_storage).await {
                Ok(_) => {
                    app_state.settings.set(settings_for_storage.clone());
                    let _ = gloo_storage::LocalStorage::set(
                        "beebotos_settings",
                        serde_json::to_string(&settings_for_storage).unwrap_or_default(),
                    );
                    saving.set(false);
                    save_message.set(Some("Settings saved successfully".to_string()));
                }
                Err(e) => {
                    // Fallback: save to localStorage
                    let _ = gloo_storage::LocalStorage::set(
                        "beebotos_settings",
                        serde_json::to_string(&settings_for_storage).unwrap_or_default(),
                    );
                    app_state.settings.set(settings_for_storage);
                    saving.set(false);
                    save_message.set(Some(format!("Saved locally (backend: {})", e)));
                }
            }
        });

        // Clear message after 3 seconds
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3000).await;
            save_message.set(None);
        });
    };
    let on_save_stored = StoredValue::new(on_save);

    view! {
        <Title text="Settings - BeeBotOS" />
        <div class="page settings-page">
            <div class="page-header">
                <h1>"Settings"</h1>
                <p class="page-description">"Manage your preferences and system configuration"</p>
            </div>

            {move || if loading.get() {
                view! {
                    <div class="loading-state">
                        <div class="spinner"></div>
                        <p>"Loading settings..."</p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <>
                        {move || error_message.get().map(|msg| view! {
                            <div class="alert alert-error">{msg}</div>
                        })}

                        <div class="settings-grid">
                            <section class="card settings-section">
                                <h2>"Appearance"</h2>

                                <div class="form-group">
                                    <label>"Theme"</label>
                                    <div class="theme-selector">
                                        <ThemeOption
                                            label="Dark"
                                            value=Theme::Dark
                                            current=theme
                                            icon="🌙"
                                        />
                                        <ThemeOption
                                            label="Light"
                                            value=Theme::Light
                                            current=theme
                                            icon="☀️"
                                        />
                                        <ThemeOption
                                            label="System"
                                            value=Theme::System
                                            current=theme
                                            icon="💻"
                                        />
                                    </div>
                                </div>

                                <div class="form-group">
                                    <label>"Language"</label>
                                    <select
                                        prop:value=language
                                        on:change=move |e| language.set(event_target_value(&e))
                                    >
                                        <option value="en">"English"</option>
                                        <option value="zh">"中文"</option>
                                        <option value="ja">"日本語"</option>
                                        <option value="ko">"한국어"</option>
                                    </select>
                                </div>
                            </section>

                            <section class="card settings-section">
                                <h2>"Notifications"</h2>

                                <div class="form-group checkbox-group">
                                    <label class="checkbox-label">
                                        <input
                                            type="checkbox"
                                            prop:checked=notifications_enabled
                                            on:change=move |e| notifications_enabled.set(event_target_checked(&e))
                                        />
                                        <span>"Enable notifications"</span>
                                    </label>
                                    <p class="form-help">"Receive alerts about agent status and DAO governance"</p>
                                </div>

                                <div class="form-group checkbox-group">
                                    <label class="checkbox-label">
                                        <input
                                            type="checkbox"
                                            prop:checked=auto_update
                                            on:change=move |e| auto_update.set(event_target_checked(&e))
                                        />
                                        <span>"Auto-update"</span>
                                    </label>
                                    <p class="form-help">"Automatically update to the latest version"</p>
                                </div>
                            </section>

                            <section class="card settings-section">
                                <h2>"Network"</h2>

                                <div class=move || format!("form-group {}",
                                    if validator.get().has_error("api_endpoint") { "has-error" } else { "" })>
                                    <label>"API Endpoint"</label>
                                    <input
                                        type="text"
                                        placeholder="https://api.beebotos.dev"
                                        prop:value=api_endpoint
                                        on:input=move |e| {
                                            api_endpoint.set(event_target_value(&e));
                                            validator.update(|v| {
                                                if !api_endpoint.get().is_empty() {
                                                    v.validate(StringValidators::url("api_endpoint", &api_endpoint.get()));
                                                }
                                            });
                                        }
                                    />
                                    <p class="form-help">"Custom API endpoint (leave empty for default)"</p>
                                    {move || validator.get().first_error_message("api_endpoint").map(|msg| view! {
                                        <span class="form-error">{msg}</span>
                                    })}
                                </div>
                            </section>

                            <section class="card settings-section">
                                <h2>"Wallet"</h2>

                                <div class=move || format!("form-group {}",
                                    if validator.get().has_error("wallet_address") { "has-error" } else { "" })>
                                    <label>"Wallet Address"</label>
                                    <input
                                        type="text"
                                        placeholder="0x..."
                                        prop:value=wallet_address
                                        on:input=move |e| {
                                            wallet_address.set(event_target_value(&e));
                                            validator.update(|v| {
                                                if !wallet_address.get().is_empty() {
                                                    v.validate(StringValidators::ethereum_address("wallet_address", &wallet_address.get()));
                                                }
                                            });
                                        }
                                    />
                                    <p class="form-help">"Your wallet address for DAO participation"</p>
                                    {move || validator.get().first_error_message("wallet_address").map(|msg| view! {
                                        <span class="form-error">{msg}</span>
                                    })}
                                </div>

                                <div class="wallet-actions">
                                    <button class="btn btn-secondary">"Connect Wallet"</button>
                                    <button class="btn btn-secondary">"Disconnect"</button>
                                </div>
                            </section>

                            <section class="card settings-section">
                                <h2>"AI Configuration"</h2>
                                <p class="form-help">"View global LLM provider settings and metrics"</p>
                                <button
                                    class="btn btn-secondary"
                                    on:click=move |_| {
                                        let navigate = leptos_router::hooks::use_navigate();
                                        navigate("/llm-config", Default::default());
                                    }
                                >
                                    "Open LLM Configuration →"
                                </button>
                            </section>

                            <section class="card settings-section">
                                <h2>"Gateway Setup"</h2>
                                <p class="form-help">"Run the configuration wizard to setup or reconfigure Gateway"</p>
                                <button
                                    class="btn btn-secondary"
                                    on:click=move |_| {
                                        let navigate = leptos_router::hooks::use_navigate();
                                        navigate("/settings/wizard", Default::default());
                                    }
                                >
                                    "Configuration Wizard →"
                                </button>
                            </section>

                            <section class="card settings-section">
                                <h2>"System"</h2>

                                <div class="system-info">
                                    <div class="info-row">
                                        <span>"Version"</span>
                                        <span>"v2.0.0"</span>
                                    </div>
                                    <div class="info-row">
                                        <span>"Build"</span>
                                        <span>"release-2024.03.22"</span>
                                    </div>
                                    <div class="info-row">
                                        <span>"Platform"</span>
                                        <span>"WebAssembly"</span>
                                    </div>
                                </div>

                                <div class="system-actions">
                                    <button class="btn btn-secondary">"Check for Updates"</button>
                                    <button
                                        class="btn btn-secondary"
                                        on:click=move |_| {
                                            let client = crate::api::create_client();
                                            spawn_local(async move {
                                                match client.post::<serde_json::Value, _>("/admin/config/reload", &serde_json::json!({})).await {
                                                    Ok(resp) => {
                                                        let msg = resp.get("message").and_then(|v| v.as_str()).unwrap_or("Config reloaded");
                                                        save_message.set(Some(msg.to_string()));
                                                    }
                                                    Err(e) => {
                                                        error_message.set(Some(format!("Reload failed: {}", e)));
                                                    }
                                                }
                                            });
                                        }
                                    >
                                        "Reload Config"
                                    </button>
                                    <button class="btn btn-danger">"Reset to Defaults"</button>
                                </div>
                            </section>
                        </div>

                        <div class="settings-footer">
                            {move || save_message.get().map(|msg| view! {
                                <div class="save-message success">{msg}</div>
                            })}
                            {move || error_message.get().map(|msg| view! {
                                <div class="save-message error">{msg}</div>
                            })}

                            <div class="settings-actions">
                                <button
                                    class="btn btn-primary"
                                    on:click=move |_| on_save_stored.get_value()()
                                    disabled=saving
                                >
                                    {move || if saving.get() {
                                        "Saving..."
                                    } else {
                                        "Save Changes"
                                    }}
                                </button>
                            </div>
                        </div>
                    </>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn ThemeOption(
    #[prop(into)] label: String,
    value: Theme,
    current: RwSignal<Theme>,
    #[prop(into)] icon: String,
) -> impl IntoView {
    let value_for_check = value.clone();
    let is_selected = move || current.get() == value_for_check;

    view! {
        <button
            class=move || format!("theme-option {}", if is_selected() { "selected" } else { "" })
            on:click=move |_| current.set(value.clone())
        >
            <span class="theme-icon">{icon}</span>
            <span class="theme-label">{label}</span>
        </button>
    }
}


