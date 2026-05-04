//! LLM Model Settings Page
//!
//! Allows users to select and configure the active LLM model provider,
//! choose model versions (e.g. kimi-k2.6 thinking/fast), and hot-reload config.

use crate::api::{LlmGlobalConfig, UpdateLlmConfigRequest};
use crate::components::InlineLoading;
use crate::state::use_app_state;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;

/// Predefined model options for Kimi provider.
/// Note: kimi-k2.6 (reasoning model) only supports temperature=1.0.
const KIMI_MODELS: &[(&str, &str, f32)] = &[
    ("kimi-k2.6", "kimi-k2.6 思考版", 1.0),
    ("kimi-k2.5", "kimi-k2.5 思考版", 1.0),
    ("kimi-k2.5", "kimi-k2.5 快速版", 0.6),
];

/// Find the display label for a given model + temperature combo.
fn find_kimi_label(model: &str, temperature: f32) -> Option<&'static str> {
    KIMI_MODELS
        .iter()
        .find(|(m, _, t)| *m == model && (*t - temperature).abs() < 0.01)
        .map(|(_, label, _)| *label)
}

#[component]
pub fn LlmSettingsPage() -> impl IntoView {
    let config: RwSignal<Option<LlmGlobalConfig>> = RwSignal::new(None);
    let loading = RwSignal::new(true);
    let saving = RwSignal::new(false);
    let reloading = RwSignal::new(false);
    let message: RwSignal<Option<String>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let save_error: RwSignal<Option<String>> = RwSignal::new(None);

    // Form state
    let selected_provider = RwSignal::new(String::new());
    let selected_model = RwSignal::new(String::new());
    let selected_temperature = RwSignal::new(1.0_f32);
    let selected_variant_label = RwSignal::new(String::new());

    let fetch_config = move || {
        let service = use_app_state().llm_config_service();
        loading.set(true);
        error.set(None);
        message.set(None);
        save_error.set(None);
        spawn_local(async move {
            match service.get_config().await {
                Ok(c) => {
                    // Auto-select default provider
                    let default = c.default_provider.clone();
                    if let Some(provider) = c.providers.iter().find(|p| p.name == default) {
                        selected_provider.set(provider.name.clone());
                        selected_model.set(provider.model.clone());
                        selected_temperature.set(provider.temperature);
                        // Try to match variant label
                        if provider.name == "kimi" {
                            // Auto-correct k2.6 to temperature=1.0 (API restriction)
                            let temp = if provider.model.contains("k2.6") {
                                1.0
                            } else {
                                provider.temperature
                            };
                            selected_temperature.set(temp);
                            let label = find_kimi_label(&provider.model, temp)
                                .unwrap_or(&provider.model);
                            selected_variant_label.set(label.to_string());
                        } else {
                            selected_variant_label.set(provider.model.clone());
                        }
                    }
                    config.set(Some(c));
                }
                Err(e) => error.set(Some(format!("加载配置失败: {}", e))),
            }
            loading.set(false);
        });
    };

    let fetch_stored = StoredValue::new(fetch_config);

    Effect::new(move |_| {
        fetch_stored.get_value()();
    });

    // When provider changes, reset model selection
    let on_provider_change = move |provider: String| {
        selected_provider.set(provider.clone());
        selected_model.set(String::new());
        selected_variant_label.set(String::new());
        selected_temperature.set(1.0);
        save_error.set(None);
    };

    // When model variant changes for kimi
    let on_kimi_variant_change = move |label: String| {
        selected_variant_label.set(label.clone());
        if let Some((model, _, temp)) = KIMI_MODELS.iter().find(|(_, l, _)| *l == label) {
            selected_model.set(model.to_string());
            selected_temperature.set(*temp);
        }
    };

    view! {
        <Title text="大模型设置 - BeeBotOS" />
        <div class="page llm-settings-page">
            <div class="page-header">
                <h1>"大模型设置"</h1>
                <p class="page-description">"选择并配置当前使用的大语言模型及其参数"</p>
            </div>

            {move || if loading.get() {
                view! { <InlineLoading /> }.into_any()
            } else if let Some(err) = error.get() {
                view! {
                    <div class="error-state">
                        <div class="error-icon">"⚠️"</div>
                        <p>{err}</p>
                        <button class="btn btn-primary" on:click=move |_| fetch_stored.get_value()()>
                            "重试"
                        </button>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="llm-settings-grid">
                        // Provider Selection
                        <section class="card llm-settings-section">
                            <h2>"模型提供商"</h2>
                            <div class="form-group">
                                <label>"选择提供商"</label>
                                <select
                                    prop:value=selected_provider
                                    on:change=move |e| {
                                        let val = crate::utils::event_target_value(&e);
                                        on_provider_change(val);
                                    }
                                >
                                    <option value="">"-- 请选择 --"</option>
                                    {move || {
                                        config.get()
                                            .map(|c| c.providers)
                                            .unwrap_or_default()
                                            .into_iter()
                                            .map(|p| {
                                                let name = p.name.clone();
                                                view! {
                                                    <option value={name.clone()}>{name.clone()}</option>
                                                }
                                            })
                                            .collect::<Vec<_>>()
                                    }}
                                </select>
                            </div>
                        </section>

                        // Model Selection
                        <section class="card llm-settings-section">
                            <h2>"模型版本"</h2>
                            {move || {
                                let provider = selected_provider.get();
                                if provider == "kimi" {
                                    view! {
                                        <div class="form-group">
                                            <label>"选择 Kimi 模型"</label>
                                            <select
                                                prop:value=selected_variant_label
                                                on:change=move |e| {
                                                    let val = crate::utils::event_target_value(&e);
                                                    on_kimi_variant_change(val);
                                                }
                                            >
                                                <option value="">"-- 请选择 --"</option>
                                                {KIMI_MODELS.iter().map(|(_, label, _)| {
                                                    let label = label.to_string();
                                                    view! {
                                                        <option value={label.clone()}>{label.clone()}</option>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </select>
                                            <p class="form-help">
                                                "kimi-k2.6 仅支持 temperature=1.0（思考版）；kimi-k2.5 支持思考版(1.0)和快速版(0.6)"
                                            </p>
                                        </div>
                                    }.into_any()
                                } else if !provider.is_empty() {
                                    view! {
                                        <div class="form-group">
                                            <label>"模型名称"</label>
                                            <input
                                                type="text"
                                                prop:value=selected_model
                                                on:input=move |e| {
                                                    selected_model.set(crate::utils::event_target_value(&e));
                                                }
                                                placeholder="例如: gpt-4o"
                                            />
                                        </div>
                                        <div class="form-group">
                                            <label>"Temperature"</label>
                                            <input
                                                type="number"
                                                step="0.1"
                                                min="0"
                                                max="2"
                                                prop:value=move || format!("{:.1}", selected_temperature.get())
                                                on:input=move |e| {
                                                    if let Ok(v) = crate::utils::event_target_value(&e).parse::<f32>() {
                                                        selected_temperature.set(v.clamp(0.0, 2.0));
                                                    }
                                                }
                                            />
                                            <p class="form-help">
                                                "取值范围 0.0 ~ 2.0，越低越确定，越高越 creative"
                                            </p>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <p class="form-help">"请先选择模型提供商"</p>
                                    }.into_any()
                                }
                            }}
                        </section>

                        // Current Parameters Summary
                        <section class="card llm-settings-section">
                            <h2>"当前参数"</h2>
                            <div class="info-grid">
                                <div class="info-row">
                                    <span>"提供商"</span>
                                    <span class="info-value">{move || selected_provider.get()}</span>
                                </div>
                                <div class="info-row">
                                    <span>"模型"</span>
                                    <span class="info-value">{move || selected_model.get()}</span>
                                </div>
                                <div class="info-row">
                                    <span>"Temperature"</span>
                                    <span class="info-value">{move || format!("{:.1}", selected_temperature.get())}</span>
                                </div>
                            </div>
                        </section>

                        // Actions
                        <section class="card llm-settings-section">
                            <h2>"操作"</h2>
                            {move || message.get().map(|msg| view! {
                                <div class="save-message success">{msg}</div>
                            })}
                            {move || save_error.get().map(|err| view! {
                                <div class="save-message error">{err}</div>
                            })}
                            <div class="form-actions">
                                <button
                                    class="btn btn-primary"
                                    on:click=move |_| {
                                        if selected_provider.get().is_empty() || selected_model.get().is_empty() {
                                            save_error.set(Some("请选择提供商和模型".to_string()));
                                            return;
                                        }

                                        let req = UpdateLlmConfigRequest {
                                            provider: selected_provider.get(),
                                            model: selected_model.get(),
                                            temperature: selected_temperature.get(),
                                            set_default: Some(true),
                                        };

                                        saving.set(true);
                                        save_error.set(None);
                                        message.set(None);

                                        let service = use_app_state().llm_config_service();
                                        spawn_local(async move {
                                            match service.update_config(&req).await {
                                                Ok(resp) => {
                                                    let msg = resp
                                                        .get("message")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("保存成功");
                                                    message.set(Some(msg.to_string()));
                                                }
                                                Err(e) => save_error.set(Some(format!("保存失败: {}", e))),
                                            }
                                            saving.set(false);
                                        });
                                    }
                                    disabled=saving
                                >
                                    {move || if saving.get() { "保存中..." } else { "保存配置" }}
                                </button>
                                <button
                                    class="btn btn-secondary"
                                    on:click=move |_| {
                                        reloading.set(true);
                                        save_error.set(None);
                                        message.set(None);

                                        let client = use_app_state().api_client();
                                        spawn_local(async move {
                                            match client
                                                .post::<serde_json::Value, _>("/admin/config/reload", &serde_json::json!({}))
                                                .await
                                            {
                                                Ok(resp) => {
                                                    let msg = resp
                                                        .get("message")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("配置已重载");
                                                    message.set(Some(msg.to_string()));
                                                }
                                                Err(e) => save_error.set(Some(format!("重载失败: {}", e))),
                                            }
                                            reloading.set(false);
                                        });
                                    }
                                    disabled=reloading
                                >
                                    {move || if reloading.get() { "重载中..." } else { "重启生效 (Reload)" }}
                                </button>
                            </div>
                            <p class="form-help">
                                "保存后会自动写入 config/beebotos.toml 并热重载。如需完全生效，请点击"重启生效"。"
                            </p>
                        </section>
                    </div>
                }.into_any()
            }}
        </div>
    }
}
