//! 浏览器自动化页面
//!
//! 提供 Chrome DevTools 控制、批处理操作、沙箱管理等功能

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::Title;

use crate::browser::ConnectionStatus;
use crate::components::Modal;
use crate::state::{
    use_browser_state, use_browser_ui_state,
};

/// 浏览器自动化页面
#[component]
pub fn BrowserPage() -> impl IntoView {
    let browser_state = use_browser_state();
    let _ui_state = use_browser_ui_state();

    // Load profiles from API on mount
    let client = crate::api::create_client();
    let browser_service = crate::api::BrowserApiService::new(client);
    let service_stored = leptos::prelude::StoredValue::new(browser_service);

    leptos::task::spawn_local(async move {
        let service = service_stored.get_value();
        match service.list_profiles().await {
            Ok(profiles) => browser_state.profiles.set(profiles),
            Err(e) => browser_state.error.set(Some(crate::browser::BrowserError {
                error_type: crate::browser::BrowserErrorType::ConnectionLost,
                message: e.to_string(),
                current_url: None,
                screenshot_path: None,
                suggestions: vec![],
            })),
        }
        match service.get_status().await {
            Ok(status) => {
                browser_state.connection_status.set(
                    if status.active_instances > 0 { crate::browser::ConnectionStatus::Connected } else { crate::browser::ConnectionStatus::Disconnected }
                );
            }
            Err(_) => {}
        }
    });

    view! {
        <Title text="Browser Automation - BeeBotOS" />
        <div class="browser-page">
            <BrowserHeader />
            <div class="browser-container">
                <BrowserSidebar />
                <BrowserMainContent />
            </div>
        </div>
    }
}

/// 页面头部
#[component]
fn BrowserHeader() -> impl IntoView {
    view! {
        <header class="browser-header">
            <h1>"Browser Automation"</h1>
            <p class="browser-subtitle">
                "Chrome DevTools MCP Control - Compatible with OpenClaw V2026.3.13"
            </p>
        </header>
    }
}

/// 侧边栏
#[component]
fn BrowserSidebar() -> impl IntoView {
    let ui_state = use_browser_ui_state();
    let ui_state_sv = StoredValue::new(ui_state.clone());

    view! {
        <aside class="browser-sidebar">
            <div class="sidebar-section">
                <h3>"Profiles"</h3>
                <button
                    class="btn btn-primary btn-sm"
                    on:click=move |_| ui_state_sv.get_value().open_add_profile_modal()
                >
                    "+ Add Profile"
                </button>
                <ProfileList />
            </div>

            <div class="sidebar-section">
                <h3>"Sandboxes"</h3>
                <button
                    class="btn btn-secondary btn-sm"
                    on:click=move |_| ui_state_sv.get_value().open_create_sandbox_modal()
                >
                    "+ Create Sandbox"
                </button>
                <SandboxList />
            </div>
        </aside>
    }
}

/// 配置列表
#[component]
fn ProfileList() -> impl IntoView {
    let state = use_browser_state();

    view! {
        <div class="profile-list">
            <For
                each=move || state.profiles.get()
                key=|profile| profile.id.clone()
                children=move |profile| {
                    let profile_stored = StoredValue::new(profile.clone());
                    let profile_id_del = profile.id.clone();
                    view! {
                        <div
                            class=move || {
                                let state = use_browser_state();
                                let p = profile_stored.get_value();
                                if state.selected_profile_id.get() == Some(p.id.clone()) {
                                    "profile-item selected"
                                } else {
                                    "profile-item"
                                }
                            }
                            style=move || format!("border-left-color: {}", profile_stored.get_value().color)
                            on:click=move |_| {
                                let p = profile_stored.get_value();
                                let profile_id = p.id.clone();
                                let profile_name = p.name.clone();
                                let state = use_browser_state();
                                let client = crate::api::create_client();
                                let service = crate::api::BrowserApiService::new(client);
                                state.select_profile(profile_id.clone());
                                state.connection_status.set(ConnectionStatus::Connecting);
                                spawn_local(async move {
                                    match service.connect(&profile_id).await {
                                        Ok(instance) => {
                                            let state = use_browser_state();
                                            state.current_instance.set(Some(instance.clone()));
                                            state.connection_status.set(ConnectionStatus::Connected);
                                            state.current_url.set(instance.current_url.clone().unwrap_or_else(|| "about:blank".to_string()));
                                            state.logs.update(|logs| logs.push(format!("Connected to profile: {}", profile_name)));
                                        }
                                        Err(e) => {
                                            let state = use_browser_state();
                                            state.connection_status.set(ConnectionStatus::Error(e.to_string()));
                                            state.logs.update(|logs| logs.push(format!("Connection failed: {}", e)));
                                        }
                                    }
                                });
                            }
                        >
                            <div class="profile-main">
                                <div class="profile-name">{move || profile_stored.get_value().name.clone()}</div>
                                <div class="profile-info">
                                    {move || format!("Port: {}", profile_stored.get_value().cdp_port)}
                                </div>
                            </div>
                            <button
                                class="btn btn-icon btn-danger btn-xs"
                                title="Delete profile"
                                on:click=move |e| {
                                    e.stop_propagation();
                                    let id = profile_id_del.clone();
                                    let state = use_browser_state();
                                    spawn_local(async move {
                                        let client = crate::api::create_client();
                                        let service = crate::api::BrowserApiService::new(client);
                                        match service.delete_profile(&id).await {
                                            Ok(_) => {
                                                state.logs.update(|logs| logs.push(format!("Deleted profile: {}", id)));
                                                state.profiles.update(|p| p.retain(|pr| pr.id != id));
                                            }
                                            Err(e) => {
                                                state.logs.update(|logs| logs.push(format!("Delete failed: {}", e)));
                                            }
                                        }
                                    });
                                }
                            >
                                "🗑"
                            </button>
                        </div>
                    }
                }
            />
        </div>
    }
}

