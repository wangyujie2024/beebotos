//! LLM Provider Management Page

use crate::api::llm_provider_service::LlmProvider;
use crate::pages::llm_provider_modals::{AddProviderModal, ModelManageModal, ProviderConfigModal};
use crate::state::use_app_state;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn LlmProvidersPage() -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());
    let providers_data: RwSignal<Option<Vec<LlmProvider>>> = RwSignal::new(None);
    let is_loading = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);

    let selected_provider: RwSignal<Option<LlmProvider>> = RwSignal::new(None);
    let show_config_modal = RwSignal::new(false);
    let show_model_modal = RwSignal::new(false);
    let show_add_modal = RwSignal::new(false);

    let fetch_providers = move || {
        is_loading.set(true);
        error_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        spawn_local(async move {
            match service.list_providers().await {
                Ok(resp) => {
                    providers_data.set(Some(resp.providers));
                    is_loading.set(false);
                }
                Err(e) => {
                    error_msg.set(Some(format!("加载失败: {}", e)));
                    is_loading.set(false);
                }
            }
        });
    };

    // Initial load
    let refresh = StoredValue::new(fetch_providers);
    Effect::new(move |_| {
        refresh.get_value()();
    });

    view! {
        <div class="page llm-providers-page">
            <div class="page-header">
                <h1>"模型管理"</h1>
                <button class="btn btn-primary" on:click=move |_| show_add_modal.set(true)>
                    "添加自定义提供商"
                </button>
            </div>

            {move || {
                if is_loading.get() {
                    view! { <div class="loading">"加载中..."</div> }.into_any()
                } else if let Some(error) = error_msg.get() {
                    view! {
                        <div class="error-state">
                            <div class="error-icon">"⚠️"</div>
                            <p>{error}</p>
                            <button class="btn btn-primary" on:click=move |_| refresh.get_value()()>
                                "重试"
                            </button>
                        </div>
                    }.into_any()
                } else if let Some(providers) = providers_data.get() {
                    if providers.is_empty() {
                        view! { <div class="empty-state">"暂无提供商数据"</div> }.into_any()
                    } else {
                        view! {
                            <div class="providers-grid">
                                {providers.into_iter().map(|provider| {
                                    let provider_id = provider.id;
                                    let provider_name = provider.name.clone();
                                    let provider_protocol = provider.protocol.clone();
                                    let provider_model_count = provider.models.len();
                                    let provider_base_url = provider.base_url.clone();
                                    let provider_is_default = provider.is_default_provider;
                                    let provider_enabled = provider.enabled;
                                    let provider_for_config = provider.clone();
                                    let provider_for_model = provider.clone();
                                    view! {
                                        <div
                                            class="provider-card"
                                            class:default=provider_is_default
                                            class:disabled=!provider_enabled
                                        >
                                            <div class="provider-header">
                                                <h3>{provider_name.clone()}</h3>
                                                <div class="provider-badges">
                                                    {if provider_is_default {
                                                        view! { <span class="badge default">"默认"</span> }.into_any()
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }}
                                                    {if !provider_enabled {
                                                        view! { <span class="badge disabled">"已禁用"</span> }.into_any()
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }}
                                                </div>
                                            </div>
                                            <div class="provider-meta">
                                                <span class="protocol">{provider_protocol.clone()}</span>
                                                <span class="model-count">
                                                    {format!("{} 个模型", provider_model_count)}
                                                </span>
                                            </div>
                                            <div class="provider-url">
                                                {provider_base_url.clone().unwrap_or_else(|| "未配置".to_string())}
                                            </div>
                                            <div class="provider-actions">
                                                {if !provider_is_default {
                                                    view! {
                                                        <button
                                                            class="btn btn-sm btn-secondary"
                                                            on:click=move |_| {
                                                                let service = app_state.get_value().llm_provider_service();
                                                                spawn_local(async move {
                                                                    let _ = service.set_default_provider(provider_id).await;
                                                                    refresh.get_value()();
                                                                });
                                                            }
                                                        >
                                                            "设为默认"
                                                        </button>
                                                    }.into_any()
                                                } else {
                                                    view! { <span></span> }.into_any()
                                                }}
                                                <button
                                                    class="btn btn-sm btn-secondary"
                                                    on:click=move |_| {
                                                        selected_provider.set(Some(provider_for_config.clone()));
                                                        show_config_modal.set(true);
                                                    }
                                                >
                                                    "配置"
                                                </button>
                                                <button
                                                    class="btn btn-sm btn-secondary"
                                                    on:click=move |_| {
                                                        selected_provider.set(Some(provider_for_model.clone()));
                                                        show_model_modal.set(true);
                                                    }
                                                >
                                                    "模型管理"
                                                </button>
                                                <button
                                                    class="btn btn-sm btn-danger"
                                                    on:click=move |_| {
                                                        let service = app_state.get_value().llm_provider_service();
                                                        spawn_local(async move {
                                                            let _ = service.delete_provider(provider_id).await;
                                                            refresh.get_value()();
                                                        });
                                                    }
                                                >
                                                    "删除"
                                                </button>
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                } else {
                    view! { <div>"暂无数据"</div> }.into_any()
                }
            }}

            <Show when=move || show_config_modal.get()>
                {move || selected_provider.get().map(|p| view! {
                    <ProviderConfigModal
                        provider=p
                        on_close=move || show_config_modal.set(false)
                        on_updated=move || { refresh.get_value()(); }
                    />
                })}
            </Show>

            <Show when=move || show_model_modal.get()>
                {move || selected_provider.get().map(|p| view! {
                    <ModelManageModal
                        provider=p
                        on_close=move || show_model_modal.set(false)
                        on_updated=move || { refresh.get_value()(); }
                    />
                })}
            </Show>

            <Show when=move || show_add_modal.get()>
                <AddProviderModal
                    on_close=move || show_add_modal.set(false)
                    on_created=move || { refresh.get_value()(); }
                />
            </Show>
        </div>
    }
}
