//! 会话项组件

use leptos::prelude::*;

use crate::webchat::ChatSession;

/// 会话项组件
#[component]
pub fn SessionItem(
    session: ChatSession,
    #[prop(optional)] is_selected: Option<bool>,
    #[prop(optional)] on_click: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_pin: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_delete: Option<Box<dyn Fn()>>,
) -> impl IntoView {
    let is_selected = is_selected.unwrap_or(false);

    view! {
        <div
            class=format!("session-item {}", if is_selected { "selected" } else { "" })
            on:click=move |_| {
                if let Some(ref cb) = on_click {
                    cb();
                }
            }
        >
            <div class="session-content">
                <div class="session-header">
                    <span class="session-title">{session.title.clone()}</span>
                    {if session.is_pinned {
                        view! {
                            <span class="pin-badge" on:click=move |ev| {
                                ev.stop_propagation();
                                if let Some(ref cb) = on_pin {
                                    cb();
                                }
                            }>"📌"</span>
                        }.into_any()
                    } else {
                        view! { <div /> }.into_any()
                    }}
                </div>

                <div class="session-stats">
                    <span>{format!("{} messages", session.messages.len())}</span>
                    {if session.total_token_usage.total_tokens > 0 {
                        view! {
                            <span class="token-count">
                                {session.total_token_usage.format()}
                            </span>
                        }.into_any()
                    } else {
                        view! { <div /> }.into_any()
                    }}
                </div>
            </div>

            <button
                class="btn btn-icon btn-delete"
                on:click=move |ev| {
                    ev.stop_propagation();
                    if let Some(ref cb) = on_delete {
                        cb();
                    }
                }
            >
                "🗑"
            </button>
        </div>
    }
}
