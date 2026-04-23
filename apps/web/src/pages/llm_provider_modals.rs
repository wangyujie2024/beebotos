//! LLM Provider Management Modals

use crate::api::llm_provider_service::{
    AddModelRequest, CreateProviderRequest, LlmProvider, UpdateProviderRequest,
};
use crate::components::modal::Modal;
use crate::state::use_app_state;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// Provider Configuration Modal
#[component]
pub fn ProviderConfigModal(
    provider: LlmProvider,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_updated: Callback<()>,
) -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());
    let app_state_for_save = app_state;

    let name = RwSignal::new(provider.name.clone());
    let base_url = RwSignal::new(provider.base_url.clone().unwrap_or_default());
    let api_key = RwSignal::new(String::new());
    let enabled = RwSignal::new(provider.enabled);
    let saving = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);

    let on_save = move |_| {
        saving.set(true);
        error_msg.set(None);
        let service = app_state_for_save.get_value().llm_provider_service();
        let id = provider.id;
        let req = UpdateProviderRequest {
            name: Some(name.get()),
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

    view! {
        <Modal title=format!("配置 - {}", provider.name) on_close=move |_| on_close.run(())>
            <div class="modal-body">
                {move || error_msg.get().map(|e| view! {
                    <div class="error-message">{e}</div>
                })}
                <div class="form-group">
                    <label>"显示名称"</label>
                    <input type="text" prop:value=name.get() on:input=move |ev| name.set(event_target_value(&ev)) />
                </div>
                <div class="form-group">
                    <label>"Base URL"</label>
                    <input type="text" prop:value=base_url.get() on:input=move |ev| base_url.set(event_target_value(&ev)) />
                </div>
                <div class="form-group">
                    <label>"API Key (留空保持不变)"</label>
                    <input type="password" prop:value=api_key.get() on:input=move |ev| api_key.set(event_target_value(&ev)) />
                </div>
                <div class="form-group checkbox">
                    <label>
                        <input type="checkbox" checked=enabled.get() on:change=move |ev| enabled.set(event_target_checked(&ev)) />
                        "启用"
                    </label>
                </div>
            </div>
            <div class="modal-footer">
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "取消"
                </button>
                <button class="btn btn-primary" on:click=on_save disabled=move || saving.get()>
                    {if saving.get() { "保存中..." } else { "保存" }}
                </button>
            </div>
        </Modal>
    }
}

/// Model Management Modal
#[component]
pub fn ModelManageModal(
    provider: LlmProvider,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_updated: Callback<()>,
) -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());

    let new_model_name = RwSignal::new(String::new());
    let new_model_display = RwSignal::new(String::new());
    let adding = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);

    let on_add_model = {
        let app_state = app_state.clone();
        move |_| {
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
                display_name: Some(new_model_display.get()).filter(|s| !s.is_empty()),
            };
            spawn_local(async move {
                match service.add_model(provider_id, req).await {
                    Ok(_) => {
                        adding.set(false);
                        new_model_name.set(String::new());
                        new_model_display.set(String::new());
                        on_updated.run(());
                    }
                    Err(e) => {
                        adding.set(false);
                        error_msg.set(Some(format!("添加失败: {}", e)));
                    }
                }
            });
        }
    };

    let on_delete_model = {
        let app_state = app_state.clone();
        move |model_id: i64| {
            let service = app_state.get_value().llm_provider_service();
            let provider_id = provider.id;
            spawn_local(async move {
                let _ = service.delete_model(provider_id, model_id).await;
                on_updated.run(());
            });
        }
    };

    let on_set_default_model = {
        let app_state = app_state.clone();
        move |model_id: i64| {
            let service = app_state.get_value().llm_provider_service();
            let provider_id = provider.id;
            spawn_local(async move {
                let _ = service.set_default_model(provider_id, model_id).await;
                on_updated.run(());
            });
        }
    };

    view! {
        <Modal title=format!("模型管理 - {}", provider.name) on_close=move |_| on_close.run(())>
            <div class="modal-body">
                {move || error_msg.get().map(|e| view! {
                    <div class="error-message">{e}</div>
                })}

                <div class="model-list">
                    {provider.models.iter().map(|model| {
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
                                                class="btn btn-sm btn-secondary"
                                                on:click=move |_| on_set_default_model(model_id)
                                            >
                                                "设为默认"
                                            </button>
                                        }.into_any()
                                    } else {
                                        view! { <span></span> }.into_any()
                                    }}
                                    <button
                                        class="btn btn-sm btn-danger"
                                        on:click=move |_| on_delete_model(model_id_for_delete)
                                    >
                                        "删除"
                                    </button>
                                </div>
                            </div>
                        }
                    }).collect_view()}
                </div>

                <div class="add-model-section">
                    <h4>"添加模型"</h4>
                    <div class="form-row">
                        <input
                            type="text"
                            placeholder="模型名称 (如 gpt-4o)"
                            prop:value=new_model_name.get()
                            on:input=move |ev| new_model_name.set(event_target_value(&ev))
                        />
                        <input
                            type="text"
                            placeholder="显示名称 (可选)"
                            prop:value=new_model_display.get()
                            on:input=move |ev| new_model_display.set(event_target_value(&ev))
                        />
                        <button
                            class="btn btn-primary"
                            on:click=on_add_model
                            disabled=move || adding.get()
                        >
                            {if adding.get() { "添加中..." } else { "添加" }}
                        </button>
                    </div>
                </div>
            </div>
            <div class="modal-footer">
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "关闭"
                </button>
            </div>
        </Modal>
    }
}

