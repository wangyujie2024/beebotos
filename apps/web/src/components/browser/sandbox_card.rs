//! 沙箱卡片组件

use leptos::prelude::*;

use crate::browser::sandbox::{BrowserSandbox, SandboxStatus};

/// 沙箱卡片组件
#[component]
pub fn SandboxCard(
    sandbox: BrowserSandbox,
    #[prop(optional)] on_start: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_stop: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_delete: Option<Box<dyn Fn()>>,
) -> impl IntoView {
    let is_running = matches!(sandbox.status, SandboxStatus::Running);

    view! {
        <div
            class="sandbox-card"
            style:border-left-color=sandbox.color.clone()
        >
            <div class="sandbox-header">
                <h4 class="sandbox-name">{sandbox.name.clone()}</h4>
                <span class={format!("sandbox-status {}", status_class(&sandbox.status))}>
                    {format!("{:?}", sandbox.status)}
                </span>
            </div>

            <div class="sandbox-details">
                <div class="detail-row">
                    <span class="detail-label">"CDP Port:"</span>
                    <span class="detail-value">{sandbox.cdp_port}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">"Isolation:"</span>
                    <span class="detail-value">{format!("{:?}", sandbox.isolation)}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">"Memory:"</span>
                    <span class="detail-value">{format!("{} MB", sandbox.resource_limits.memory_limit_mb)}</span>
                </div>
            </div>

            <div class="sandbox-actions">
                {if is_running {
                    view! {
                        <button
                            class="btn btn-warning btn-sm"
                            on:click=move |_| {
                                if let Some(ref cb) = on_stop {
                                    cb();
                                }
                            }
                        >
                            "Stop"
                        </button>
                    }.into_any()
                } else {
                    view! {
                        <button
                            class="btn btn-success btn-sm"
                            on:click=move |_| {
                                if let Some(ref cb) = on_start {
                                    cb();
                                }
                            }
                        >
                            "Start"
                        </button>
                    }.into_any()
                }}

                <button
                    class="btn btn-danger btn-sm btn-icon"
                    on:click=move |_| {
                        if let Some(ref cb) = on_delete {
                            cb();
                        }
                    }
                >
                    "🗑"
                </button>
            </div>
        </div>
    }
}

fn status_class(status: &SandboxStatus) -> &'static str {
    match status {
        SandboxStatus::Running => "running",
        SandboxStatus::Creating => "creating",
        SandboxStatus::Paused => "paused",
        SandboxStatus::Stopped => "stopped",
        SandboxStatus::Cleaning => "cleaning",
        SandboxStatus::Error(_) => "error",
    }
}
