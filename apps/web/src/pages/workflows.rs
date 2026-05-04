//! Workflow Dashboard Page
//!
//! Visualize workflow definitions, execution stats, recent instances,
//! and skill compositions with full CRUD operations.

use crate::api::{
    CompositionInfo, DashboardStats, ExecuteWorkflowRequest, InstallWorkflowRequest, WorkflowInfo,
    WorkflowInstanceSummary,
};
use crate::components::Modal;
use crate::state::use_app_state;
use leptos::either::Either;
use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;

const POLL_INTERVAL_MS: u32 = 10_000;

// ============================================================================
// Main Page
// ============================================================================

#[component]
pub fn WorkflowDashboardPage() -> impl IntoView {
    let app_state = use_app_state();
    let active_tab = RwSignal::new("workflows".to_string());

    // ---- Fetch dashboard stats ----
    let stats = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let service = app_state.workflow_service();
            async move { service.dashboard_stats().await.ok() }
        }
    });

    // ---- Fetch recent instances ----
    let recent = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let service = app_state.workflow_service();
            async move { service.recent_instances(10).await.ok() }
        }
    });

    // ---- Fetch workflow list ----
    let workflows = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let service = app_state.workflow_service();
            async move { service.list().await.ok() }
        }
    });

    // ---- Fetch composition list ----
    let compositions = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let service = app_state.composition_service();
            async move { service.list().await.ok() }
        }
    });

    // ---- Auto-refresh polling ----
    let should_poll = RwSignal::new(true);

    let stats_refetch = stats.clone();
    let recent_refetch = recent.clone();
    let workflows_refetch = workflows.clone();
    let compositions_refetch = compositions.clone();

    Effect::new(move |_| {
        let stats_r = stats_refetch.clone();
        let recent_r = recent_refetch.clone();
        let workflows_r = workflows_refetch.clone();
        let compositions_r = compositions_refetch.clone();
        leptos::task::spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(POLL_INTERVAL_MS).await;
                if !should_poll.get() {
                    break;
                }
                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    if document.hidden() {
                        continue;
                    }
                }
                stats_r.refetch();
                recent_r.refetch();
                workflows_r.refetch();
                compositions_r.refetch();
            }
        });

        on_cleanup(move || {
            should_poll.set(false);
        });
    });

    // ---- Manual refresh ----
    let refresh_all = move || {
        stats.refetch();
        recent.refetch();
        workflows.refetch();
        compositions.refetch();
    };

    view! {
        <Title text="Workflows - BeeBotOS" />
        <div class="page workflow-dashboard">
            <div class="page-header">
                <div>
                    <h1>"Workflow Dashboard"</h1>
                    <p class="page-description">"Monitor workflow definitions, executions, and skill compositions"</p>
                </div>
                <button class="btn btn-secondary" on:click=move |_| refresh_all()>
                    "🔄 Refresh"
                </button>
            </div>

            // ---- Tab Selector ----
            <div class="hub-selector" style="margin-bottom: 1.5rem;">
                <TabButton
                    label="📋 Workflow 编排"
                    is_active={
                        let tab = active_tab.clone();
                        move || tab.get() == "workflows"
                    }
                    on_click={
                        let tab = active_tab.clone();
                        move || tab.set("workflows".to_string())
                    }
                />
                <TabButton
                    label="🔗 Skill 组合"
                    is_active={
                        let tab = active_tab.clone();
                        move || tab.get() == "compositions"
                    }
                    on_click={
                        let tab = active_tab.clone();
                        move || tab.set("compositions".to_string())
                    }
                />
            </div>

            // ---- Tab Content ----
            {move || {
                if active_tab.get() == "compositions" {
                    view! {
                        <CompositionTab
                            compositions=compositions.clone()
                            refresh_all=refresh_all.clone()
                        />
                    }.into_any()
                } else {
                    view! {
                        <WorkflowTab
                            stats=stats.clone()
                            recent=recent.clone()
                            workflows=workflows.clone()
                            refresh_all=refresh_all.clone()
                        />
                    }.into_any()
                }
            }}
        </div>
    }
}

// ============================================================================
// Tab Button
// ============================================================================

#[component]
fn TabButton(
    #[prop(into)] label: String,
    is_active: impl Fn() -> bool + Clone + Send + Sync + 'static,
    on_click: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <button
            class=move || format!("hub-btn {}", if is_active() { "active" } else { "" })
            on:click=move |_| on_click()
        >
            {label}
        </button>
    }
}

// ============================================================================
// Workflow Tab
// ============================================================================