/// Add Custom Provider Modal
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
    let api_key = RwSignal::new(String::new());
    let creating = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);

    let on_create = move |_| {
        let pid = provider_id.get().trim().to_string();
        let pname = name.get().trim().to_string();

        if pid.is_empty() {
            error_msg.set(Some("Provider ID 不能为空".to_string()));
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
            api_key: Some(api_key.get()).filter(|s| !s.is_empty()),
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
            <div class="modal-body">
                {move || error_msg.get().map(|e| view! {
                    <div class="error-message">{e}</div>
                })}
                <div class="form-group">
                    <label>"Provider ID (唯一标识)"</label>
                    <input
                        type="text"
                        placeholder="如 my-custom-provider"
                        prop:value=provider_id.get()
                        on:input=move |ev| provider_id.set(event_target_value(&ev))
                    />
                </div>
                <div class="form-group">
                    <label>"显示名称"</label>
                    <input
                        type="text"
                        placeholder="如 My Custom Provider"
                        prop:value=name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </div>
                <div class="form-group">
                    <label>"协议"</label>
                    <select
                        prop:value=protocol.get()
                        on:change=move |ev| protocol.set(event_target_value(&ev))
                    >
                        <option value="openai-compatible">"OpenAI Compatible"</option>
                        <option value="anthropic">"Anthropic"</option>
                    </select>
                </div>
                <div class="form-group">
                    <label>"Base URL"</label>
                    <input
                        type="text"
                        placeholder="如 https://api.example.com/v1"
                        prop:value=base_url.get()
                        on:input=move |ev| base_url.set(event_target_value(&ev))
                    />
                </div>
                <div class="form-group">
                    <label>"API Key"</label>
                    <input
                        type="password"
                        placeholder="API Key"
                        prop:value=api_key.get()
                        on:input=move |ev| api_key.set(event_target_value(&ev))
                    />
                </div>
            </div>
            <div class="modal-footer">
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "取消"
                </button>
                <button class="btn btn-primary" on:click=on_create disabled=move || creating.get()>
                    {if creating.get() { "创建中..." } else { "创建" }}
                </button>
            </div>
        </Modal>
    }
}