/// 沙箱列表
#[component]
fn SandboxList() -> impl IntoView {
    let state = use_browser_state();

    view! {
        <div class="sandbox-list">
            <For
                each=move || state.sandboxes.get()
                key=|sandbox| sandbox.id.clone()
                children=move |sandbox| {
                    let sandbox_del = sandbox.id.clone();
                    view! {
                        <div
                            class="sandbox-item"
                            style=format!("border-left-color: {}", sandbox.color)
                        >
                            <div class="sandbox-main">
                                <div class="sandbox-name">{sandbox.name.clone()}</div>
                                <div class="sandbox-info">
                                    {format!("Port: {}", sandbox.cdp_port)}
                                </div>
                            </div>
                            <button
                                class="btn btn-icon btn-danger btn-xs"
                                title="Delete sandbox"
                                on:click=move |e| {
                                    e.stop_propagation();
                                    let id = sandbox_del.clone();
                                    let state = use_browser_state();
                                    spawn_local(async move {
                                        let client = crate::api::create_client();
                                        let service = crate::api::BrowserApiService::new(client);
                                        match service.delete_sandbox(&id).await {
                                            Ok(_) => {
                                                state.logs.update(|logs| logs.push(format!("Deleted sandbox: {}", id)));
                                                state.sandboxes.update(|s| s.retain(|sb| sb.id != id));
                                            }
                                            Err(e) => {
                                                state.logs.update(|logs| logs.push(format!("Delete failed: {}", e)));
                                            }
                                        }
                                    });
                                }
                            >
                                "🗑"
                            </button>
                        </div>
                    }
                }
            />
        </div>
    }
}

/// 主内容区
#[component]
fn BrowserMainContent() -> impl IntoView {
    let ui_state = use_browser_ui_state();

    view! {
        <main class="browser-main">
            <BrowserToolbar />
            <BrowserViewport />
            {move || if ui_state.show_debug_panel.get() {
                view! { <BrowserDebugPanel /> }.into_any()
            } else {
                view! { <div class="debug-collapsed">"Debug panel hidden"</div> }.into_any()
            }}
            <AddProfileModal />
            <CreateSandboxModal />
        </main>
    }
}