#[component]
fn WorkflowTab(
    stats: LocalResource<Option<DashboardStats>>,
    recent: LocalResource<Option<Vec<WorkflowInstanceSummary>>>,
    workflows: LocalResource<Option<Vec<WorkflowInfo>>>,
    refresh_all: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let show_install_modal = RwSignal::new(false);
    let refresh_all_for_modal = refresh_all.clone();

    view! {
        // ---- Stats Cards ----
        <Suspense fallback=|| view! { <StatsSkeleton /> }>
            {move || {
                stats.get().map(|s| {
                    s.map(|stats| view! { <StatsCards stats=stats /> }.into_any())
                        .unwrap_or_else(|| view! { <StatsError /> }.into_any())
                }).unwrap_or_else(|| view! { <StatsSkeleton /> }.into_any())
            }}
        </Suspense>

        // ---- Two-column layout: Recent instances + Workflow list ----
        <div class="dashboard-grid">
            <div class="dashboard-panel">
                <h2>"Recent Instances"</h2>
                <Suspense fallback=|| view! { <TableSkeleton rows=5 /> }>
                    {move || {
                        recent.get().map(|r| {
                            r.map(|instances| view! { <RecentInstancesTable instances=instances /> }.into_any())
                                .unwrap_or_else(|| view! { <InstancesError /> }.into_any())
                        }).unwrap_or_else(|| view! { <TableSkeleton rows=5 /> }.into_any())
                    }}
                </Suspense>
            </div>

            <div class="dashboard-panel">
                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;">
                    <h2 style="margin: 0;">"Workflow Definitions"</h2>
                    <button
                        class="btn btn-primary"
                        on:click=move |_| show_install_modal.set(true)
                    >
                        "➕ 添加 Workflow"
                    </button>
                </div>
                <Suspense fallback=|| view! { <TableSkeleton rows=5 /> }>
                    {move || {
                        workflows.get().map(|w| {
                            w.map(|list| view! { <WorkflowList workflows=list refresh=refresh_all.clone() /> }.into_any())
                                .unwrap_or_else(|| view! { <WorkflowsError /> }.into_any())
                        }).unwrap_or_else(|| view! { <TableSkeleton rows=5 /> }.into_any())
                    }}
                </Suspense>
            </div>
        </div>

        // Install Workflow Modal
        {move || if show_install_modal.get() {
            let refresh = refresh_all_for_modal.clone();
            view! {
                <InstallWorkflowModal
                    on_close=move || show_install_modal.set(false)
                    on_installed=move || {
                        show_install_modal.set(false);
                        refresh();
                    }
                />
            }.into_any()
        } else {
            view! { <></> }.into_any()
        }}
    }
}

// ============================================================================
// Composition Tab
// ============================================================================

#[component]
fn CompositionTab(
    compositions: LocalResource<Option<Vec<CompositionInfo>>>,
    refresh_all: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <div class="dashboard-panel" style="margin-top: 0;">
            <h2>"Skill Compositions"</h2>
            <Suspense fallback=|| view! { <TableSkeleton rows=5 /> }>
                {move || {
                    compositions.get().map(|c| {
                        c.map(|list| view! { <CompositionList compositions=list refresh=refresh_all.clone() /> }.into_any())
                            .unwrap_or_else(|| view! { <CompositionsError /> }.into_any())
                    }).unwrap_or_else(|| view! { <TableSkeleton rows=5 /> }.into_any())
                }}
            </Suspense>
        </div>
    }
}

// ============================================================================
// Stats Cards
// ============================================================================

#[component]
fn StatsCards(stats: DashboardStats) -> impl IntoView {
    view! {
        <div class="stats-grid">
            <StatCard
                label="Total Workflows".to_string()
                value=stats.total_workflows.to_string()
                icon="📋"
                color="blue"
            />
            <StatCard
                label="Total Instances".to_string()
                value=stats.total_instances.to_string()
                icon="🔄"
                color="purple"
            />
            <StatCard
                label="Completed".to_string()
                value=stats.completed.to_string()
                icon="✅"
                color="green"
            />
            <StatCard
                label="Failed".to_string()
                value=stats.failed.to_string()
                icon="❌"
                color="red"
            />
            <StatCard
                label="Running".to_string()
                value=stats.running.to_string()
                icon="▶️"
                color="yellow"
            />
            <StatCard
                label="Pending".to_string()
                value=stats.pending.to_string()
                icon="⏳"
                color="gray"
            />
        </div>
    }
}

#[component]
fn StatCard(label: String, value: String, icon: &'static str, color: &'static str) -> impl IntoView {
    let color_class = format!("stat-card stat-card-{}", color);
    view! {
        <div class={color_class}>
            <div class="stat-icon">{icon}</div>
            <div class="stat-value">{value}</div>
            <div class="stat-label">{label}</div>
        </div>
    }
}

