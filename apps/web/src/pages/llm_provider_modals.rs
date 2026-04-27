//! LLM Provider Management Modals
//!
//! Dark-theme modals: config, model management, add provider.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::llm_provider_service::{
    AddModelRequest, CreateProviderRequest, LlmProvider, UpdateProviderRequest,
};
use crate::components::modal::Modal;
use crate::state::use_app_state;

// ============================================
// Provider Configuration Modal
// ============================================
#[component]
pub fn ProviderConfigModal(
    provider: LlmProvider,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_updated: Callback<()>,
) -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());

    let base_url = RwSignal::new(provider.base_url.clone().unwrap_or_default());
    let api_key = RwSignal::new(String::new());
    let show_api_key = RwSignal::new(false);
    let enabled = RwSignal::new(provider.enabled);
    let advanced_open = RwSignal::new(false);
    let generation_params = RwSignal::new(String::from("{\n  \"max_tokens\": null\n}"));
    let saving = RwSignal::new(false);
    let testing = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);
    let test_result: RwSignal<Option<String>> = RwSignal::new(None);

    let on_save = move |_| {
        saving.set(true);
        error_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        let id = provider.id;
        let req = UpdateProviderRequest {
            name: None,
            base_url: Some(base_url.get()).filter(|s| !s.is_empty()),
            api_key: Some(api_key.get()).filter(|s| !s.is_empty()),
            enabled: Some(enabled.get()),
        };
        spawn_local(async move {
            match service.update_provider(id, req).await {
                Ok(_) => {
                    saving.set(false);
                    on_updated.run(());
                    on_close.run(());
                }
                Err(e) => {
                    saving.set(false);
                    error_msg.set(Some(format!("保存失败: {}", e)));
                }
            }
        });
    };

    let on_test = move |_| {
        testing.set(true);
        test_result.set(None);
        spawn_local(async move {
            // TODO: Implement actual connection test API
            gloo_timers::future::TimeoutFuture::new(1_000).await;
            testing.set(false);
            test_result.set(Some("连接成功 (模拟)".to_string()));
        });
    };

    let input_type = move || if show_api_key.get() { "text" } else { "password" };

    view! {
        <Modal title=format!("配置 {}", provider.name) on_close=move |_| on_close.run(())>
            <div class="modal-body llm-config-modal-body">
                {move || error_msg.get().map(|e| view! {
                    <div class="error-message">{e}</div>
                })}
                {move || test_result.get().map(|msg| view! {
                    <div class="success-message">{msg}</div>
                })}

                <div class="form-group required">
                    <label>"基础 URL"</label>
                    <input
                        type="text"
                        placeholder="Ollama 端点，例如 http://localhost:11434"
                        prop:value=base_url.get()
                        on:input=move |ev| base_url.set(event_target_value(&ev))
                    />
                </div>

                <div class="form-group">
                    <label>"API 密钥"</label>
                    <div class="input-with-suffix">
                        <input
                            type={input_type}
                            placeholder="输入 API 密钥（可选）"
                            prop:value=api_key.get()
                            on:input=move |ev| api_key.set(event_target_value(&ev))
                        />
                        <button
                            class="input-suffix-btn"
                            on:click=move |_| show_api_key.update(|v| *v = !*v)
                            title=move || if show_api_key.get() { "隐藏" } else { "显示" }
                        >
                            {move || if show_api_key.get() { "🙈" } else { "👁️" }}
                        </button>
                    </div>
                </div>

                // Advanced Config
                <div class="advanced-section">
                    <button
                        class="advanced-toggle"
                        on:click=move |_| advanced_open.update(|v| *v = !*v)
                    >
                        <span class="toggle-icon">
                            {move || if advanced_open.get() { "▼" } else { "▶" }}
                        </span>
                        "进阶配置"
                    </button>
                    <Show when=move || advanced_open.get()>
                        <div class="advanced-content">
                            <div class="form-group">
                                <label>"生成参数配置"</label>
                                <textarea
                                    class="code-textarea"
                                    rows="6"
                                    prop:value=generation_params.get()
                                    on:input=move |ev| generation_params.set(event_target_value(&ev))
                                />
                                <p class="field-hint">
                                    "使用 JSON 格式表示的生成参数配置项，会被展开传入到生成请求（"
                                    <code>"openai.chat.completions"</code>
                                    " 或 "
                                    <code>"anthropic.messages"</code>
                                    "）中。"
                                </p>
                            </div>
                        </div>
                    </Show>
                </div>
            </div>
            <div class="modal-footer llm-config-footer">
                <button
                    class="btn btn-test"
                    on:click=on_test
                    disabled=move || testing.get()
                >
                    <span>"🔗"</span>
                    {if testing.get() { "测试中..." } else { "测试连接" }}
                </button>
                <div class="footer-spacer" />
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "取消"
                </button>
                <button
                    class="btn btn-primary"
                    on:click=on_save
                    disabled=move || saving.get() || base_url.get().trim().is_empty()
                >
                    {if saving.get() { "保存中..." } else { "保存" }}
                </button>
            </div>
        </Modal>
    }
}

