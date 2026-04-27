//! 消息项组件

use leptos::prelude::*;

use crate::webchat::ChatMessage;

/// 消息项组件
#[component]
pub fn MessageItem(
    message: ChatMessage,
    #[prop(optional)] is_streaming: Option<bool>,
) -> impl IntoView {
    let is_streaming = is_streaming.unwrap_or(false);
    let is_user = matches!(message.role, crate::webchat::MessageRole::User);

    let class = format!(
        "message {} {}",
        if is_user { "user" } else { "assistant" },
        if is_streaming { "streaming" } else { "" }
    );

    view! {
        <div class=class>
            <div class="message-avatar">
                {if is_user {
                    "👤"
                } else {
                    "🤖"
                }}
            </div>
            <div class="message-content-wrapper">
                <div class="message-content">
                    {message.content.clone()}
                </div>
                <div class="message-meta">
                    <span class="message-time">{format_timestamp(&message.timestamp)}</span>
                    {if let Some(usage) = &message.token_usage {
                        view! {
                            <span class="token-usage">{usage.format()}</span>
                        }.into_any()
                    } else {
                        view! { <div /> }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}

fn format_timestamp(timestamp: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        let local = dt.with_timezone(&chrono::Local);
        local.format("%H:%M").to_string()
    } else {
        timestamp.to_string()
    }
}