// ============================================================================
// Recent Instances Table
// ============================================================================

#[component]
fn RecentInstancesTable(instances: Vec<WorkflowInstanceSummary>) -> impl IntoView {
    if instances.is_empty() {
        return Either::Left(view! {
            <div class="empty-state">
                <span class="empty-icon">"📝"</span>
                <p>"No workflow instances yet"</p>
            </div>
        });
    }

    Either::Right(view! {
        <div class="table-container">
            <table class="data-table">
                <thead>
                    <tr>
                        <th>"Workflow"</th>
                        <th>"Status"</th>
                        <th>"Progress"</th>
                        <th>"Duration"</th>
                        <th>"Started"</th>
                    </tr>
                </thead>
                <tbody>
                    {instances.into_iter().map(|inst| {
                        let status_class = format!("status-badge status-{}", inst.status.to_lowercase());
                        view! {
                            <tr>
                                <td>
                                    <div class="cell-primary">{inst.workflow_name.clone()}</div>
                                    <div class="cell-secondary">{inst.workflow_id.clone()}</div>
                                </td>
                                <td><span class={status_class.clone()}>{inst.status.clone()}</span></td>
                                <td>
                                    <div class="progress-bar">
                                        <div
                                            class="progress-fill"
                                            style={format!("width: {}%", inst.completion_pct.round())}
                                        />
                                    </div>
                                    <span class="progress-text">{format!("{:.0}%", inst.completion_pct)}</span>
                                </td>
                                <td>{format!("{}s", inst.duration_secs)}</td>
                                <td>{inst.started_at.clone()}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    })
}

// ============================================================================
// Workflow Definitions List (with action buttons)
// ============================================================================

#[component]
fn WorkflowList(
    workflows: Vec<WorkflowInfo>,
    refresh: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    if workflows.is_empty() {
        return Either::Left(view! {
            <div class="empty-state">
                <span class="empty-icon">"📋"</span>
                <p>"No workflows defined yet"</p>
            </div>
        });
    }

    Either::Right(view! {
        <div class="workflow-list">
            {workflows.into_iter().map(|wf| {
                view! {
                    <WorkflowCard workflow=wf refresh=refresh.clone() />
                }
            }).collect_view()}
        </div>
    })
}

#[component]
fn WorkflowCard(
    workflow: WorkflowInfo,
    refresh: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let refresh = RwSignal::new(refresh);
    let wf_sig = RwSignal::new(workflow);
    let show_execute_modal = RwSignal::new(false);
    let show_schedule_modal = RwSignal::new(false);
    let show_config_modal = RwSignal::new(false);
    let is_uninstalling = RwSignal::new(false);
    let is_executing = RwSignal::new(false);
    let is_stopping = RwSignal::new(false);

    let trigger_types = wf_sig.get().triggers.iter().map(|t| t.trigger_type.clone()).collect::<Vec<_>>().join(", ");

    view! {
        <div class="workflow-card">
            <div class="workflow-card-header">
                <h3>{wf_sig.get().name.clone()}</h3>
                <span class="workflow-version">{format!("v{}", wf_sig.get().version)}</span>
            </div>
            <p class="workflow-description">{wf_sig.get().description.clone()}</p>
            <div class="workflow-meta">
                <span class="workflow-steps">{format!("{} steps", wf_sig.get().steps_count)}</span>
                <span class="workflow-triggers">{if trigger_types.is_empty() { "No triggers".to_string() } else { trigger_types }}</span>
            </div>
            <div class="workflow-tags">
                {wf_sig.get().tags.into_iter().map(|tag| {
                    view! { <span class="tag">{tag}</span> }
                }).collect_view()}
            </div>
            <div class="workflow-actions">
                <button
                    class="btn btn-sm btn-success"
                    disabled=move || is_executing.get()
                    on:click=move |_| {
                        show_execute_modal.set(true);
                    }
                    title="Manual execute"
                >
                    {move || if is_executing.get() { "⏳" } else { "▶️" }}
                    {move || if is_executing.get() { "Running..." } else { "Start" }}
                </button>
                <button
                    class="btn btn-sm btn-danger"
                    disabled=move || is_stopping.get()
                    on:click=move |_| {
                        let wf_id = wf_sig.get().id.clone();
                        let refresh = refresh.get_untracked();
                        is_stopping.set(true);
                        leptos::task::spawn_local(async move {
                            let app_state = use_app_state();
                            let service = app_state.workflow_service();
                            match service.recent_instances(100).await {
                                Ok(instances) => {
                                    if let Some(inst) = instances.iter().find(|i| i.workflow_id == wf_id && i.status == "running") {
                                        let client = app_state.api_client();
                                        let _resp: Result<serde_json::Value, _> = client.post(&format!("/workflow-instances/{}/cancel", inst.instance_id), &serde_json::json!({})).await;
                                        app_state.notify(
                                            crate::state::notification::NotificationType::Success,
                                            "Workflow Stopped",
                                            format!("Instance {} cancelled", inst.instance_id),
                                        );
                                    } else {
                                        app_state.notify(
                                            crate::state::notification::NotificationType::Warning,
                                            "No Running Instance",
                                            "This workflow is not currently running",
                                        );
                                    }
                                }
                                Err(e) => {
                                    app_state.notify(
                                        crate::state::notification::NotificationType::Error,
                                        "Stop Failed",
                                        format!("Failed to fetch instances: {}", e),
                                    );
                                }
                            }
                            is_stopping.set(false);
                            refresh();
                        });
                    }
                    title="Stop latest running instance"
                >
                    {move || if is_stopping.get() { "⏳" } else { "⏹" }}
                    {move || if is_stopping.get() { "Stopping..." } else { "Stop" }}
                </button>
                <button
                    class="btn btn-sm btn-secondary"
                    on:click=move |_| {
                        show_schedule_modal.set(true);
                    }
                    title="Edit schedule"
                >
                    "⏰ Schedule"
                </button>
                <button
                    class="btn btn-sm btn-secondary"
                    on:click=move |_| {
                        show_config_modal.set(true);
                    }
                    title="Configure workflow parameters"
                >
                    "⚙️ Config"
                </button>
                <button
                    class="btn btn-sm btn-danger"
                    disabled=move || is_uninstalling.get()
                    on:click=move |_| {
                        let wf_id = wf_sig.get().id.clone();
                        let refresh = refresh.get_untracked();
                        let confirmed = web_sys::window()
                            .and_then(|w| w.confirm_with_message(&format!("Uninstall workflow '{}' permanently?", wf_id)).ok())
                            .unwrap_or(false);
                        if !confirmed {
                            return;
                        }
                        is_uninstalling.set(true);
                        leptos::task::spawn_local(async move {
                            let app_state = use_app_state();
                            let service = app_state.workflow_service();
                            match service.uninstall(&wf_id).await {
                                Ok(_) => {
                                    app_state.notify(
                                        crate::state::notification::NotificationType::Success,
                                        "Workflow Uninstalled",
                                        format!("{} removed successfully", wf_id),
                                    );
                                    refresh();
                                }
                                Err(e) => {
                                    app_state.notify(
                                        crate::state::notification::NotificationType::Error,
                                        "Uninstall Failed",
                                        format!("Failed to uninstall {}: {}", wf_id, e),
                                    );
                                }
                            }
                            is_uninstalling.set(false);
                        });
                    }
                    title="Uninstall workflow"
                >
                    {move || if is_uninstalling.get() { "⏳" } else { "🗑" }}
                    {move || if is_uninstalling.get() { "Uninstalling..." } else { "Uninstall" }}
                </button>
                <a class="btn btn-sm btn-primary" href={format!("/workflows/{}", wf_sig.get().id)}>
                    "🔍 DAG"
                </a>
            </div>
        </div>

        // Execute Modal
        {move || if show_execute_modal.get() {
            let wf_id = wf_sig.get().id.clone();
            let refresh = refresh.get_untracked();
            view! {
                <ExecuteWorkflowModal
                    workflow_id=wf_id
                    on_close=move || show_execute_modal.set(false)
                    on_executed=move || {
                        show_execute_modal.set(false);
                        refresh();
                    }
                />
            }.into_any()
        } else {
            view! { <></> }.into_any()
        }}

        // Schedule Modal
        {move || if show_schedule_modal.get() {
            let wf_id = wf_sig.get().id.clone();
            let refresh = refresh.get_untracked();
            view! {
                <ScheduleWorkflowModal
                    workflow_id=wf_id
                    on_close=move || show_schedule_modal.set(false)
                    on_scheduled=move || {
                        show_schedule_modal.set(false);
                        refresh();
                    }
                />
            }.into_any()
        } else {
            view! { <></> }.into_any()
        }}

        // Config Modal
        {move || if show_config_modal.get() {
            let wf_id = wf_sig.get().id.clone();
            let refresh = refresh.get_untracked();
            view! {
                <ConfigWorkflowModal
                    workflow_id=wf_id
                    on_close=move || show_config_modal.set(false)
                    on_configured=move || {
                        show_config_modal.set(false);
                        refresh();
                    }
                />
            }.into_any()
        } else {
            view! { <></> }.into_any()
        }}
    }
}

// ============================================================================
// Execute Workflow Modal
// ============================================================================

#[component]
fn ExecuteWorkflowModal(
    workflow_id: String,
    on_close: impl Fn() + Clone + Send + Sync + 'static,
    on_executed: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let context_input = RwSignal::new("{}".to_string());
    let is_executing = RwSignal::new(false);

    view! {
        <Modal title=format!("Execute Workflow: {}", workflow_id.clone()) on_close={
            let on_close = on_close.clone();
            move || on_close()
        }>
            <div class="modal-body">
                <div class="form-group">
                    <label>"Trigger Context (JSON)"</label>
                    <textarea
                        class="form-control"
                        rows="6"
                        prop:value=context_input
                        on:input=move |e| context_input.set(event_target_value(&e))
                    />
                    <small class="text-muted">"Enter a JSON object to pass as trigger context"</small>
                </div>
                <div class="form-actions">
                    <button
                        class="btn btn-success"
                        disabled=move || is_executing.get()
                        on:click=move |_| {
                            let workflow_id = workflow_id.clone();
                            let on_executed = on_executed.clone();
                            let ctx_str = context_input.get();
                            match serde_json::from_str(&ctx_str) {
                                Ok(context) => {
                                    is_executing.set(true);
                                    let req = ExecuteWorkflowRequest { context, agent_id: None };
                                    leptos::task::spawn_local(async move {
                                        let app_state = use_app_state();
                                        let service = app_state.workflow_service();
                                        match service.execute(&workflow_id, &req).await {
                                            Ok(resp) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Success,
                                                    "Workflow Started",
                                                    format!("Instance {} launched", resp.instance_id),
                                                );
                                                on_executed();
                                            }
                                            Err(e) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Error,
                                                    "Execution Failed",
                                                    format!("{}", e),
                                                );
                                                is_executing.set(false);
                                            }
                                        }
                                    });
                                }
                                Err(e) => {
                                    let app_state = use_app_state();
                                    app_state.notify(
                                        crate::state::notification::NotificationType::Error,
                                        "Invalid JSON",
                                        format!("Please check your JSON syntax: {}", e),
                                    );
                                }
                            }
                        }
                    >
                        {move || if is_executing.get() { "⏳ Executing..." } else { "▶️ Execute" }}
                    </button>
                    <button class="btn btn-secondary" on:click=move |_| { on_close(); }>
                        "Cancel"
                    </button>
                </div>
            </div>
        </Modal>
    }
}

