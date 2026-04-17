//! 消息列表组件

use crate::webchat::ChatMessage;
use leptos::html;
use leptos::prelude::*;

/// 消息列表组件
#[component]
pub fn MessageList(
    messages: Signal<Vec<ChatMessage>>,
    #[prop(into)] is_streaming: Signal<bool>,
    #[prop(into)] streaming_content: Signal<String>,
) -> impl IntoView {
    let message_list_ref = NodeRef::<html::Div>::new();

    Effect::new(move |_| {
        messages.get();
        streaming_content.get();
        if let Some(div) = message_list_ref.get() {
            div.set_scroll_top(div.scroll_height());
        }
    });

    view! {
        <div class="message-list" node_ref=message_list_ref>
            <For
                each=move || messages.get()
                key=|msg| msg.id.clone()
                children=move |message| {
                    view! {
                        <MessageItem message=message />
                    }
                }
            />

            {move || {
                if is_streaming.get() {
                    view! {
                        <StreamingMessage content=streaming_content.clone() />
                    }.into_any()
                } else {
                    view! { <div /> }.into_any()
                }
            }}
        </div>
    }
}

/// 消息项组件
#[component]
fn MessageItem(message: ChatMessage) -> impl IntoView {
    let is_user = matches!(message.role, crate::webchat::MessageRole::User);
    let class = if is_user { "message user" } else { "message assistant" };

    view! {
        <div class=class>
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
    }
}

/// 流式消息组件
#[component]
fn StreamingMessage(content: Signal<String>) -> impl IntoView {
    view! {
        <div class="message assistant streaming">
            <div class="message-content">
                {move || content.get()}
            </div>
            <div class="streaming-indicator">
                <span class="cursor">"▋"</span>
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