// ============================================
// Model Management Modal
// ============================================
#[component]
pub fn ModelManageModal(
    provider: LlmProvider,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_updated: Callback<()>,
) -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());

    let search_query = RwSignal::new(String::new());
    let new_model_name = RwSignal::new(String::new());
    let adding = RwSignal::new(false);
    let discovering = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);

    let add_model_action = move || {
        let name = new_model_name.get();
        if name.trim().is_empty() {
            error_msg.set(Some("模型名称不能为空".to_string()));
            return;
        }
        adding.set(true);
        error_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        let provider_id = provider.id;
        let req = AddModelRequest {
            name: name.trim().to_string(),
            display_name: None,
        };
        spawn_local(async move {
            match service.add_model(provider_id, req).await {
                Ok(_) => {
                    adding.set(false);
                    new_model_name.set(String::new());
                    on_updated.run(());
                }
                Err(e) => {
                    adding.set(false);
                    error_msg.set(Some(format!("添加失败: {}", e)));
                }
            }
        });
    };

    let on_discover_models = move |_| {
        discovering.set(true);
        error_msg.set(None);
        spawn_local(async move {
            // TODO: Implement actual model discovery API
            gloo_timers::future::TimeoutFuture::new(1_500).await;
            discovering.set(false);
            error_msg.set(Some("自动发现功能暂未实现".to_string()));
        });
    };

    let on_delete_model = move |model_id: i64| {
        let service = app_state.get_value().llm_provider_service();
        let provider_id = provider.id;
        spawn_local(async move {
            let _ = service.delete_model(provider_id, model_id).await;
            on_updated.run(());
        });
    };

    let on_set_default_model = move |model_id: i64| {
        let service = app_state.get_value().llm_provider_service();
        let provider_id = provider.id;
        spawn_local(async move {
            let _ = service.set_default_model(provider_id, model_id).await;
            on_updated.run(());
        });
    };

    // Filter models by search
    let filtered_models = move || {
        let query = search_query.get().to_lowercase();
        let models = provider.models.clone();
        if query.is_empty() {
            models
        } else {
            models
                .into_iter()
                .filter(|m| {
                    m.name.to_lowercase().contains(&query)
                        || m.display_name
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&query))
                            .unwrap_or(false)
                })
                .collect()
        }
    };

    view! {
        <Modal title=format!("{} — 模型管理", provider.name) on_close=move |_| on_close.run(())>
            <div class="modal-body model-manage-body">
                {move || error_msg.get().map(|e| view! {
                    <div class="error-message">{e}</div>
                })}

                // Search
                <div class="search-box model-search">
                    <span class="search-icon">"🔍"</span>
                    <input
                        type="text"
                        placeholder="搜索模型..."
                        prop:value=search_query.get()
                        on:input=move |ev| search_query.set(event_target_value(&ev))
                    />
                </div>

                // Model list
                <div class="model-list-container">
                    {move || {
                        let models = filtered_models();
                        if models.is_empty() {
                            view! {
                                <div class="empty-state compact">
                                    <p>"暂无模型"</p>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="model-list">
                                    {models.into_iter().map(|model| {
                                        let model_id = model.id;
                                        let model_id_for_delete = model.id;
                                        view! {
                                            <div class="model-item" class:default=model.is_default_model>
                                                <div class="model-info">
                                                    <span class="model-name">{model.name.clone()}</span>
                                                    {model.display_name.clone().map(|d| view! {
                                                        <span class="model-display">{"("}{d}{")"}</span>
                                                    })}
                                                    {if model.is_default_model {
                                                        view! { <span class="badge default">"默认"</span> }.into_any()
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }}
                                                </div>
                                                <div class="model-actions">
                                                    {if !model.is_default_model {
                                                        view! {
                                                            <button
                                                                class="btn btn-sm btn-text"
                                                                on:click=move |_| on_set_default_model(model_id)
                                                            >
                                                                "设为默认"
                                                            </button>
                                                        }.into_any()
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }}
                                                    <button
                                                        class="btn btn-sm btn-text danger"
                                                        on:click=move |_| on_delete_model(model_id_for_delete)
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
                    }}
                </div>

                // Add model inline
                <div class="add-model-inline">
                    <input
                        type="text"
                        placeholder="输入模型名称（如 llama2）"
                        prop:value=new_model_name.get()
                        on:input=move |ev| new_model_name.set(event_target_value(&ev))
                        on:keyup=move |ev| {
                            if ev.key() == "Enter" {
                                add_model_action();
                            }
                        }
                    />
                    <button
                        class="btn btn-primary"
                        on:click=move |_: leptos::ev::MouseEvent| add_model_action()
                        prop:disabled=move || adding.get() || new_model_name.get().trim().is_empty()
                    >
                        {if adding.get() { "添加中..." } else { "添加" }}
                    </button>
                </div>
            </div>
            <div class="modal-footer model-manage-footer">
                <button
                    class="btn btn-secondary"
                    on:click=on_discover_models
                    disabled=move || discovering.get()
                >
                    <span>"🔍"</span>
                    {if discovering.get() { "发现中..." } else { "自动发现模型" }}
                </button>
                <div class="footer-spacer" />
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "关闭"
                </button>
            </div>
        </Modal>
    }
}

// ============================================
// Add Custom Provider Modal
// ============================================
#[component]
pub fn AddProviderModal(
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_created: Callback<()>,
) -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());

    let provider_id = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let protocol = RwSignal::new("openai-compatible".to_string());
    let base_url = RwSignal::new(String::new());
    let creating = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);

    let on_create = move |_| {
        let pid = provider_id.get().trim().to_string();
        let pname = name.get().trim().to_string();

        if pid.is_empty() {
            error_msg.set(Some("Provider ID 不能为空".to_string()));
            return;
        }
        if !pid.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
            error_msg.set(Some("Provider ID 只能包含小写字母、数字、连字符和下划线".to_string()));
            return;
        }
        if pname.is_empty() {
            error_msg.set(Some("显示名称不能为空".to_string()));
            return;
        }

        creating.set(true);
        error_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        let req = CreateProviderRequest {
            provider_id: pid,
            name: pname,
            protocol: protocol.get(),
            base_url: Some(base_url.get()).filter(|s| !s.is_empty()),
            api_key: None,
        };
        spawn_local(async move {
            match service.create_provider(req).await {
                Ok(_) => {
                    creating.set(false);
                    on_created.run(());
                    on_close.run(());
                }
                Err(e) => {
                    creating.set(false);
                    error_msg.set(Some(format!("创建失败: {}", e)));
                }
            }
        });
    };

    view! {
        <Modal title="添加自定义提供商" on_close=move |_| on_close.run(())>
            <div class="modal-body add-provider-body">
                {move || error_msg.get().map(|e| view! {
                    <div class="error-message">{e}</div>
                })}

                <div class="form-group required">
                    <label>"提供商 ID"</label>
                    <input
                        type="text"
                        placeholder="例如 openai, google, anthropic"
                        prop:value=provider_id.get()
                        on:input=move |ev| provider_id.set(event_target_value(&ev))
                    />
                    <p class="field-hint">
                        "小写字母、数字、连字符、下划线，创建后不可更改。"
                    </p>
                </div>

                <div class="form-group required">
                    <label>"显示名称"</label>
                    <input
                        type="text"
                        placeholder="例如 OpenAI, Google Gemini"
                        prop:value=name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </div>

                <div class="form-group">
                    <label>"默认 Base URL"</label>
                    <input
                        type="text"
                        placeholder="例如 https://api.example.com"
                        prop:value=base_url.get()
                        on:input=move |ev| base_url.set(event_target_value(&ev))
                    />
                </div>

                <div class="form-group required">
                    <label>"协议"</label>
                    <select
                        prop:value=protocol.get()
                        on:change=move |ev| protocol.set(event_target_value(&ev))
                    >
                        <option value="openai-compatible">"OpenAI 兼容（Chat Completions）"</option>
                        <option value="anthropic">"Anthropic（Messages API）"</option>
                    </select>
                </div>
            </div>
            <div class="modal-footer">
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "取消"
                </button>
                <button
                    class="btn btn-primary"
                    on:click=on_create
                    disabled=move || creating.get()
                >
                    {if creating.get() { "创建中..." } else { "创建" }}
                </button>
            </div>
        </Modal>
    }
}