/// 工具栏
#[component]
fn BrowserToolbar() -> impl IntoView {
    let state = use_browser_state();
    let ui_state = use_browser_ui_state();
    let ui_state_sv = StoredValue::new(ui_state.clone());
    let url_input = RwSignal::new(String::new());

    view! {
        <div class="browser-toolbar">
            <div class="toolbar-group">
                <button
                    class="btn btn-icon"
                    title="Toggle Profiles"
                    on:click=move |_| ui_state_sv.get_value().toggle_profiles_panel()
                >
                    "📑"
                </button>
                <button
                    class="btn btn-icon"
                    title="Toggle Sandboxes"
                    on:click=move |_| ui_state_sv.get_value().toggle_sandboxes_panel()
                >
                    "🔲"
                </button>
            </div>

            <div class="toolbar-group toolbar-url">
                <input
                    type="text"
                    class="url-input"
                    prop:value=move || state.current_url.get()
                    on:input=move |e| url_input.set(crate::utils::event_target_value(&e))
                    placeholder="Enter URL..."
                />
                <button
                    class="btn btn-primary"
                    disabled=move || state.current_instance.get().is_none()
                    on:click=move |_| {
                        if let Some(ref instance) = state.current_instance.get() {
                            let url = url_input.get();
                            if url.is_empty() {
                                return;
                            }
                            let instance_id = instance.id.clone();
                            spawn_local(async move {
                                let client = crate::api::create_client();
                                let service = crate::api::BrowserApiService::new(client);
                                match service.navigate(&instance_id, &url).await {
                                    Ok(resp) => {
                                        let state = use_browser_state();
                                        state.current_url.set(url);
                                        state.logs.update(|logs| logs.push(format!("Navigated to: {}", resp.url)));
                                    }
                                    Err(e) => {
                                        let state = use_browser_state();
                                        state.logs.update(|logs| logs.push(format!("Navigation failed: {}", e)));
                                    }
                                }
                            });
                        }
                    }
                >
                    "Go"
                </button>
            </div>

            <div class="toolbar-group">
                <button
                    class=move || format!("btn btn-icon {}", if ui_state.show_debug_panel.get() { "active" } else { "" })
                    title="Toggle Debug Panel"
                    on:click=move |_| ui_state_sv.get_value().toggle_debug_panel()
                >
                    "🐛"
                </button>
                <button
                    class="btn btn-icon"
                    title="Take Screenshot"
                    on:click=move |_| {
                        if let Some(ref instance) = state.current_instance.get() {
                            let instance_id = instance.id.clone();
                            spawn_local(async move {
                                let client = crate::api::create_client();
                                let service = crate::api::BrowserApiService::new(client);
                                match service.capture_screenshot(&instance_id, false).await {
                                    Ok(result) => {
                                        let state = use_browser_state();
                                        state.logs.update(|logs| logs.push(format!("Screenshot captured ({}x{})", result.width, result.height)));
                                    }
                                    Err(e) => {
                                        let state = use_browser_state();
                                        state.logs.update(|logs| logs.push(format!("Screenshot failed: {}", e)));
                                    }
                                }
                            });
                        }
                    }
                >
                    "📷"
                </button>
            </div>
        </div>
    }
}

/// 视口区域
#[component]
fn BrowserViewport() -> impl IntoView {
    view! {
        <div class="browser-viewport">
            {move || {
                let state = use_browser_state();
                match state.connection_status.get() {
                    ConnectionStatus::Connected => {
                        let url = state.current_url.get();
                        view! {
                            <div class="browser-connected">
                                <div class="browser-address-bar">{url.clone()}</div>
                                <div class="browser-content">
                                    <iframe
                                        class="browser-iframe"
                                        src=url
                                        title="Browser Preview"
                                    />
                                </div>
                            </div>
                        }.into_any()
                    }
                    ConnectionStatus::Connecting => view! {
                        <div class="browser-placeholder">
                            <p>"Connecting..."</p>
                            <div class="spinner"></div>
                        </div>
                    }.into_any(),
                    ConnectionStatus::Error(ref msg) => view! {
                        <div class="browser-placeholder">
                            <p>"Connection failed"</p>
                            <p class="text-error">{msg.clone()}</p>
                        </div>
                    }.into_any(),
                    ConnectionStatus::Disconnected => view! {
                        <div class="browser-placeholder">
                            <p>"No browser connected"</p>
                            <p>"Select a profile to connect"</p>
                        </div>
                    }.into_any(),
                }
            }}
        </div>
    }
}