// ============================================================================
// Schedule Workflow Modal
// ============================================================================

/// Helper structs for safe YAML serialization when scheduling workflows
#[derive(serde::Serialize)]
struct ScheduleWorkflowYaml {
    id: String,
    name: String,
    description: String,
    version: String,
    author: String,
    tags: Vec<String>,
    triggers: Vec<ScheduleTriggerYaml>,
    config: ScheduleConfigYaml,
    steps: Vec<serde_json::Value>,
}

#[derive(serde::Serialize)]
struct ScheduleTriggerYaml {
    #[serde(rename = "type")]
    trigger_type: String,
    schedule: String,
    timezone: String,
}

#[derive(serde::Serialize)]
struct ScheduleConfigYaml {
    timeout_sec: u64,
    continue_on_failure: bool,
}

#[component]
fn ScheduleWorkflowModal(
    workflow_id: String,
    on_close: impl Fn() + Clone + Send + Sync + 'static,
    on_scheduled: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let schedule_input = RwSignal::new("0 9 * * *".to_string());
    let tz_input = RwSignal::new("UTC".to_string());
    let is_saving = RwSignal::new(false);

    view! {
        <Modal title=format!("Schedule Workflow: {}", workflow_id.clone()) on_close={
            let on_close = on_close.clone();
            move || on_close()
        }>
            <div class="modal-body">
                <div class="form-group">
                    <label>"Cron Expression"</label>
                    <input
                        class="form-control"
                        type="text"
                        prop:value=schedule_input
                        on:input=move |e| schedule_input.set(event_target_value(&e))
                    />
                    <small class="text-muted">"e.g. 0 9 * * * (daily at 9am), 0 */6 * * * (every 6 hours)"</small>
                </div>
                <div class="form-group">
                    <label>"Timezone"</label>
                    <input
                        class="form-control"
                        type="text"
                        prop:value=tz_input
                        on:input=move |e| tz_input.set(event_target_value(&e))
                    />
                    <small class="text-muted">"e.g. UTC, Asia/Shanghai, America/New_York"</small>
                </div>
                <div class="form-actions">
                    <button
                        class="btn btn-success"
                        disabled=move || is_saving.get()
                        on:click=move |_| {
                            let workflow_id = workflow_id.clone();
                            let on_scheduled = on_scheduled.clone();
                            is_saving.set(true);
                            let schedule = schedule_input.get();
                            let tz = tz_input.get();
                            leptos::task::spawn_local(async move {
                                let app_state = use_app_state();
                                let service = app_state.workflow_service();
                                match service.get(&workflow_id).await {
                                    Ok(wf) => {
                                        let schedule_clone = schedule.clone();
                                        let tz_clone = tz.clone();
                                        let yaml_def = ScheduleWorkflowYaml {
                                            id: wf.id,
                                            name: wf.name,
                                            description: wf.description,
                                            version: wf.version,
                                            author: wf.author.unwrap_or_default(),
                                            tags: wf.tags,
                                            triggers: vec![ScheduleTriggerYaml {
                                                trigger_type: "cron".to_string(),
                                                schedule,
                                                timezone: tz,
                                            }],
                                            config: ScheduleConfigYaml {
                                                timeout_sec: 600,
                                                continue_on_failure: false,
                                            },
                                            steps: vec![],
                                        };
                                        let yaml = serde_yaml::to_string(&yaml_def).unwrap_or_default();
                                        match service.update(&workflow_id, &yaml).await {
                                            Ok(_) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Success,
                                                    "Schedule Updated",
                                                    format!("Cron: {} ({})", schedule_clone, tz_clone),
                                                );
                                                on_scheduled();
                                            }
                                            Err(e) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Error,
                                                    "Update Failed",
                                                    format!("{}", e),
                                                );
                                                is_saving.set(false);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        app_state.notify(
                                            crate::state::notification::NotificationType::Error,
                                            "Fetch Failed",
                                            format!("{}", e),
                                        );
                                        is_saving.set(false);
                                    }
                                }
                            });
                        }
                    >
                        {move || if is_saving.get() { "⏳ Saving..." } else { "💾 Save Schedule" }}
                    </button>
                    <button class="btn btn-secondary" on:click=move |_| { on_close(); }>
                        "Cancel"
                    </button>
                </div>
            </div>
        </Modal>
    }
}

