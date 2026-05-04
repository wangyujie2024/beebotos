//! Workflow Detail Page — DAG Visualization
//!
//! Displays an interactive SVG graph of workflow step dependencies.

use crate::api::{WorkflowInfo, WorkflowStepInfo};
use crate::state::use_app_state;
use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;
use leptos_router::hooks::use_params_map;

const NODE_WIDTH: i32 = 150;
const NODE_HEIGHT: i32 = 56;
const HORIZ_SPACING: i32 = 200;
const VERT_SPACING: i32 = 100;
const MARGIN: i32 = 40;

/// Compute topological depth for each step (0 = no dependencies)
fn compute_depths(steps: &[WorkflowStepInfo]) -> Vec<(String, usize)> {
    let mut depths: Vec<(String, usize)> = Vec::new();
    let mut computed: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    fn depth_of(
        step_id: &str,
        steps: &[WorkflowStepInfo],
        computed: &mut std::collections::HashMap<String, usize>,
    ) -> usize {
        if let Some(&d) = computed.get(step_id) {
            return d;
        }
        let step = steps.iter().find(|s| s.id == step_id);
        let d = match step {
            Some(s) => match &s.depends_on {
                Some(deps) if !deps.is_empty() => {
                    deps.iter()
                        .map(|dep| depth_of(dep, steps, computed) + 1)
                        .max()
                        .unwrap_or(0)
                }
                _ => 0,
            },
            None => 0,
        };
        computed.insert(step_id.to_string(), d);
        d
    }

    for step in steps {
        let d = depth_of(&step.id, steps, &mut computed);
        depths.push((step.id.clone(), d));
    }
    depths
}

/// Compute node positions for DAG layout
fn layout_nodes(steps: &[WorkflowStepInfo]) -> Vec<(String, i32, i32)> {
    let depths = compute_depths(steps);
    let max_depth = depths.iter().map(|(_, d)| *d).max().unwrap_or(0);

    // Group by depth
    let mut layers: Vec<Vec<String>> = vec![Vec::new(); max_depth + 1];
    for (id, d) in depths {
        layers[d].push(id);
    }

    let mut positions: Vec<(String, i32, i32)> = Vec::new();
    for (depth, layer) in layers.iter().enumerate() {
        let y = MARGIN + depth as i32 * VERT_SPACING;
        let total_width = layer.len() as i32 * NODE_WIDTH + (layer.len().saturating_sub(1) as i32) * (HORIZ_SPACING - NODE_WIDTH);
        let start_x = MARGIN + (total_width.max(0) / 2).saturating_sub(total_width / 2);
        for (idx, step_id) in layer.iter().enumerate() {
            let x = start_x + idx as i32 * HORIZ_SPACING;
            positions.push((step_id.clone(), x, y));
        }
    }
    positions
}

