//! 用量面板组件

use leptos::prelude::*;

use crate::webchat::{TokenUsage, UsagePanel};

/// 用量面板属性
pub struct UsagePanelProps {
    pub usage: UsagePanel,
    pub is_open: bool,
    pub on_close: Option<std::rc::Rc<dyn Fn()>>,
}

/// 用量面板组件
#[component]
pub fn UsagePanelComponent(
    usage: UsagePanel,
    #[prop(optional)] is_open: Option<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn()>>,
) -> impl IntoView {
    let is_open = is_open.unwrap_or(true);

    view! {
        {if !is_open {
            view! { <div /> }.into_any()
        } else {
            view! { <div class="usage-panel">
            <div class="usage-panel-header">
                <h4>"Token Usage"</h4>
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

            <div class="usage-stats">
                <UsageStatItem
                    label="Session"
                    usage=usage.session_usage.clone()
                />
                <UsageStatItem
                    label="Daily"
                    usage=usage.daily_usage.clone()
                />
                <UsageStatItem
                    label="Monthly"
                    usage=usage.monthly_usage.clone()
                />
            </div>

            {if usage.limit_status.has_limit {
                view! {
                    <div class="limit-status">
                        <h5>"Limits"</h5>
                        {if let Some(remaining) = usage.limit_status.daily_remaining {
                            view! {
                                <div class="limit-item">
                                    <span>"Daily Remaining:"</span>
                                    <span>{remaining.to_string()}</span>
                                </div>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }}
                        {if let Some(remaining) = usage.limit_status.monthly_remaining {
                            view! {
                                <div class="limit-item">
                                    <span>"Monthly Remaining:"</span>
                                    <span>{remaining.to_string()}</span>
                                </div>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }}
                        {if usage.limit_status.is_near_limit {
                            view! {
                                <div class="limit-warning">
                                    "⚠️ Approaching limit"
                                </div>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }}
                    </div>
                }.into_any()
            } else {
                view! { <div /> }.into_any()
            }}
        </div> }.into_any()
        }}
    }
}

/// 用量统计项组件
#[component]
fn UsageStatItem(label: &'static str, usage: TokenUsage) -> impl IntoView {
    view! {
        <div class="usage-stat-item">
            <div class="stat-header">
                <span class="stat-label">{label}</span>
                <span class="stat-model">{usage.model.clone()}</span>
            </div>
            <div class="stat-value">{usage.format()}</div>
            <div class="stat-details">
                <span>{format!("Prompt: {}", usage.prompt_tokens)}</span>
                <span>{format!("Completion: {}", usage.completion_tokens)}</span>
            </div>
        </div>
    }
}

/// 小型用量指示器组件
#[component]
pub fn UsageIndicator(usage: TokenUsage) -> impl IntoView {
    view! {
        <div class="usage-indicator">
            <span class="token-icon">"🪙"</span>
            <span class="token-count">{usage.total_tokens.to_string()}</span>
            <span class="token-cost">{format!("${:.4}", usage.estimated_cost)}</span>
        </div>
    }
}