// ============================================================================
// Install Workflow Modal
// ============================================================================

#[component]
fn InstallWorkflowModal(
    on_close: impl Fn() + Clone + Send + Sync + 'static,
    on_installed: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let source_path = RwSignal::new("".to_string());
    let is_installing = RwSignal::new(false);

    view! {
        <Modal title="Install Workflow".to_string() on_close={
            let on_close = on_close.clone();
            move || on_close()
        }>
            <div class="modal-body">
                <div class="form-group">
                    <label>"Workflow File Path"</label>
                    <input
                        class="form-control"
                        type="text"
                        prop:value=source_path
                        on:input=move |e| source_path.set(event_target_value(&e))
                        placeholder="/path/to/workflow.yaml"
                    />
                    <small class="text-muted">"Absolute or relative path to a YAML/JSON workflow file"</small>
                </div>
                <div class="form-actions">
                    <button
                        class="btn btn-success"
                        disabled=move || is_installing.get() || source_path.get().is_empty()
                        on:click=move |_| {
                            let on_installed = on_installed.clone();
                            let path = source_path.get();
                            if path.is_empty() {
                                return;
                            }
                            is_installing.set(true);
                            leptos::task::spawn_local(async move {
                                let app_state = use_app_state();
                                let service = app_state.workflow_service();
                                let req = InstallWorkflowRequest { source_path: path };
                                match service.install(&req).await {
                                    Ok(resp) => {
                                        app_state.notify(
                                            crate::state::notification::NotificationType::Success,
                                            "Workflow Installed",
                                            format!("{} ({}) installed to {}", resp.name, resp.id, resp.installed_path),
                                        );
                                        on_installed();
                                    }
                                    Err(e) => {
                                        app_state.notify(
                                            crate::state::notification::NotificationType::Error,
                                            "Install Failed",
                                            format!("{}", e),
                                        );
                                        is_installing.set(false);
                                    }
                                }
                            });
                        }
                    >
                        {move || if is_installing.get() { "⏳ Installing..." } else { "📥 Install" }}
                    </button>
                    <button class="btn btn-secondary" on:click=move |_| { on_close(); }>
                        "Cancel"
                    </button>
                </div>
            </div>
        </Modal>
    }
}

