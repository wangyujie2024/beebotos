use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::api::{AgentInfo, AgentLogEntry, AgentStatus, CreateAgentRequest, UpdateAgentRequest};
use crate::components::{InfoItem, Modal};
use crate::state::use_app_state;
use crate::utils::{download_file, event_target_value};

#[component]
pub fn AgentDetail() -> impl IntoView {
    let params = use_params_map();
    let app_state = use_app_state();
    let app_state_stored = StoredValue::new(app_state);

    let agent_id = move || params.with(|p| p.get("id").unwrap_or_default());

    let agent_data: RwSignal<Option<AgentInfo>> = RwSignal::new(None);
    let agent_error = RwSignal::new(None::<String>);
    let edit_open = RwSignal::new(false);
    let delete_confirm_open = RwSignal::new(false);
    let logs_open = RwSignal::new(false);
    let configure_open = RwSignal::new(false);
    let clone_success = RwSignal::new(None::<String>);
    let action_error = RwSignal::new(None::<String>);

    // Logs state
    let logs_data: RwSignal<Vec<AgentLogEntry>> = RwSignal::new(Vec::new());
    let logs_loading = RwSignal::new(false);
    let logs_error = RwSignal::new(None::<String>);

    let fetch_logs = move |id: String| {
        logs_loading.set(true);
        logs_error.set(None);
        let app_state = app_state_stored.get_value();
        spawn_local(async move {
            let service = app_state.agent_service();
            match service.get_logs(&id).await {
                Ok(data) => logs_data.set(data),
                Err(e) => logs_error.set(Some(e.to_string())),
            }
            logs_loading.set(false);
        });
    };
    let fetch_logs_stored = StoredValue::new(fetch_logs);

    // Auto-fetch logs when modal opens
    Effect::new(move |_| {
        if logs_open.get() {
            let id = agent_id();
            if !id.is_empty() {
                fetch_logs_stored.get_value()(id);
            }
        }
    });

    // Fetch agent details
    let fetch_agent = move || {
        let app_state = app_state_stored.get_value();
        let id = agent_id();
        spawn_local(async move {
            if id.is_empty() {
                agent_error.set(Some("Agent ID is required".to_string()));
            } else {
                let service = app_state.agent_service();
                match service.get(&id).await {
                    Ok(data) => agent_data.set(Some(data)),
                    Err(e) => agent_error.set(Some(e.to_string())),
                }
            }
        });
    };

    // Store for reuse
    let fetch_agent_stored = StoredValue::new(fetch_agent);

    // Initial fetch
    fetch_agent_stored.get_value()();

    // Edit modal state
    let edit_name = RwSignal::new(String::new());
    let edit_description = RwSignal::new(String::new());
    let edit_saving = RwSignal::new(false);
    let edit_error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        if let Some(a) = agent_data.get() {
            edit_name.set(a.name);
            edit_description.set(a.description.unwrap_or_default());
        }
    });

    view! {
        <Title text="Agent Details - BeeBotOS" />
        <div class="page agent-detail-page">
            {move || {
                if let Some(error) = agent_error.get() {
                    view! { <AgentDetailError message=error/> }.into_any()
                } else if let Some(agent) = agent_data.get() {
                    let agent_id_start = agent.id.clone();
                    let agent_id_stop = agent.id.clone();
                    let agent_id_edit = agent.id.clone();
                    let agent_id_delete = agent.id.clone();
                    let agent_name_delete = agent.name.clone();
                    let agent_for_clone = agent.clone();
                    let agent_for_export = agent.clone();
                    view! {
                        <AgentDetailView
                            agent=agent
                            on_start={
                                move || {
                                    let app_state = app_state_stored.get_value();
                                    let id = agent_id_start.clone();
                                    spawn_local(async move {
                                        let service = app_state.agent_service();
                                        let _ = service.start(&id).await;
                                        fetch_agent_stored.get_value()();
                                    });
                                }
                            }
                            on_stop={
                                move || {
                                    let app_state = app_state_stored.get_value();
                                    let id = agent_id_stop.clone();
                                    spawn_local(async move {
                                        let service = app_state.agent_service();
                                        let _ = service.stop(&id).await;
                                        fetch_agent_stored.get_value()();
                                    });
                                }
                            }
                            on_edit={
                                move || {
                                    edit_open.set(true);
                                }
                            }
                            on_delete={
                                move || {
                                    delete_confirm_open.set(true);
                                }
                            }
                            on_view_logs={
                                move || {
                                    logs_open.set(true);
                                }
                            }
                            on_configure={
                                move || {
                                    configure_open.set(true);
                                }
                            }
                            on_clone={
                                let app_state = app_state_stored.get_value();
                                move || {
                                    let app_state = app_state.clone();
                                    let name = format!("{} (Clone)", agent_for_clone.name);
                                    let description = agent_for_clone.description.clone();
                                    let capabilities = agent_for_clone.capabilities.clone();
                                    spawn_local(async move {
                                        let service = app_state.agent_service();
                                        let req = CreateAgentRequest {
                                            name,
                                            description,
                                            capabilities,
                                            model_provider: None,
                                            model_name: None,
                                        };
                                        match service.create(req).await {
                                            Ok(_) => {
                                                clone_success.set(Some("Agent cloned successfully".to_string()));
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Success,
                                                    "Agent Cloned",
                                                    "Agent cloned successfully".to_string()
                                                );
                                            }
                                            Err(e) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Error,
                                                    "Clone Failed",
                                                    format!("Failed to clone agent: {}", e)
                                                );
                                            }
                                        }
                                    });
                                }
                            }
                            on_export={
                                let agent = agent_for_export.clone();
                                move || {
                                    let export = serde_json::json!({
                                        "id": agent.id,
                                        "name": agent.name,
                                        "description": agent.description,
                                        "status": agent.status,
                                        "capabilities": agent.capabilities,
                                        "created_at": agent.created_at,
                                        "updated_at": agent.updated_at,
                                    });
                                    if let Ok(json) = serde_json::to_string_pretty(&export) {
                                        download_file(&format!("agent-{}.json", agent.id), &json, "application/json");
                                    }
                                }
                            }
                        />

                        // Edit Modal
                        {move || {
                            let agent_id_edit = agent_id_edit.clone();
                            if edit_open.get() {
                            view! {
                                <Modal title="Edit Agent" on_close=move || edit_open.set(false)>
                                    <div class="modal-body">
                                        {move || edit_error.get().map(|msg| view! {
                                            <div class="alert alert-error">{msg}</div>
                                        })}
                                        <div class="form-group">
                                            <label>"Name"</label>
                                            <input
                                                type="text"
                                                prop:value=edit_name
                                                on:input=move |e| edit_name.set(event_target_value(&e))
                                            />
                                        </div>
                                        <div class="form-group">
                                            <label>"Description"</label>
                                            <textarea
                                                prop:value=edit_description
                                                on:input=move |e| edit_description.set(event_target_value(&e))
                                            />
                                        </div>
                                    </div>
                                    <div class="modal-footer">
                                        <button class="btn btn-secondary" on:click=move |_| edit_open.set(false)>
                                            "Cancel"
                                        </button>
                                        <button
                                            class="btn btn-primary"
                                            on:click={
                                                let id = agent_id_edit.clone();
                                                move |_| {
                                                    let req = UpdateAgentRequest {
                                                        name: Some(edit_name.get()).filter(|s| !s.is_empty()),
                                                        description: Some(edit_description.get()).filter(|s| !s.is_empty()),
                                                        status: None,
                                                        capabilities: None,
                                                        model_provider: None,
                                                        model_name: None,
                                                    };
                                                    edit_saving.set(true);
                                                    edit_error.set(None);
                                                    let app_state = app_state_stored.get_value();
                                                    let id = id.clone();
                                                    spawn_local(async move {
                                                        let service = app_state.agent_service();
                                                        match service.update(&id, req).await {
                                                            Ok(_) => {
                                                                edit_saving.set(false);
                                                                edit_open.set(false);
                                                                fetch_agent_stored.get_value()();
                                                            }
                                                            Err(e) => {
                                                                edit_saving.set(false);
                                                                edit_error.set(Some(format!("Update failed: {}", e)));
                                                            }
                                                        }
                                                    });
                                                }
                                            }
                                            disabled=edit_saving
                                        >
                                            {move || if edit_saving.get() { "Saving..." } else { "Save" }}
                                        </button>
                                    </div>
                                </Modal>
                            }.into_any()
                        } else {
                            ().into_any()
                        }
                        }}

                        // Configure Modal
                        {move || if configure_open.get() {
                            view! {
                                <Modal title="Configure Agent" on_close=move || configure_open.set(false)>
                                    <div class="modal-body">
                                        <p>"Agent-level configuration options will be available here. Global model settings can be configured in LLM Configuration."</p>
                                    </div>
                                    <div class="modal-footer">
                                        <button class="btn btn-secondary" on:click=move |_| configure_open.set(false)>
                                            "Close"
                                        </button>
                                        <button
                                            class="btn btn-primary"
                                            on:click={
                                                let navigate = use_navigate();
                                                move |_| {
                                                    navigate("/llm-config", Default::default());
                                                }
                                            }
                                        >
                                            "Go to LLM Config"
                                        </button>
                                    </div>
                                </Modal>
                            }.into_any()
                        } else {
                            ().into_any()
                        }}

                        // Logs Modal
                        {move || if logs_open.get() {
                            view! {
                                <Modal title="Agent Logs" on_close=move || logs_open.set(false)>
                                    <div class="modal-body">
                                        {move || if logs_loading.get() {
                                            view! { <p>"Loading logs..."</p> }.into_any()
                                        } else if let Some(err) = logs_error.get() {
                                            view! { <div class="alert alert-error">{err}</div> }.into_any()
                                        } else {
                                            let logs = logs_data.get();
                                            if logs.is_empty() {
                                                view! { <p class="text-muted">"No logs available"</p> }.into_any()
                                            } else {
                                                view! {
                                                    <div class="log-list">
                                                        {logs.into_iter().map(|log| {
                                                            let level_class = match log.level.as_str() {
                                                                "error" => "log-error",
                                                                "warn" => "log-warn",
                                                                _ => "log-info",
                                                            };
                                                            view! {
                                                                <div class="log-entry">
                                                                    <span class=format!("log-level {}", level_class)>{log.level.to_uppercase()}</span>
                                                                    <span class="log-time">{log.timestamp}</span>
                                                                    <span class="log-message">{log.message}</span>
                                                                </div>
                                                            }
                                                        }).collect::<Vec<_>>()}
                                                    </div>
                                                }.into_any()
                                            }
                                        }}
                                    </div>
                                </Modal>
                            }.into_any()
                        } else {
                            ().into_any()
                        }}

                        // Delete Confirm Modal
                        {move || {
                            let agent_name_delete = agent_name_delete.clone();
                            let agent_id_delete = agent_id_delete.clone();
                            if delete_confirm_open.get() {
                            view! {
                                <Modal title="Confirm Delete" on_close=move || delete_confirm_open.set(false)>
                                    <div class="modal-body">
                                        <p>{format!("Are you sure you want to delete '{}'? This action cannot be undone.", agent_name_delete)}</p>
                                    </div>
                                    <div class="modal-footer">
                                        <button class="btn btn-secondary" on:click=move |_| delete_confirm_open.set(false)>
                                            "Cancel"
                                        </button>
                                        <button
                                            class="btn btn-danger"
                                            on:click={
                                                let id = agent_id_delete.clone();
                                                move |_| {
                                                    let app_state = app_state_stored.get_value();
                                                    let id = id.clone();
                                                    let navigate = use_navigate();
                                                    spawn_local(async move {
                                                        let service = app_state.agent_service();
                                                        match service.delete(&id).await {
                                                            Ok(_) => {
                                                                delete_confirm_open.set(false);
                                                                navigate("/agents", Default::default());
                                                            }
                                                            Err(e) => {
                                                                action_error.set(Some(format!("Delete failed: {}", e)));
                                                            }
                                                        }
                                                    });
                                                }
                                            }
                                        >
                                            "Delete"
                                        </button>
                                    </div>
                                </Modal>
                            }.into_any()
                        } else {
                            ().into_any()
                        }
                        }}
                    }.into_any()
                } else {
                    view! { <AgentDetailLoading/> }.into_any()
                }
            }}
            {move || action_error.get().map(|msg| view! {
                <div class="alert alert-error">{msg}</div>
            })}
        </div>
    }
}

