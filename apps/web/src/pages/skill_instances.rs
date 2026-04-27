//! Skill Instance Management Page
//!
//! Create, manage, and execute skill instances bound to agents.

use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;

use crate::api::{CreateInstanceRequest, InstanceInfo};
use crate::state::use_app_state;

#[component]
pub fn SkillInstancesPage() -> impl IntoView {
    let app_state = use_app_state();
    let show_create_form = RwSignal::new(false);
    let create_skill_id = RwSignal::new(String::new());
    let create_agent_id = RwSignal::new(String::new());
    let is_creating = RwSignal::new(false);
    let is_executing = RwSignal::new(None::<String>);

    // Fetch instances
    let instances = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            async move {
                app_state.loading().skills.set(true);
                let result = service.list_instances().await;
                app_state.loading().skills.set(false);
                result
            }
        }
    });

    let reload_instances = {
        let instances = instances.clone();
        move || instances.refetch()
    };

    let create_instance = {
        let app_state = app_state.clone();
        let reload = reload_instances.clone();
        move || {
            let skill_id = create_skill_id.get();
            let agent_id = create_agent_id.get();
            if skill_id.is_empty() || agent_id.is_empty() {
                app_state.notify(
                    crate::state::notification::NotificationType::Warning,
                    "Missing Fields",
                    "Please fill in both Skill ID and Agent ID",
                );
                return;
            }
            is_creating.set(true);
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            let reload = reload.clone();
            leptos::task::spawn_local(async move {
                let req = CreateInstanceRequest {
                    skill_id,
                    agent_id,
                    config: std::collections::HashMap::new(),
                };
                match service.create_instance(req).await {
                    Ok(instance) => {
                        app_state.notify(
                            crate::state::notification::NotificationType::Success,
                            "Instance Created",
                            format!("Instance {} created successfully", instance.instance_id),
                        );
                        create_skill_id.set(String::new());
                        create_agent_id.set(String::new());
                        show_create_form.set(false);
                        reload();
                    }
                    Err(e) => {
                        app_state.notify(
                            crate::state::notification::NotificationType::Error,
                            "Creation Failed",
                            format!("Failed to create instance: {}", e),
                        );
                    }
                }
                is_creating.set(false);
            });
        }
    };
    let create_instance_cb = StoredValue::new(create_instance);

    let delete_instance = {
        let app_state = app_state.clone();
        let reload = reload_instances.clone();
        move |instance_id: String| {
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            let reload = reload.clone();
            leptos::task::spawn_local(async move {
                match service.delete_instance(&instance_id).await {
                    Ok(()) => {
                        app_state.notify(
                            crate::state::notification::NotificationType::Success,
                            "Instance Deleted",
                            format!("Instance {} deleted", instance_id),
                        );
                        reload();
                    }
                    Err(e) => {
                        app_state.notify(
                            crate::state::notification::NotificationType::Error,
                            "Delete Failed",
                            format!("Failed to delete instance: {}", e),
                        );
                    }
                }
            });
        }
    };
    let delete_instance_cb = StoredValue::new(delete_instance);

    let execute_instance = {
        let app_state = app_state.clone();
        move |instance_id: String| {
            is_executing.set(Some(instance_id.clone()));
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            leptos::task::spawn_local(async move {
                match service.execute_instance(&instance_id).await {
                    Ok(resp) => {
                        let msg = if resp.success {
                            format!("Execution completed in {}ms", resp.execution_time_ms)
                        } else {
                            format!("Execution failed: {}", resp.output)
                        };
                        app_state.notify(
                            crate::state::notification::NotificationType::Success,
                            "Execution Result",
                            msg,
                        );
                    }
                    Err(e) => {
                        app_state.notify(
                            crate::state::notification::NotificationType::Error,
                            "Execution Failed",
                            format!("Failed to execute instance: {}", e),
                        );
                    }
                }
                is_executing.set(None);
            });
        }
    };
    let execute_instance_cb = StoredValue::new(execute_instance);

    view! {
        <Title text="Skill Instances - BeeBotOS" />
        <div class="page skill-instances-page">
            <div class="page-header">
                <div>
                    <h1>"Skill Instances"</h1>
                    <p class="page-description">"Manage skill instances bound to your agents"</p>
                </div>
                <button
                    class="btn btn-primary"
                    on:click=move |_| show_create_form.update(|v| *v = !*v)
                >
                    {move || if show_create_form.get() { "✕ Cancel" } else { "+ New Instance" }}
                </button>
            </div>

            {move || if show_create_form.get() {
                view! {
                    <div class="create-form card">
                        <h3>"Create Instance"</h3>
                        <div class="form-group">
                            <label>"Skill ID"</label>
                            <input
                                type="text"
                                placeholder="e.g. echo-skill"
                                prop:value=create_skill_id
                                on:input=move |e| create_skill_id.set(event_target_value(&e))
                            />
                        </div>
                        <div class="form-group">
                            <label>"Agent ID"</label>
                            <input
                                type="text"
                                placeholder="e.g. agent-001"
                                prop:value=create_agent_id
                                on:input=move |e| create_agent_id.set(event_target_value(&e))
                            />
                        </div>
                        <button
                            class="btn btn-primary"
                            disabled=move || is_creating.get()
                            on:click=move |_| create_instance_cb.with_value(|f| f())
                        >
                            {move || if is_creating.get() { "Creating..." } else { "Create Instance" }}
                        </button>
                    </div>
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}

            <Suspense fallback=|| view! { <InstancesLoading/> }>
                {move || {
                    Suspend::new(async move {
                        match instances.await {
                            Ok(data) => {
                                if data.is_empty() {
                                    view! { <InstancesEmpty/> }.into_any()
                                } else {
                                    view! {
                                        <InstancesTable
                                            instances=data
                                            on_delete=move |id| delete_instance_cb.with_value(|f| f(id))
                                            on_execute=move |id| execute_instance_cb.with_value(|f| f(id))
                                            executing_id=is_executing.clone()
                                        />
                                    }.into_any()
                                }
                            }
                            Err(e) => view! { <InstancesError message=e.to_string()/> }.into_any(),
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn InstancesTable(
    instances: Vec<InstanceInfo>,
    on_delete: impl Fn(String) + Clone + Send + Sync + 'static,
    on_execute: impl Fn(String) + Clone + Send + Sync + 'static,
    executing_id: RwSignal<Option<String>>,
) -> impl IntoView {
    view! {
        <div class="instances-table-wrapper">
            <table class="instances-table">
                <thead>
                    <tr>
                        <th>"Instance ID"</th>
                        <th>"Skill"</th>
                        <th>"Agent"</th>
                        <th>"Status"</th>
                        <th>"Usage"</th>
                        <th>"Actions"</th>
                    </tr>
                </thead>
                <tbody>
                    {instances.into_iter().map(|instance| {
                        let status_class = format!("status-badge status-{}", instance.status.to_lowercase());
                        let is_exec = {
                            let id = instance.instance_id.clone();
                            let executing_id = executing_id.clone();
                            move || executing_id.get().as_ref() == Some(&id)
                        };
                        let is_exec2 = is_exec.clone();
                        view! {
                            <tr>
                                <td class="mono">{instance.instance_id.clone()}</td>
                                <td>{instance.skill_id.clone()}</td>
                                <td>{instance.agent_id.clone()}</td>
                                <td><span class=status_class.clone()>{instance.status.clone()}</span></td>
                                <td>
                                    {format!(
                                        "{} calls · {}ms avg",
                                        instance.usage.total_calls,
                                        instance.usage.avg_latency_ms as u64
                                    )}
                                </td>
                                <td class="actions">
                                    <button
                                        class="btn btn-sm btn-primary"
                                        disabled=is_exec
                                        on:click={
                                            let id = instance.instance_id.clone();
                                            let on_execute = on_execute.clone();
                                            move |_| on_execute(id.clone())
                                        }
                                    >
                                        {move || if is_exec2() { "Running..." } else { "▶ Run" }}
                                    </button>
                                    <button
                                        class="btn btn-sm btn-danger"
                                        on:click={
                                            let id = instance.instance_id.clone();
                                            let on_delete = on_delete.clone();
                                            move |_| on_delete(id.clone())
                                        }
                                    >
                                        "Delete"
                                    </button>
                                </td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn InstancesLoading() -> impl IntoView {
    view! {
        <div class="instances-table-wrapper">
            <div class="skeleton-table">
                <div class="skeleton-row"></div>
                <div class="skeleton-row"></div>
                <div class="skeleton-row"></div>
            </div>
        </div>
    }
}

#[component]
fn InstancesEmpty() -> impl IntoView {
    view! {
        <div class="empty-state">
            <div class="empty-icon">"🤖"</div>
            <h3>"No instances yet"</h3>
            <p>"Create a new instance to bind a skill to an agent"</p>
        </div>
    }
}

#[component]
fn InstancesError(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <div class="error-state">
            <div class="error-icon">"⚠️"</div>
            <h3>"Failed to load instances"</h3>
            <p>{message}</p>
        </div>
    }
}
