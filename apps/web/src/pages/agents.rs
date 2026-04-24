use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;
use leptos_router::components::A;

use crate::api::{AgentInfo, AgentStatus, CreateAgentRequest};
use crate::components::{ErrorContext, Modal, Pagination, PaginationState, SkeletonGrid};
use crate::state::use_app_state;
use crate::utils::{FormValidator, StringValidators};

const PAGE_SIZE: usize = 9;

#[component]
pub fn AgentsPage() -> impl IntoView {
    let app_state = use_app_state();
    let _error_ctx = expect_context::<ErrorContext>();
    let pagination = RwSignal::new(PaginationState::new(PAGE_SIZE));

    // Use RwSignal with Option for data fetching in CSR mode
    let agents_data: RwSignal<Option<(Vec<AgentInfo>, usize)>> = RwSignal::new(None);
    let agents_error: RwSignal<Option<String>> = RwSignal::new(None);
    let is_loading = RwSignal::new(false);
    let show_create_modal = RwSignal::new(false);

    // Store app_state in StoredValue to avoid Send/Sync issues
    let app_state_stored = StoredValue::new(app_state);

    // Fetch function using StoredValue
    let fetch_agents = move || {
        is_loading.set(true);
        agents_error.set(None);
        let app_state = app_state_stored.get_value();
        let page = pagination.get().current_page;
        let pag = pagination;

        spawn_local(async move {
            let service = app_state.agent_service();
            match service.list_paginated(page, PAGE_SIZE).await {
                Ok(resp) => {
                    let total = resp.total as usize;
                    pag.update(|p| p.set_total(total));
                    agents_data.set(Some((resp.data, total)));
                    is_loading.set(false);
                }
                Err(e) => {
                    agents_error.set(Some(format!("Failed to load agents: {}", e)));
                    is_loading.set(false);
                }
            }
        });
    };

    // Initial fetch
    fetch_agents();

    // Store fetch function for reuse
    let fetch_agents_stored = StoredValue::new(fetch_agents);

    view! {
        <Title text="Agents - BeeBotOS" />
        <div class="page agents-page">
            <div class="page-header">
                <div>
                    <h1>"Agents"</h1>
                    <p class="page-description">"Manage your autonomous AI agents"</p>
                </div>
                <button
                    class="btn btn-primary"
                    on:click=move |_| show_create_modal.set(true)
                >
                    "+ New Agent"
                </button>
            </div>

            {move || {
                if is_loading.get() {
                    view! { <AgentsLoading/> }.into_any()
                } else if let Some(error) = agents_error.get() {
                    view! {
                        <AgentsError
                            message=error
                            on_retry=move || fetch_agents_stored.get_value()()
                        />
                    }.into_any()
                } else if let Some((data, _)) = agents_data.get() {
                    if data.is_empty() {
                        view! { <AgentsEmpty/> }.into_any()
                    } else {
                        view! {
                            <AgentsList
                                agents=data
                                _app_state={app_state_stored}
                                on_delete={
                                    move |id: String| {
                                        let app_state = app_state_stored.get_value();
                                        spawn_local(async move {
                                            let service = app_state.agent_service();
                                            let _ = service.delete(&id).await;
                                            fetch_agents_stored.get_value()();
                                        });
                                    }
                                }
                                on_status_change={
                                    move || fetch_agents_stored.get_value()()
                                }
                            />
                        }.into_any()
                    }
                } else {
                    view! { <AgentsLoading/> }.into_any()
                }
            }}

            {move || {
                let total = agents_data.get().map(|(_, t)| t).unwrap_or(0);
                if total > PAGE_SIZE {
                    view! {
                        <Pagination
                            state=pagination
                            on_change=move |_| fetch_agents_stored.get_value()()
                        />
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }
            }}

            <Show when=move || show_create_modal.get()>
                <CreateAgentModal
                    on_close=move || show_create_modal.set(false)
                    on_created={
                        move || {
                            show_create_modal.set(false);
                            fetch_agents_stored.get_value()();
                        }
                    }
                />
            </Show>
        </div>
    }
}

#[component]
fn AgentsList(
    agents: Vec<AgentInfo>,
    _app_state: StoredValue<crate::state::AppState>,
    on_delete: impl Fn(String) + Clone + 'static,
    on_status_change: impl Fn() + Clone + 'static,
) -> impl IntoView {
    view! {
        <div class="agents-grid">
            {agents.into_iter().map(|agent| {
                let on_delete = on_delete.clone();
                let on_status_change = on_status_change.clone();
                view! {
                    <AgentCard
                        agent=agent
                        on_delete=move |id| on_delete(id)
                        on_status_change=move || on_status_change()
                    />
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

#[component]
fn AgentCard(
    #[prop(into)] agent: AgentInfo,
    on_delete: impl Fn(String) + 'static,
    #[prop(optional)] on_status_change: Option<impl Fn() + Clone + 'static>,
) -> impl IntoView {
    let app_state = use_app_state();
    let status_class = match agent.status {
        AgentStatus::Running => "status-running",
        AgentStatus::Stopped | AgentStatus::Idle => "status-idle",
        AgentStatus::Error => "status-error",
        AgentStatus::Pending => "status-pending",
    };
    let agent_id = agent.id.clone();
    let agent_id_start = agent.id.clone();
    let agent_id_stop = agent.id.clone();

    // Signal to track if action is in progress
    let is_starting = RwSignal::new(false);
    let is_stopping = RwSignal::new(false);

    view! {
        <div class="card agent-card">
            <div class="agent-header">
                <div class="agent-title">
                    <h3>{agent.name.clone()}</h3>
                    <span class=format!("status-badge {}", status_class)>
                        {format!("{:?}", agent.status)}
                    </span>
                </div>
                <button
                    class="btn btn-icon btn-danger"
                    on:click=move |_| on_delete(agent_id.clone())
                >
                    "🗑"
                </button>
            </div>

            {agent.description.clone().map(|desc| view! {
                <p class="agent-description">{desc}</p>
            })}

            <div class="agent-meta">
                <span class="agent-tasks">
                    {agent.task_count.map(|t| format!("{} tasks", t)).unwrap_or_else(|| "0 tasks".to_string())}
                </span>
                <span class="agent-uptime">
                    {agent.uptime_percent.map(|u| format!("{:.1}% uptime", u)).unwrap_or_else(|| "N/A".to_string())}
                </span>
            </div>

            <div class="agent-capabilities">
                {agent.capabilities.clone().into_iter().map(|cap| view! {
                    <span class="capability-tag">{cap}</span>
                }).collect::<Vec<_>>()}
            </div>

            <div class="agent-actions">
                <A href=format!("/agents/{}", agent.id) attr:class="btn btn-primary">
                    "Manage"
                </A>
                {match agent.status {
                    AgentStatus::Running => {
                        let on_status_change = on_status_change.clone();
                        let app_state = app_state.clone();
                        view! {
                            <button
                                class="btn btn-secondary"
                                disabled=is_stopping
                                on:click=move |_| {
                                    let app_state = app_state.clone();
                                    let agent_id = agent_id_stop.clone();
                                    let on_status_change = on_status_change.clone();
                                    is_stopping.set(true);
                                    spawn_local(async move {
                                        let service = app_state.agent_service();
                                        match service.stop(&agent_id).await {
                                            Ok(_) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Success,
                                                    "Agent Stopped",
                                                    format!("Agent {} has been stopped", agent_id)
                                                );
                                                // Trigger refresh
                                                if let Some(ref cb) = on_status_change {
                                                    cb();
                                                }
                                            }
                                            Err(e) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Error,
                                                    "Stop Failed",
                                                    format!("Failed to stop agent: {}", e)
                                                );
                                            }
                                        }
                                        is_stopping.set(false);
                                    });
                                }
                            >
                                {move || if is_stopping.get() { "Stopping..." } else { "Stop" }}
                            </button>
                        }.into_any()
                    }
                    _ => {
                        let on_status_change = on_status_change.clone();
                        let app_state = app_state.clone();
                        view! {
                            <button
                                class="btn btn-secondary"
                                disabled=is_starting
                                on:click=move |_| {
                                    let app_state = app_state.clone();
                                    let agent_id = agent_id_start.clone();
                                    let on_status_change = on_status_change.clone();
                                    is_starting.set(true);
                                    spawn_local(async move {
                                        let service = app_state.agent_service();
                                        match service.start(&agent_id).await {
                                            Ok(_) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Success,
                                                    "Agent Started",
                                                    format!("Agent {} has been started", agent_id)
                                                );
                                                // Trigger refresh
                                                if let Some(ref cb) = on_status_change {
                                                    cb();
                                                }
                                            }
                                            Err(e) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Error,
                                                    "Start Failed",
                                                    format!("Failed to start agent: {}", e)
                                                );
                                            }
                                        }
                                        is_starting.set(false);
                                    });
                                }
                            >
                                {move || if is_starting.get() { "Starting..." } else { "Start" }}
                            </button>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

#[component]
fn AgentsLoading() -> impl IntoView {
    view! {
        <SkeletonGrid count=6 columns=3/>
    }
}

#[component]
fn AgentsEmpty() -> impl IntoView {
    view! {
        <div class="empty-state">
            <div class="empty-icon">"🤖"</div>
            <h3>"No agents yet"</h3>
            <p>"Create your first autonomous agent to get started"</p>
            <button class="btn btn-primary" on:click=move |_| {}>
                "Create Agent"
            </button>
        </div>
    }
}

#[component]
fn AgentsError(#[prop(into)] message: String, on_retry: impl Fn() + 'static) -> impl IntoView {
    view! {
        <div class="error-state">
            <div class="error-icon">"⚠️"</div>
            <h3>"Failed to load agents"</h3>
            <p>{message}</p>
            <div class="error-actions">
                <button class="btn btn-primary" on:click=move |_| on_retry()>
                    "Retry"
                </button>
                <button
                    class="btn btn-secondary"
                    on:click=move |_| { let _ = window().location().reload(); }
                >
                    "Refresh Page"
                </button>
            </div>
        </div>
    }
}

#[component]
fn CreateAgentModal(
    on_close: impl Fn() + Clone + Send + Sync + 'static,
    on_created: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let app_state = use_app_state();
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let model_provider = RwSignal::new(String::from("openai"));
    let model_name = RwSignal::new(String::from("gpt-4"));
    let validator = RwSignal::new(FormValidator::new());
    let is_submitting = RwSignal::new(false);
    let on_close_modal = on_close.clone();
    let on_close_submit = on_close.clone();
    let on_close_cancel = on_close.clone();
    let on_created_submit = on_created.clone();

    view! {
        <Modal title="Create New Agent" on_close=move || on_close_modal()>
            <form on:submit={
                move |ev: leptos::ev::SubmitEvent| {
                    ev.prevent_default();

                    let mut v = FormValidator::new();
                    v.validate(StringValidators::required("name", &name.get()))
                     .validate(StringValidators::min_length("name", &name.get(), 3))
                     .validate(StringValidators::max_length("name", &name.get(), 50));

                    if v.is_valid() {
                        is_submitting.set(true);
                        let service = app_state.agent_service();
                        let req = CreateAgentRequest {
                            name: name.get(),
                            description: Some(description.get()).filter(|s| !s.is_empty()),
                            capabilities: vec![],
                            model_provider: Some(model_provider.get()).filter(|s| !s.is_empty()),
                            model_name: Some(model_name.get()).filter(|s| !s.is_empty()),
                        };
                        let on_created = on_created_submit.clone();
                        let on_close = on_close_submit.clone();

                        spawn_local(async move {
                            let _ = service.create(req).await;
                            is_submitting.set(false);
                            on_created();
                            on_close();
                        });
                    } else {
                        validator.set(v);
                    }
                }
            }>
                <div class="modal-body">
                    <div class=move || format!("form-group {}",
                        if validator.get().has_error("name") { "has-error" } else { "" })>
                        <label>"Agent Name *"</label>
                        <input
                            type="text"
                            placeholder="Enter agent name"
                            prop:value=name
                            on:input=move |e| {
                                name.set(event_target_value(&e));
                                validator.update(|v| { v.validate(StringValidators::required("name", &name.get())); });
                            }
                        />
                        {move || validator.get().first_error_message("name").map(|msg| view! {
                            <span class="form-error">{msg}</span>
                        })}
                    </div>

                    <div class="form-group">
                        <label>"Description"</label>
                        <textarea
                            placeholder="Enter agent description"
                            prop:value=description
                            on:input=move |e| description.set(event_target_value(&e))
                            rows="3"
                        />
                    </div>

                    <div class="form-group">
                        <label>"Model Provider"</label>
                        <select
                            prop:value=model_provider
                            on:change=move |e| model_provider.set(event_target_value(&e))
                        >
                            <option value="openai">"OpenAI"</option>
                            <option value="anthropic">"Anthropic"</option>
                            <option value="kimi">"Kimi"</option>
                            <option value="deepseek">"DeepSeek"</option>
                            <option value="zhipu">"Zhipu"</option>
                            <option value="ollama">"Ollama"</option>
                        </select>
                    </div>

                    <div class="form-group">
                        <label>"Model Name"</label>
                        <input
                            type="text"
                            placeholder="e.g. gpt-4, claude-3-opus-20240229"
                            prop:value=model_name
                            on:input=move |e| model_name.set(event_target_value(&e))
                        />
                    </div>
                </div>
                <div class="modal-footer">
                    <button type="button" class="btn btn-secondary" on:click=move |_| on_close_cancel()>
                        "Cancel"
                    </button>
                    <button
                        type="submit"
                        class="btn btn-primary"
                        disabled=is_submitting
                    >
                        {move || if is_submitting.get() { "Creating..." } else { "Create Agent" }}
                    </button>
                </div>
            </form>
        </Modal>
    }
}