#[component]
fn AgentDetailView(
    #[prop(into)] agent: AgentInfo,
    on_start: impl Fn() + Clone + 'static,
    on_stop: impl Fn() + Clone + 'static,
    on_edit: impl Fn() + Clone + 'static,
    on_delete: impl Fn() + Clone + 'static,
    on_view_logs: impl Fn() + Clone + 'static,
    on_configure: impl Fn() + Clone + 'static,
    on_clone: impl Fn() + Clone + 'static,
    on_export: impl Fn() + Clone + 'static,
) -> impl IntoView {
    let status_class = match agent.status {
        AgentStatus::Running => "status-running",
        AgentStatus::Stopped | AgentStatus::Idle => "status-idle",
        AgentStatus::Error => "status-error",
        AgentStatus::Pending => "status-pending",
    };

    let is_running = agent.status == AgentStatus::Running;

    // Use Rc<RefCell> for callbacks (not Send/Sync but works in single-threaded
    // WASM)
    let on_start = std::rc::Rc::new(std::cell::RefCell::new(on_start));
    let on_stop = std::rc::Rc::new(std::cell::RefCell::new(on_stop));
    let on_edit = std::rc::Rc::new(std::cell::RefCell::new(on_edit));
    let on_delete = std::rc::Rc::new(std::cell::RefCell::new(on_delete));
    let on_view_logs = std::rc::Rc::new(std::cell::RefCell::new(on_view_logs));
    let on_configure = std::rc::Rc::new(std::cell::RefCell::new(on_configure));
    let on_clone = std::rc::Rc::new(std::cell::RefCell::new(on_clone));
    let on_export = std::rc::Rc::new(std::cell::RefCell::new(on_export));

    view! {
        <div class="agent-detail-header">
            <div class="breadcrumb">
                <A href="/agents">"Agents"</A>
                <span>"/"</span>
                <span>{agent.name.clone()}</span>
            </div>
            <div class="agent-title-section">
                <div class="agent-title">
                    <h1>{agent.name}</h1>
                    <span class=format!("status-badge {}", status_class)>
                        {format!("{:?}", agent.status)}
                    </span>
                </div>
                <div class="agent-actions">
                    {if is_running {
                        let on_stop = on_stop.clone();
                        view! {
                            <button class="btn btn-warning" on:click=move |_| on_stop.borrow_mut()()>
                                "⏹ Stop"
                            </button>
                        }.into_any()
                    } else {
                        let on_start = on_start.clone();
                        view! {
                            <button class="btn btn-success" on:click=move |_| on_start.borrow_mut()()>
                                "▶ Start"
                            </button>
                        }.into_any()
                    }}
                    <button class="btn btn-secondary" on:click=move |_| on_edit.borrow_mut()()>
                        "Edit"
                    </button>
                    <button class="btn btn-danger" on:click=move |_| on_delete.borrow_mut()()>
                        "Delete"
                    </button>
                </div>
            </div>
        </div>

        <div class="agent-detail-grid">
            <div class="agent-detail-main">
                <section class="card">
                    <h2>"Overview"</h2>
                    {agent.description.clone().map(|desc| view! {
                        <p class="agent-description">{desc}</p>
                    })}
                    <div class="agent-info-grid">
                        <InfoItem label="Agent ID" value=agent.id.clone() />
                        <InfoItem
                            label="Created"
                            value=agent.created_at.clone().unwrap_or_else(|| "Unknown".to_string())
                        />
                        <InfoItem
                            label="Updated"
                            value=agent.updated_at.clone().unwrap_or_else(|| "Unknown".to_string())
                        />
                        <InfoItem
                            label="Tasks Completed"
                            value=agent.task_count.map(|t| t.to_string()).unwrap_or_else(|| "0".to_string())
                        />
                        <InfoItem
                            label="Uptime"
                            value=agent.uptime_percent.map(|u| format!("{:.1}%", u)).unwrap_or_else(|| "N/A".to_string())
                        />
                    </div>
                </section>

                <section class="card">
                    <h2>"Capabilities"</h2>
                    <div class="capabilities-list">
                        {if agent.capabilities.is_empty() {
                            view! { <p class="text-muted">"No capabilities configured"</p> }.into_any()
                        } else {
                            view! {
                                <div class="capabilities-list">
                                    {agent.capabilities.iter().map(|cap| view! {
                                        <div class="capability-item">
                                            <span class="capability-icon">"⚡"</span>
                                            <span class="capability-name">{cap.clone()}</span>
                                        </div>
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any()
                        }}
                    </div>
                </section>

                <section class="card">
                    <h2>"Recent Activity"</h2>
                    <AgentActivityLog agent_id=agent.id.clone() />
                </section>
            </div>

            <div class="agent-detail-sidebar">
                <section class="card">
                    <h3>"Quick Stats"</h3>
                    <div class="quick-stats">
                        <StatItem label="Status" value=format!("{:?}", agent.status) />
                        <StatItem label="Tasks" value=agent.task_count.unwrap_or(0).to_string() />
                        <StatItem
                            label="Uptime"
                            value=agent.uptime_percent.map(|u| format!("{:.1}%", u)).unwrap_or_else(|| "N/A".to_string())
                        />
                    </div>
                </section>

                <section class="card">
                    <h3>"Actions"</h3>
                    <div class="action-list">
                        <button
                            class="btn btn-secondary btn-block"
                            on:click=move |_| on_view_logs.borrow_mut()()
                        >
                            "View Logs"
                        </button>
                        <button class="btn btn-secondary btn-block" on:click=move |_| on_configure.borrow_mut()()>"Configure"</button>
                        <button class="btn btn-secondary btn-block" on:click=move |_| on_clone.borrow_mut()()>"Clone Agent"</button>
                        <button class="btn btn-secondary btn-block" on:click=move |_| on_export.borrow_mut()()>"Export Config"</button>
                    </div>
                </section>
            </div>
        </div>
    }
}

#[component]
fn StatItem(#[prop(into)] label: String, #[prop(into)] value: String) -> impl IntoView {
    view! {
        <div class="stat-item">
            <span class="stat-label">{label}</span>
            <span class="stat-value">{value}</span>
        </div>
    }
}

#[component]
fn AgentDetailLoading() -> impl IntoView {
    view! {
        <div class="agent-detail-loading">
            <div class="skeleton-header"></div>
            <div class="skeleton-grid">
                <div class="skeleton-card">
                    <div class="skeleton-line"></div>
                    <div class="skeleton-line"></div>
                </div>
                <div class="skeleton-card">
                    <div class="skeleton-line"></div>
                    <div class="skeleton-line"></div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn AgentDetailError(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <div class="error-state">
            <div class="error-icon">"⚠️"</div>
            <h2>"Agent Not Found"</h2>
            <p>{message}</p>
            <A href="/agents" attr:class="btn btn-primary">
                "Back to Agents"
            </A>
        </div>
    }
}

#[component]
fn AgentActivityLog(#[prop(into)] agent_id: String) -> impl IntoView {
    let app_state = use_app_state();
    let logs = LocalResource::new({
        let agent_id = agent_id.clone();
        move || {
            let service = app_state.agent_service();
            let agent_id = agent_id.clone();
            async move { service.get_logs(&agent_id).await }
        }
    });

    view! {
        <div class="activity-log">
            <Suspense fallback=|| view! { <p class="text-muted">"Loading activity..."</p> }>
                {move || {
                    let logs = logs.clone();
                    Suspend::new(async move {
                        match logs.await {
                            Ok(entries) => {
                                if entries.is_empty() {
                                    view! { <p class="text-muted">"No recent activity"</p> }.into_any()
                                } else {
                                    view! {
                                        <div class="activity-log">
                                            {entries.into_iter().map(|entry| view! {
                                                <div class="activity-item">
                                                    <span class="activity-time">{entry.timestamp}</span>
                                                    <span class="activity-action">{entry.message}</span>
                                                </div>
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }
                            }
                            Err(_) => view! { <p class="text-muted">"Unable to load activity logs"</p> }.into_any(),
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}