/// 调试面板
#[component]
fn BrowserDebugPanel() -> impl IntoView {
    view! {
        <div class="browser-debug-panel">
            <div class="debug-header">
                <h4>"Debug Console"</h4>
                <button
                    class="btn btn-sm"
                    on:click=move |_| {
                        let state = use_browser_state();
                        state.logs.set(Vec::new());
                    }
                >
                    "Clear"
                </button>
            </div>
            <div class="debug-logs">
                {move || {
                    let state = use_browser_state();
                    let logs = state.logs.get();
                    if logs.is_empty() {
                        view! { <p class="text-muted">"Debug logs will appear here..."</p> }.into_any()
                    } else {
                        view! {
                            <div class="log-entries">
                                {logs.into_iter().map(|log| view! {
                                    <div class="log-entry">{log}</div>
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

/// Add Profile Modal
#[component]
fn AddProfileModal() -> impl IntoView {
    let ui_state = use_browser_ui_state();
    let ui_state_sv = StoredValue::new(ui_state.clone());
    let state = use_browser_state();
    let state_sv = StoredValue::new(state.clone());
    let name = RwSignal::new(String::new());
    let port = RwSignal::new(9222u16);

    view! {
        {move || if ui_state.show_add_profile_modal.get() {
            let ui_state_sv = ui_state_sv.clone();
            let state_sv = state_sv.clone();
            view! {
                <Modal
                    title="Add Browser Profile"
                    on_close=Callback::from(move || ui_state_sv.get_value().close_add_profile_modal())
                >
                    <div class="form-group">
                        <label>"Profile Name"</label>
                        <input
                            type="text"
                            prop:value=name
                            on:input=move |e| name.set(crate::utils::event_target_value(&e))
                            placeholder="e.g. Work Profile"
                        />
                    </div>
                    <div class="form-group">
                        <label>"CDP Port"</label>
                        <input
                            type="number"
                            prop:value=move || port.get().to_string()
                            on:input=move |e| {
                                if let Ok(p) = crate::utils::event_target_value(&e).parse::<u16>() {
                                    port.set(p);
                                }
                            }
                            placeholder="9222"
                        />
                    </div>
                    <div class="modal-actions">
                        <button
                            class="btn btn-primary"
                            disabled=move || name.get().is_empty()
                            on:click=move |_| {
                                let profile = crate::browser::BrowserProfile::new(name.get(), port.get());
                                let state = state_sv.get_value();
                                let ui_state = ui_state_sv.get_value();
                                spawn_local(async move {
                                    let client = crate::api::create_client();
                                    let service = crate::api::BrowserApiService::new(client);
                                    match service.create_profile(profile).await {
                                        Ok(created) => {
                                            state.profiles.update(|p| p.push(created));
                                            state.logs.update(|logs| logs.push("Profile created".to_string()));
                                            ui_state.close_add_profile_modal();
                                        }
                                        Err(e) => {
                                            state.logs.update(|logs| logs.push(format!("Create failed: {}", e)));
                                        }
                                    }
                                });
                            }
                        >
                            "Create"
                        </button>
                        <button
                            class="btn btn-secondary"
                            on:click=move |_| ui_state_sv.get_value().close_add_profile_modal()
                        >
                            "Cancel"
                        </button>
                    </div>
                </Modal>
            }.into_any()
        } else {
            view! { <div></div> }.into_any()
        }}
    }
}

/// Create Sandbox Modal
#[component]
fn CreateSandboxModal() -> impl IntoView {
    let ui_state = use_browser_ui_state();
    let ui_state_sv = StoredValue::new(ui_state.clone());
    let state = use_browser_state();
    let state_sv = StoredValue::new(state.clone());
    let name = RwSignal::new(String::new());

    view! {
        {move || if ui_state.show_create_sandbox_modal.get() {
            let profiles = state.profiles.get();
            let selected_profile = RwSignal::new(profiles.first().map(|p| p.id.clone()).unwrap_or_default());
            let ui_state_sv = ui_state_sv.clone();
            let state_sv = state_sv.clone();

            view! {
                <Modal
                    title="Create Sandbox"
                    on_close=Callback::from(move || ui_state_sv.get_value().close_create_sandbox_modal())
                >
                    <div class="form-group">
                        <label>"Sandbox Name"</label>
                        <input
                            type="text"
                            prop:value=name
                            on:input=move |e| name.set(crate::utils::event_target_value(&e))
                            placeholder="e.g. Test Sandbox"
                        />
                    </div>
                    <div class="form-group">
                        <label>"Base Profile"</label>
                        <select
                            prop:value=move || selected_profile.get()
                            on:change=move |e| selected_profile.set(crate::utils::event_target_value(&e))
                        >
                            {profiles.into_iter().map(|p| view! {
                                <option value=p.id.clone()>{p.name}</option>
                            }).collect::<Vec<_>>()}
                        </select>
                    </div>
                    <div class="modal-actions">
                        <button
                            class="btn btn-primary"
                            disabled=move || name.get().is_empty() || selected_profile.get().is_empty()
                            on:click=move |_| {
                                let name_val = name.get();
                                let profile_id = selected_profile.get();
                                let state = state_sv.get_value();
                                let ui_state = ui_state_sv.get_value();
                                spawn_local(async move {
                                    let client = crate::api::create_client();
                                    let service = crate::api::BrowserApiService::new(client);
                                    match service.create_sandbox(&name_val, &profile_id).await {
                                        Ok(sandbox) => {
                                            state.sandboxes.update(|s| s.push(sandbox));
                                            state.logs.update(|logs| logs.push("Sandbox created".to_string()));
                                            ui_state.close_create_sandbox_modal();
                                        }
                                        Err(e) => {
                                            state.logs.update(|logs| logs.push(format!("Create failed: {}", e)));
                                        }
                                    }
                                });
                            }
                        >
                            "Create"
                        </button>
                        <button
                            class="btn btn-secondary"
                            on:click=move |_| ui_state_sv.get_value().close_create_sandbox_modal()
                        >
                            "Cancel"
                        </button>
                    </div>
                </Modal>
            }.into_any()
        } else {
            view! { <div></div> }.into_any()
        }}
    }
}
