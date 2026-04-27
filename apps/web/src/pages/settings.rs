use gloo_storage::Storage;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;

use crate::api::{Settings as ApiSettings, SettingsService};
use crate::i18n::I18nContext;
use crate::state::use_app_state;
use crate::utils::{event_target_value, FormValidator, StringValidators};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n.clone());
    let app_state = use_app_state();
    let settings = app_state.settings();
    let saving = RwSignal::new(false);
    let save_message = RwSignal::new(None::<String>);
    let validator = RwSignal::new(FormValidator::new());
    let error_message = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);

    // 仅保留钱包地址
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
                    let result: Result<String, _> =
                        gloo_storage::LocalStorage::get("beebotos_settings");
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
        wallet_address.set(s.wallet_address.clone().unwrap_or_default());
    });

    let validate = move || {
        let mut v = FormValidator::new();

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
            // 保留字段的默认值，避免破坏后端结构
            theme: settings.get().theme.clone(),
            language: settings.get().language.clone(),
            notifications_enabled: settings.get().notifications_enabled,
            auto_update: settings.get().auto_update,
            api_endpoint: settings.get().api_endpoint.clone(),
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
        let i18n = i18n_stored.get_value();
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
                    save_message.set(Some(i18n.t("settings-save-success")));
                }
                Err(e) => {
                    // Fallback: save to localStorage
                    let _ = gloo_storage::LocalStorage::set(
                        "beebotos_settings",
                        serde_json::to_string(&settings_for_storage).unwrap_or_default(),
                    );
                    app_state.settings.set(settings_for_storage);
                    saving.set(false);
                    save_message.set(Some(format!(
                        "{} (backend: {})",
                        i18n.t("settings-save-local"),
                        e
                    )));
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
        <Title text=move || i18n_stored.get_value().t("settings-page-title") />
        <div class="page settings-page">
            <div class="page-header">
                <h1>{i18n_stored.get_value().t("settings-heading")}</h1>
                <p class="page-description">{i18n_stored.get_value().t("settings-description")}</p>
            </div>

            {move || if loading.get() {
                view! {
                    <div class="loading-state">
                        <div class="spinner"></div>
                        <p>{i18n_stored.get_value().t("settings-loading")}</p>
                    </div>
                }.into_any()
            } else {
                let i18n = i18n_stored.get_value();
                view! {
                    <>
                        {move || error_message.get().map(|msg| view! {
                            <div class="alert alert-error">{msg}</div>
                        })}

                        <div class="settings-grid">
                            <section class="card settings-section">
                                <h2>{i18n.t("settings-wallet")}</h2>

                                <div class=move || format!("form-group {}",
                                    if validator.get().has_error("wallet_address") { "has-error" } else { "" })>
                                    <label>{i18n.t("settings-wallet-address")}</label>
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
                                    <p class="form-help">{i18n.t("settings-wallet-help")}</p>
                                    {move || validator.get().first_error_message("wallet_address").map(|msg| view! {
                                        <span class="form-error">{msg}</span>
                                    })}
                                </div>

                                <div class="wallet-actions">
                                    <button class="btn btn-secondary">{i18n.t("settings-connect-wallet")}</button>
                                    <button class="btn btn-secondary">{i18n.t("settings-disconnect-wallet")}</button>
                                </div>
                            </section>

                            <section class="card settings-section">
                                <h2>{i18n.t("settings-system")}</h2>

                                <div class="system-info">
                                    <div class="info-row">
                                        <span>{i18n.t("settings-version")}</span>
                                        <span>"v2.0.0"</span>
                                    </div>
                                    <div class="info-row">
                                        <span>{i18n.t("settings-build")}</span>
                                        <span>"release-2024.03.22"</span>
                                    </div>
                                    <div class="info-row">
                                        <span>{i18n.t("settings-platform")}</span>
                                        <span>"WebAssembly"</span>
                                    </div>
                                </div>

                                <div class="system-actions">
                                    <button class="btn btn-secondary">{i18n.t("settings-check-updates")}</button>
                                    <button
                                        class="btn btn-secondary"
                                        on:click=move |_| {
                                            let client = crate::api::create_client();
                                            let i18n = i18n_stored.get_value();
                                            spawn_local(async move {
                                                match client.post::<serde_json::Value, _>("/admin/config/reload", &serde_json::json!({})).await {
                                                    Ok(resp) => {
                                                        let msg = resp.get("message").and_then(|v| v.as_str()).unwrap_or("Config reloaded");
                                                        save_message.set(Some(msg.to_string()));
                                                    }
                                                    Err(e) => {
                                                        error_message.set(Some(format!("{}: {}", i18n.t("settings-reload-failed"), e)));
                                                    }
                                                }
                                            });
                                        }
                                    >
                                        {i18n.t("settings-reload-config")}
                                    </button>
                                    <button class="btn btn-danger">{i18n.t("settings-reset-defaults")}</button>
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
                                        i18n.t("settings-saving")
                                    } else {
                                        i18n.t("settings-save-changes")
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
