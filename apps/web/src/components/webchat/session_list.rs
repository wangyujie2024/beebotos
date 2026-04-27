//! 会话列表组件

use leptos::prelude::*;

use crate::webchat::ChatSession;

/// 会话列表组件
#[component]
pub fn SessionList(
    sessions: Signal<Vec<ChatSession>>,
    #[prop(into)] selected_id: Signal<String>,
    #[prop(optional)] on_select: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
    #[prop(optional)] on_new: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
) -> impl IntoView {
    view! {
        <div class="session-list-container">
            <div class="session-list-header">
                <h3>"Sessions"</h3>
                <button
                    class="btn btn-primary btn-sm"
                    on:click={
                        let on_new = on_new.clone();
                        move |_| {
                            if let Some(ref cb) = on_new {
                                cb();
                            }
                        }
                    }
                >
                    "+ New"
                </button>
            </div>

            <div class="session-list">
                {move || {
                    let on_select = on_select.clone();
                    let mut sorted_sessions = sessions.get();
                    sorted_sessions.sort_by(|a, b| {
                        if a.is_pinned && !b.is_pinned {
                            std::cmp::Ordering::Less
                        } else if !a.is_pinned && b.is_pinned {
                            std::cmp::Ordering::Greater
                        } else {
                            b.updated_at.cmp(&a.updated_at)
                        }
                    });

                    if sorted_sessions.is_empty() {
                        view! {
                            <div class="empty-sessions">
                                <p>"No sessions yet"</p>
                                <p>"Click 'New' to start chatting"</p>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <For
                                each=move || sorted_sessions.clone()
                                key=|session| session.id.clone()
                                children=move |session: ChatSession| {
                                    let id = session.id.clone();
                                    let is_selected = selected_id.get() == id;
                                    let is_active = !session.is_archived;

                                    {
                                        let id_select = id.clone();
                                        view! {
                                            <SessionListItem
                                                session=session
                                                is_selected=is_selected
                                                is_active=is_active
                                                on_select={
                                                    let on_select = on_select.clone();
                                                    Callback::from(move || {
                                                        if let Some(ref cb) = on_select {
                                                            cb(id_select.clone());
                                                        }
                                                    })
                                                }
                                            />
                                        }
                                    }
                                }
                            />
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

/// 会话列表项组件
#[component]
fn SessionListItem(
    session: ChatSession,
    is_selected: bool,
    is_active: bool,
    #[prop(into)] on_select: Callback<()>,
) -> impl IntoView {
    let class = format!(
        "session-list-item {} {}",
        if is_selected { "selected" } else { "" },
        if !is_active { "archived" } else { "" }
    );

    let preview = session
        .messages
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let preview = if preview.len() > 50 {
        format!("{}...", &preview[..50])
    } else {
        preview
    };

    view! {
        <div
            class=class
            on:click=move |_| {
                on_select.run(());
            }
        >
            <div class="session-info">
                <div class="session-title-row">
                    {if session.is_pinned {
                        view! { <span class="pin-icon">"📌"</span> }.into_any()
                    } else {
                        view! { <div /> }.into_any()
                    }}
                    <span class="session-title">{session.title.clone()}</span>
                </div>
                <div class="session-preview">{preview}</div>
                <div class="session-meta">
                    <span>{format!("{} messages", session.messages.len())}</span>
                    <span>{format_timestamp(&session.updated_at)}</span>
                </div>
            </div>
        </div>
    }
}

fn format_timestamp(timestamp: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        let local = dt.with_timezone(&chrono::Local);
        let now = chrono::Local::now();
        let today = now.date_naive();
        let date = local.date_naive();

        if date == today {
            local.format("%H:%M").to_string()
        } else if date == today.pred_opt().unwrap_or(today) {
            "Yesterday".to_string()
        } else {
            local.format("%Y-%m-%d").to_string()
        }
    } else {
        timestamp.to_string()
    }
}