// ============================================================================
// Config Workflow Modal
// ============================================================================

#[component]
fn ConfigWorkflowModal(
    workflow_id: String,
    on_close: impl Fn() + Clone + Send + Sync + 'static,
    on_configured: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let yaml_content = RwSignal::new("".to_string());
    let is_loading = RwSignal::new(true);
    let is_saving = RwSignal::new(false);

    // Load source on mount
    {
        let workflow_id = workflow_id.clone();
        leptos::task::spawn_local(async move {
            let app_state = use_app_state();
            let service = app_state.workflow_service();
            match service.get_source(&workflow_id).await {
                Ok(resp) => {
                    if let Some(yaml) = resp.get("yaml").and_then(|v| v.as_str()) {
                        yaml_content.set(yaml.to_string());
                    }
                }
                Err(e) => {
                    app_state.notify(
                        crate::state::notification::NotificationType::Error,
                        "Load Failed",
                        format!("Failed to load workflow source: {}", e),
                    );
                }
            }
            is_loading.set(false);
        });
    }

    view! {
        <Modal title=format!("Configure Workflow: {}", workflow_id.clone()) on_close={
            let on_close = on_close.clone();
            move || on_close()
        }>
            <div class="modal-body">
                {move || if is_loading.get() {
                    view! { <div class="skeleton" style="height: 200px;"/> }.into_any()
                } else {
                    view! {
                        <div class="form-group">
                            <label>"Workflow Definition (YAML)"</label>
                            <textarea
                                class="form-control"
                                rows="16"
                                prop:value=yaml_content
                                on:input=move |e| yaml_content.set(event_target_value(&e))
                                style="font-family: monospace; font-size: 0.85rem;"
                            />
                            <small class="text-muted">"Edit the workflow definition directly. Be careful with YAML syntax."</small>
                        </div>
                    }.into_any()
                }}
                <div class="form-actions">
                    <button
                        class="btn btn-success"
                        disabled=move || is_saving.get() || is_loading.get()
                        on:click=move |_| {
                            let workflow_id = workflow_id.clone();
                            let on_configured = on_configured.clone();
                            let yaml = yaml_content.get();
                            is_saving.set(true);
                            leptos::task::spawn_local(async move {
                                let app_state = use_app_state();
                                let service = app_state.workflow_service();
                                match service.update(&workflow_id, &yaml).await {
                                    Ok(_) => {
                                        app_state.notify(
                                            crate::state::notification::NotificationType::Success,
                                            "Workflow Updated",
                                            "Configuration saved successfully",
                                        );
                                        on_configured();
                                    }
                                    Err(e) => {
                                        app_state.notify(
                                            crate::state::notification::NotificationType::Error,
                                            "Update Failed",
                                            format!("{}", e),
                                        );
                                        is_saving.set(false);
                                    }
                                }
                            });
                        }
                    >
                        {move || if is_saving.get() { "⏳ Saving..." } else { "💾 Save" }}
                    </button>
                    <button class="btn btn-secondary" on:click=move |_| { on_close(); }>
                        "Cancel"
                    </button>
                </div>
            </div>
        </Modal>
    }
}

