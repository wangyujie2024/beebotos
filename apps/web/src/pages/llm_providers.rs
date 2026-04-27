//! LLM Provider Management Page
//!
//! Dark theme provider grid with default LLM selector.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::llm_provider_service::LlmProvider;
use crate::pages::llm_provider_modals::{AddProviderModal, ModelManageModal, ProviderConfigModal};
use crate::state::use_app_state;

#[component]
pub fn LlmProvidersPage() -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());

    let providers: RwSignal<Option<Vec<LlmProvider>>> = RwSignal::new(None);
    let is_loading = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);
    let search_query = RwSignal::new(String::new());

    // Modal states
    let selected_provider: RwSignal<Option<LlmProvider>> = RwSignal::new(None);
    let show_config_modal = RwSignal::new(false);
    let show_model_modal = RwSignal::new(false);
    let show_add_modal = RwSignal::new(false);

    // Default LLM selection
    let default_provider_id: RwSignal<Option<i64>> = RwSignal::new(None);
    let default_model_name: RwSignal<Option<String>> = RwSignal::new(None);

    let fetch_providers = move || {
        is_loading.set(true);
        error_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        spawn_local(async move {
            match service.list_providers().await {
                Ok(resp) => {
                    // Find default provider and model
                    for p in &resp.providers {
                        if p.is_default_provider {
                            default_provider_id.set(Some(p.id));
                            if let Some(m) = p.models.iter().find(|m| m.is_default_model) {
                                default_model_name.set(Some(m.name.clone()));
                            } else if let Some(m) = p.models.first() {
                                default_model_name.set(Some(m.name.clone()));
                            }
                            break;
                        }
                    }
                    providers.set(Some(resp.providers));
                    is_loading.set(false);
                }
                Err(e) => {
                    error_msg.set(Some(format!("加载失败: {}", e)));
                    is_loading.set(false);
                }
            }
        });
    };

    let refresh = StoredValue::new(fetch_providers);

    Effect::new(move |_| {
        refresh.get_value()();
    });

    // Filtered providers based on search
    let filtered_providers = move || {
        let query = search_query.get().to_lowercase();
        providers.get().map(|list| {
            if query.is_empty() {
                list
            } else {
                list.into_iter()
                    .filter(|p| {
                        p.name.to_lowercase().contains(&query)
                            || p.provider_id.to_lowercase().contains(&query)
                            || p.base_url
                                .as_ref()
                                .map(|u| u.to_lowercase().contains(&query))
                                .unwrap_or(false)
                    })
                    .collect()
            }
        })
    };

    // Save default LLM
    let on_save_default = move || {
        if let Some(pid) = default_provider_id.get() {
            let service = app_state.get_value().llm_provider_service();
            spawn_local(async move {
                let _ = service.set_default_provider(pid).await;
            });
        }
    };

    view! {
        <div class="page llm-providers-page">
            // Breadcrumb
            <div class="breadcrumb">
                <span>"设置"</span>
                <span class="breadcrumb-separator">"/"</span>
                <span class="breadcrumb-current">"模型"</span>
            </div>

            <h1 class="page-title">"模型"</h1>

            // Default LLM Section
            <div class="default-llm-section">
                <h3>"默认LLM"</h3>
                <div class="default-llm-form">
                    <div class="form-row">
                        <div class="form-group">
                            <label>"提供商"</label>
                            <select
                                prop:value=move || default_provider_id.get().map(|id| id.to_string()).unwrap_or_default()
                                on:change=move |ev| {
                                    let val = event_target_value(&ev);
                                    if let Ok(id) = val.parse::<i64>() {
                                        default_provider_id.set(Some(id));
                                        // Update default model selection
                                        if let Some(list) = providers.get() {
                                            if let Some(p) = list.iter().find(|p| p.id == id) {
                                                if let Some(m) = p.models.iter().find(|m| m.is_default_model) {
                                                    default_model_name.set(Some(m.name.clone()));
                                                } else if let Some(m) = p.models.first() {
                                                    default_model_name.set(Some(m.name.clone()));
                                                } else {
                                                    default_model_name.set(None);
                                                }
                                            }
                                        }
                                    }
                                }
                            >
                                <option value="">"选择提供商 (必须已授权)"</option>
                                {move || providers.get().unwrap_or_default().into_iter().map(|p| {
                                    let selected = default_provider_id.get() == Some(p.id);
                                    view! {
                                        <option value={p.id.to_string()} selected={selected}>
                                            {p.name.clone()}
                                        </option>
                                    }
                                }).collect_view()}
                            </select>
                        </div>
                        <div class="form-group">
                            <label>"模型"</label>
                            <select
                                prop:value=move || default_model_name.get().unwrap_or_default()
                                on:change=move |ev| {
                                    let val = event_target_value(&ev);
                                    if !val.is_empty() {
                                        default_model_name.set(Some(val));
                                    }
                                }
                            >
                                <option value="">"请先添加模型"</option>
                                {move || {
                                    let pid = default_provider_id.get();
                                    let list = providers.get().unwrap_or_default();
                                    let models = pid.and_then(|id| {
                                        list.iter().find(|p| p.id == id).map(|p| p.models.clone())
                                    }).unwrap_or_default();
                                    models.into_iter().map(|m| {
                                        let selected = default_model_name.get().as_ref() == Some(&m.name);
                                        view! {
                                            <option value={m.name.clone()} selected={selected}>
                                                {m.display_name.clone().unwrap_or_else(|| m.name.clone())}
                                            </option>
                                        }
                                    }).collect_view()
                                }}
                            </select>
                        </div>
                        <button
                            class="btn btn-primary save-default-btn"
                            on:click=move |_| on_save_default()
                            disabled=move || default_provider_id.get().is_none()
                        >
                            "保存"
                        </button>
                    </div>
                    <p class="form-hint">
                        "在这里设置全局默认的 LLM 模型。你也可以在聊天页面为具体 Agent 单独选择使用的模型。"
                    </p>
                </div>
            </div>

            // Providers Section
            <div class="providers-section">
                <div class="section-header">
                    <h3>"提供商"</h3>
                    <div class="section-actions">
                        <div class="search-box">
                            <span class="search-icon">"🔍"</span>
                            <input
                                type="text"
                                placeholder="搜索提供商..."
                                prop:value=search_query.get()
                                on:input=move |ev| search_query.set(event_target_value(&ev))
                            />
                        </div>
                        <button
                            class="btn btn-icon"
                            on:click=move |_| refresh.get_value()()
                            title="刷新"
                        >
                            "🔄"
                        </button>
                        <button
                            class="btn btn-primary add-provider-btn"
                            on:click=move |_| show_add_modal.set(true)
                        >
                            <span>"+"</span>
                            "添加提供商"
                        </button>
                    </div>
                </div>

                {move || {
                    if is_loading.get() {
                        view! { <div class="loading-state">"加载中..."</div> }.into_any()
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
                    } else if let Some(list) = filtered_providers() {
                        if list.is_empty() {
                            if search_query.get().is_empty() {
                                view! {
                                    <div class="empty-state">
                                        <div class="empty-icon">"📭"</div>
                                        <p>"暂无提供商"</p>
                                        <button
                                            class="btn btn-primary"
                                            on:click=move |_| show_add_modal.set(true)
                                        >
                                            "添加提供商"
                                        </button>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="empty-state">
                                        <p>"未找到匹配的提供商"</p>
                                    </div>
                                }.into_any()
                            }
                        } else {
                            view! {
                                <div class="providers-grid">
                                    {list.into_iter().map(|provider| {
                                        let icon = provider.icon.clone().unwrap_or_else(|| "🔧".to_string());
                                        let color = provider.icon_color.clone().unwrap_or_else(|| "#64748b".to_string());
                                        let provider_for_config = provider.clone();
                                        let provider_for_model = provider.clone();
                                        let has_api_key = provider.api_key_masked.is_some();
                                        let model_count = provider.models.len();
                                        let default_model = provider.models.iter()
                                            .find(|m| m.is_default_model)
                                            .or_else(|| provider.models.first());

                                        view! {
                                            <div class="provider-card">
                                                <div class="provider-card-header">
                                                    <div
                                                        class="provider-avatar"
                                                        style=format!("background: {}; color: white;", color)
                                                    >
                                                        {icon}
                                                    </div>
                                                    <div class="provider-info">
                                                        <div class="provider-name-row">
                                                            <h4>{provider.name.clone()}</h4>
                                                            {provider.type_label.clone().map(|label| view! {
                                                                <span class="provider-tag">{label}</span>
                                                            })}
                                                            {if provider.is_default_provider {
                                                                view! { <span class="provider-tag default-tag">"默认"</span> }.into_any()
                                                            } else {
                                                                view! { <span></span> }.into_any()
                                                            }}
                                                        </div>
                                                        <div class="provider-status">
                                                            {if provider.enabled {
                                                                view! { <span class="status-dot online"></span> }.into_any()
                                                            } else {
                                                                view! { <span class="status-dot offline"></span> }.into_any()
                                                            }}
                                                            {if provider.enabled { "可用" } else { "不可用" }}
                                                        </div>
                                                    </div>
                                                </div>
                                                <div class="provider-card-body">
                                                    <div class="provider-detail">
                                                        <span class="detail-label">"类型"</span>
                                                        <span class="detail-value">
                                                            {if provider.provider_id == "ollama" {
                                                                "嵌入式 (进程内)"
                                                            } else {
                                                                "远程 API"
                                                            }}
                                                        </span>
                                                    </div>
                                                    <div class="provider-detail">
                                                        <span class="detail-label">"Base URL"</span>
                                                        <span class="detail-value url-value">
                                                            {provider.base_url.clone().unwrap_or_else(|| "未配置".to_string())}
                                                        </span>
                                                    </div>
                                                    <div class="provider-detail">
                                                        <span class="detail-label">"API Key"</span>
                                                        <span class="detail-value">
                                                            {if has_api_key {
                                                                view! { <span class="status-set">"已设置"</span> }.into_any()
                                                            } else {
                                                                view! { <span class="status-unset">"未设置"</span> }.into_any()
                                                            }}
                                                        </span>
                                                    </div>
                                                    <div class="provider-detail">
                                                        <span class="detail-label">"模型"</span>
                                                        <span class="detail-value">
                                                            {if model_count == 0 {
                                                                view! { <span class="status-unset">"暂无模型"</span> }.into_any()
                                                            } else {
                                                                view! {
                                                                    <span>
                                                                        {default_model.map(|m| {
                                                                            m.display_name.clone().unwrap_or_else(|| m.name.clone())
                                                                        }).unwrap_or_else(|| format!("{} 个模型", model_count))}
                                                                    </span>
                                                                }.into_any()
                                                            }}
                                                        </span>
                                                    </div>
                                                </div>
                                                <div class="provider-card-footer">
                                                    <button
                                                        class="btn btn-sm btn-secondary"
                                                        on:click=move |_| {
                                                            selected_provider.set(Some(provider_for_model.clone()));
                                                            show_model_modal.set(true);
                                                        }
                                                    >
                                                        "模型"
                                                    </button>
                                                    <button
                                                        class="btn btn-sm btn-secondary"
                                                        on:click=move |_| {
                                                            selected_provider.set(Some(provider_for_config.clone()));
                                                            show_config_modal.set(true);
                                                        }
                                                    >
                                                        "设置"
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
            </div>

            // Modals
            <Show when=move || show_config_modal.get()>
                {move || selected_provider.get().map(|p| view! {
                    <ProviderConfigModal
                        provider=p
                        on_close=move || show_config_modal.set(false)
                        on_updated=move || refresh.get_value()()
                    />
                })}
            </Show>

            <Show when=move || show_model_modal.get()>
                {move || selected_provider.get().map(|p| view! {
                    <ModelManageModal
                        provider=p
                        on_close=move || show_model_modal.set(false)
                        on_updated=move || refresh.get_value()()
                    />
                })}
            </Show>

            <Show when=move || show_add_modal.get()>
                <AddProviderModal
                    on_close=move || show_add_modal.set(false)
                    on_created=move || refresh.get_value()()
                />
            </Show>
        </div>
    }
}
