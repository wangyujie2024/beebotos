//! 侧边提问面板组件

use crate::utils::event_target_value;
use crate::webchat::SideQuestion;
use leptos::prelude::*;

/// 侧边提问面板组件
#[component]
pub fn SidePanel(
    questions: Vec<SideQuestion>,
    #[prop(optional)] is_open: Option<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_new_question: Option<Box<dyn Fn(String)>>,
) -> impl IntoView {
    let is_open = is_open.unwrap_or(true);
    let (new_question, set_new_question) = signal(String::new());

    let on_submit = move |_| {
        let content = new_question.get();
        if !content.trim().is_empty() {
            if let Some(ref cb) = on_new_question {
                cb(content);
            }
            set_new_question.set(String::new());
        }
    };

    view! {
        {if !is_open {
            view! { <div /> }.into_any()
        } else {
            view! { <aside class="side-panel">
            <div class="side-panel-header">
                <h4>"Side Questions"</h4>
                <button
                    class="btn btn-icon"
                    on:click=move |_| {
                        if let Some(ref cb) = on_close {
                            cb();
                        }
                    }
                >
                    "✕"
                </button>
            </div>

            <div class="side-questions-list">
                {if questions.is_empty() {
                    view! {
                        <div class="empty-side-questions">
                            <p>"No side questions yet"</p>
                            <p>"Use /btw to ask a side question"</p>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <For
                            each=move || questions.clone()
                            key=|q| q.id.clone()
                            children=move |question| {
                                view! {
                                    <SideQuestionItem question=question />
                                }
                            }
                        />
                    }.into_any()
                }}
            </div>

            <div class="side-panel-footer">
                <div class="new-question-input">
                    <input
                        type="text"
                        placeholder="Ask a side question..."
                        prop:value=new_question
                        on:input=move |ev| {
                            set_new_question.set(event_target_value(&ev));
                        }
                    />
                    <button
                        class="btn btn-primary"
                        disabled=move || new_question.get().trim().is_empty()
                        on:click=on_submit
                    >
                        "Ask"
                    </button>
                </div>
            </div>
        </aside> }.into_any()
        }}
    }
}

/// 侧边提问项组件
#[component]
fn SideQuestionItem(question: SideQuestion) -> impl IntoView {
    let status_class = match question.status {
        crate::webchat::SideQuestionStatus::Pending => "status-pending",
        crate::webchat::SideQuestionStatus::Processing => "status-processing",
        crate::webchat::SideQuestionStatus::Completed => "status-completed",
        crate::webchat::SideQuestionStatus::Failed => "status-failed",
    };

    view! {
        <div class={format!("side-question-item {}", status_class)}>
            <div class="question-text">{question.question.clone()}</div>
            {if let Some(response) = &question.response {
                view! {
                    <div class="response-text">{response.clone()}</div>
                }.into_any()
            } else {
                view! { <div /> }.into_any()
            }}
            <div class="question-meta">
                <span class={format!("status-badge {}", status_class)}>
                    {format!("{:?}", question.status)}
                </span>
            </div>
        </div>
    }
}