// ============================================================================
// Composition List
// ============================================================================

#[component]
fn CompositionList(
    compositions: Vec<CompositionInfo>,
    refresh: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    if compositions.is_empty() {
        return Either::Left(view! {
            <div class="empty-state">
                <span class="empty-icon">"🔗"</span>
                <p>"No skill compositions defined yet"</p>
                <p class="text-muted">"Create compositions via API or YAML files in data/compositions/"</p>
            </div>
        });
    }

    Either::Right(view! {
        <div class="workflow-list">
            {compositions.into_iter().map(|comp| {
                view! {
                    <CompositionCard composition=comp refresh=refresh.clone() />
                }
            }).collect_view()}
        </div>
    })
}

#[component]
fn CompositionCard(
    composition: CompositionInfo,
    refresh: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let refresh = RwSignal::new(refresh);
    let comp_sig = RwSignal::new(composition);
    let is_deleting = RwSignal::new(false);
    let is_executing = RwSignal::new(false);

    let type_icon = match comp_sig.get().composition_type.as_str() {
        "pipeline" => "⛓️",
        "parallel" => "⚡",
        "conditional" => "🔀",
        "loop" => "🔄",
        _ => "🔗",
    };

    let type_color = match comp_sig.get().composition_type.as_str() {
        "pipeline" => "blue",
        "parallel" => "purple",
        "conditional" => "orange",
        "loop" => "green",
        _ => "gray",
    };

    view! {
        <div class="workflow-card">
            <div class="workflow-card-header">
                <h3>{comp_sig.get().name.clone()}</h3>
                <span class={format!("tag tag-{}", type_color)}>
                    {format!("{} {}", type_icon, comp_sig.get().composition_type)}
                </span>
            </div>
            <p class="workflow-description">{comp_sig.get().description.clone()}</p>
            <div class="workflow-tags">
                {comp_sig.get().tags.into_iter().map(|tag| {
                    view! { <span class="tag">{tag}</span> }
                }).collect_view()}
            </div>
            <div class="workflow-actions">
                <button
                    class="btn btn-sm btn-success"
                    disabled=move || is_executing.get()
                    on:click=move |_| {
                        let comp_id = comp_sig.get().id.clone();
                        let refresh = refresh.get_untracked();
                        is_executing.set(true);
                        leptos::task::spawn_local(async move {
                            let app_state = use_app_state();
                            let service = app_state.composition_service();
                            let req = crate::api::ExecuteCompositionRequest {
                                input: String::new(),
                                agent_id: None,
                            };
                            match service.execute(&comp_id, &req).await {
                                Ok(resp) => {
                                    app_state.notify(
                                        crate::state::notification::NotificationType::Success,
                                        "Composition Executed",
                                        format!("Status: {}", resp.status),
                                    );
                                }
                                Err(e) => {
                                    app_state.notify(
                                        crate::state::notification::NotificationType::Error,
                                        "Execution Failed",
                                        format!("{}", e),
                                    );
                                }
                            }
                            is_executing.set(false);
                            refresh();
                        });
                    }
                    title="Execute composition"
                >
                    {move || if is_executing.get() { "⏳" } else { "▶️" }}
                    {move || if is_executing.get() { "Running..." } else { "Execute" }}
                </button>
                <button
                    class="btn btn-sm btn-danger"
                    disabled=move || is_deleting.get()
                    on:click=move |_| {
                        let comp_id = comp_sig.get().id.clone();
                        let refresh = refresh.get_untracked();
                        let confirmed = web_sys::window()
                            .and_then(|w| w.confirm_with_message(&format!("Delete composition '{}' permanently?", comp_id)).ok())
                            .unwrap_or(false);
                        if !confirmed {
                            return;
                        }
                        is_deleting.set(true);
                        leptos::task::spawn_local(async move {
                            let app_state = use_app_state();
                            let service = app_state.composition_service();
                            match service.delete(&comp_id).await {
                                Ok(()) => {
                                    app_state.notify(
                                        crate::state::notification::NotificationType::Success,
                                        "Composition Deleted",
                                        format!("{} removed successfully", comp_id),
                                    );
                                    refresh();
                                }
                                Err(e) => {
                                    app_state.notify(
                                        crate::state::notification::NotificationType::Error,
                                        "Delete Failed",
                                        format!("Failed to delete {}: {}", comp_id, e),
                                    );
                                }
                            }
                            is_deleting.set(false);
                        });
                    }
                    title="Delete composition"
                >
                    {move || if is_deleting.get() { "⏳" } else { "🗑" }}
                    {move || if is_deleting.get() { "Deleting..." } else { "Delete" }}
                </button>
            </div>
        </div>
    }
}