#[component]
pub fn WorkflowDetailPage() -> impl IntoView {
    let params = use_params_map();
    let app_state = use_app_state();

    let workflow_id = move || params.with(|p| p.get("id").unwrap_or_default());

    let workflow: LocalResource<Option<WorkflowInfo>> = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let id = workflow_id();
            let service = app_state.workflow_service();
            async move {
                if id.is_empty() {
                    return None;
                }
                service.get(&id).await.ok()
            }
        }
    });

    view! {
        <Title text="Workflow Detail - BeeBotOS" />
        <div class="page workflow-detail">
            <div class="page-header">
                <div>
                    <h1>{move || workflow.get().and_then(|w| w.map(|w| w.name)).unwrap_or_else(|| "Loading...".to_string())}</h1>
                    <p class="page-description">{move || workflow.get().and_then(|w| w.map(|w| w.description)).unwrap_or_default()}</p>
                </div>
                <a class="btn btn-secondary" href="/workflows">"← Back to Dashboard"</a>
            </div>

            <Suspense fallback=|| view! { <DagSkeleton /> }>
                {move || {
                    workflow.get().map(|w| {
                        w.map(|info| view! { <WorkflowDag info=info /> }.into_any())
                            .unwrap_or_else(|| view! { <div class="error-box">"Workflow not found"</div> }.into_any())
                    }).unwrap_or_else(|| view! { <DagSkeleton /> }.into_any())
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn WorkflowDag(info: WorkflowInfo) -> impl IntoView {
    let positions = layout_nodes(&info.steps);
    let pos_map: std::collections::HashMap<String, (i32, i32)> =
        positions.iter().map(|(id, x, y)| (id.clone(), (*x, *y))).collect();

    let max_x = positions.iter().map(|(_, x, _)| *x).max().unwrap_or(0) + NODE_WIDTH + MARGIN;
    let max_y = positions.iter().map(|(_, _, y)| *y).max().unwrap_or(0) + NODE_HEIGHT + MARGIN;

    let svg_width = max_x.max(600);
    let svg_height = max_y.max(300);

    // Build edges
    let edges: Vec<_> = info
        .steps
        .iter()
        .filter_map(|step| {
            let target_pos = pos_map.get(&step.id)?;
            let deps = step.depends_on.as_ref()?;
            Some(
                deps.iter()
                    .filter_map(|dep_id| {
                        let source_pos = pos_map.get(dep_id)?;
                        Some((dep_id.clone(), source_pos.clone(), step.id.clone(), target_pos.clone()))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect();

    let step_nodes = info.steps.clone();

    view! {
        <div class="dag-container">
            <div class="dag-meta">
                <span class="dag-badge">{format!("{} steps", info.steps_count)}</span>
                <span class="dag-badge">{format!("v{}", info.version)}</span>
                {info.author.clone().map(|a| view! { <span class="dag-badge">{format!("by {}", a)}</span> })}
            </div>
            <svg
                width=svg_width
                height=svg_height
                viewBox=format!("0 0 {} {}", svg_width, svg_height)
                class="dag-svg"
            >
                <defs>
                    <marker
                        id="arrowhead"
                        markerWidth="10"
                        markerHeight="7"
                        refX="9"
                        refY="3.5"
                        orient="auto"
                    >
                        <polygon points="0 0, 10 3.5, 0 7" fill="#94a3b8" />
                    </marker>
                </defs>

                // Edges
                {edges.into_iter().map(|(_dep_id, (sx, sy), _step_id, (tx, ty))| {
                    let start_x = sx + NODE_WIDTH / 2;
                    let start_y = sy + NODE_HEIGHT;
                    let end_x = tx + NODE_WIDTH / 2;
                    let end_y = ty;
                    let mid_y = (start_y + end_y) / 2;
                    let path = format!("M {} {} C {} {}, {} {}, {} {}", start_x, start_y, start_x, mid_y, end_x, mid_y, end_x, end_y);
                    view! {
                        <path d=path fill="none" stroke="#94a3b8" stroke-width="2" marker-end="url(#arrowhead)" />
                    }
                }).collect::<Vec<_>>()}

                // Nodes
                {step_nodes.into_iter().map(|step| {
                    let (x, y) = pos_map.get(&step.id).copied().unwrap_or((0, 0));
                    let has_condition = step.condition.is_some();
                    let node_class = if has_condition { "dag-node dag-node-conditional" } else { "dag-node" };
                    let label_y = y + NODE_HEIGHT / 2 + 4;
                    let sublabel_y = y + NODE_HEIGHT / 2 + 18;
                    view! {
                        <g class=node_class>
                            <rect
                                x=x
                                y=y
                                width=NODE_WIDTH
                                height=NODE_HEIGHT
                                rx="8"
                                ry="8"
                            />
                            <text
                                x=x + NODE_WIDTH / 2
                                y=label_y
                                text-anchor="middle"
                                class="dag-node-label"
                            >
                                {step.id.clone()}
                            </text>
                            <text
                                x=x + NODE_WIDTH / 2
                                y=sublabel_y
                                text-anchor="middle"
                                class="dag-node-sublabel"
                            >
                                {step.skill.clone()}
                            </text>
                            {step.condition.map(|cond| view! {
                                <text
                                    x=x + NODE_WIDTH / 2
                                    y=y + NODE_HEIGHT - 4
                                    text-anchor="middle"
                                    class="dag-node-condition"
                                >
                                    {format!("if: {}", cond.chars().take(20).collect::<String>())}
                                </text>
                            })}
                        </g>
                    }
                }).collect::<Vec<_>>()}
            </svg>
        </div>
    }
}

#[component]
fn DagSkeleton() -> impl IntoView {
    view! {
        <div class="dag-container">
            <div class="dag-skeleton">
                <div class="skeleton-line" style="width: 60%; height: 24px;" />
                <div class="skeleton-line" style="width: 40%; height: 16px; margin-top: 12px;" />
                <div class="skeleton-box" style="width: 100%; height: 300px; margin-top: 24px;" />
            </div>
        </div>
    }
}
