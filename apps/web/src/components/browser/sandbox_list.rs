//! 沙箱列表组件

use leptos::prelude::*;

use crate::browser::sandbox::BrowserSandbox;

/// 沙箱列表组件
#[component]
pub fn SandboxList(
    sandboxes: Vec<BrowserSandbox>,
    #[prop(optional)] on_start: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
    #[prop(optional)] on_stop: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
    #[prop(optional)] on_delete: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
) -> impl IntoView {
    view! {
        <div class="sandbox-list">
            <For
                each=move || sandboxes.clone()
                key=|sandbox| sandbox.id.clone()
                children=move |sandbox: BrowserSandbox| {
                    let id = sandbox.id.clone();
                    view! {
                        {
                            let id_start = id.clone();
                            let id_stop = id.clone();
                            let id_delete = id.clone();
                            view! {
                                <SandboxListItem
                                    sandbox=sandbox
                                    on_start={
                                        let on_start = on_start.clone();
                                        Callback::from(move || {
                                            if let Some(ref cb) = on_start {
                                                cb(id_start.clone());
                                            }
                                        })
                                    }
                                    on_stop={
                                        let on_stop = on_stop.clone();
                                        Callback::from(move || {
                                            if let Some(ref cb) = on_stop {
                                                cb(id_stop.clone());
                                            }
                                        })
                                    }
                                    on_delete={
                                        let on_delete = on_delete.clone();
                                        Callback::from(move || {
                                            if let Some(ref cb) = on_delete {
                                                cb(id_delete.clone());
                                            }
                                        })
                                    }
                                />
                            }
                        }
                    }
                }
            />
        </div>
    }
}

/// 沙箱列表项
#[component]
fn SandboxListItem(
    sandbox: BrowserSandbox,
    #[prop(into)] on_start: Callback<()>,
    #[prop(into)] on_stop: Callback<()>,
    #[prop(into)] on_delete: Callback<()>,
) -> impl IntoView {
    use crate::browser::sandbox::SandboxStatus;

    let is_running = matches!(sandbox.status, SandboxStatus::Running);

    view! {
        <div
            class="sandbox-list-item"
            style:border-left-color=sandbox.color.clone()
        >
            <div class="sandbox-info">
                <div class="sandbox-name">{sandbox.name.clone()}</div>
                <div class="sandbox-meta">
                    {format!("Port: {} | Memory: {} MB",
                        sandbox.cdp_port,
                        sandbox.resource_limits.memory_limit_mb
                    )}
                </div>
            </div>

            <div class="sandbox-actions">
                {if is_running {
                    view! {
                        <button
                            class="btn btn-warning btn-sm"
                            on:click=move |_| {
                                on_stop.run(());
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
                                on_start.run(());
                            }
                        >
                            "Start"
                        </button>
                    }.into_any()
                }}

                <button
                    class="btn btn-danger btn-sm"
                    on:click=move |_| {
                        on_delete.run(());
                    }
                >
                    "Delete"
                </button>
            </div>
        </div>
    }
}