// ============================================================================
// Skeleton / Error placeholders
// ============================================================================

#[component]
fn StatsSkeleton() -> impl IntoView {
    view! {
        <div class="stats-grid">
            {(0..6).map(|_| {
                view! { <div class="stat-card stat-card-skeleton"><div class="skeleton stat-skeleton"/></div> }
            }).collect_view()}
        </div>
    }
}

#[component]
fn StatsError() -> impl IntoView {
    view! {
        <div class="stats-grid">
            {(0..6).map(|_| {
                view! { <div class="stat-card stat-card-error">"—"</div> }
            }).collect_view()}
        </div>
    }
}

#[component]
fn TableSkeleton(rows: usize) -> impl IntoView {
    view! {
        <div class="table-container">
            <table class="data-table">
                <tbody>
                    {(0..rows).map(|_| {
                        view! {
                            <tr>
                                <td><div class="skeleton text-skeleton"/></td>
                                <td><div class="skeleton text-skeleton"/></td>
                                <td><div class="skeleton text-skeleton"/></td>
                                <td><div class="skeleton text-skeleton"/></td>
                                <td><div class="skeleton text-skeleton"/></td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn InstancesError() -> impl IntoView {
    view! {
        <div class="error-box">"Failed to load recent instances"</div>
    }
}

#[component]
fn WorkflowsError() -> impl IntoView {
    view! {
        <div class="error-box">"Failed to load workflow definitions"</div>
    }
}

#[component]
fn CompositionsError() -> impl IntoView {
    view! {
        <div class="error-box">"Failed to load skill compositions"</div>
    }
}
