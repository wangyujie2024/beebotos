//! 配置列表组件

use leptos::prelude::*;

use crate::browser::{BrowserProfile, ConnectionStatus};

/// 配置列表组件
#[component]
pub fn ProfileList(
    profiles: Vec<BrowserProfile>,
    #[prop(optional)] statuses: Option<std::collections::HashMap<String, ConnectionStatus>>,
    #[prop(optional)] selected_id: Option<String>,
) -> impl IntoView {
    let statuses = statuses.unwrap_or_default();

    view! {
        <div class="profile-list">
            <For
                each=move || profiles.clone()
                key=|profile| profile.id.clone()
                children=move |profile| {
                    let id = profile.id.clone();
                    let is_selected = selected_id.as_ref() == Some(&id);
                    let status = statuses.get(&id).cloned().unwrap_or_default();

                    view! {
                        <ProfileListItem
                            profile=profile
                            status=status
                            is_selected=is_selected
                        />
                    }
                }
            />
        </div>
    }
}

/// 配置列表项
#[component]
fn ProfileListItem(
    profile: BrowserProfile,
    status: ConnectionStatus,
    is_selected: bool,
) -> impl IntoView {
    let status_class = match status {
        ConnectionStatus::Connected => "status-connected",
        ConnectionStatus::Connecting => "status-connecting",
        ConnectionStatus::Error(_) => "status-error",
        _ => "status-disconnected",
    };

    view! {
        <div
            class=format!("profile-list-item {}", if is_selected { "selected" } else { "" })
            style:border-left-color=profile.color.clone()
        >
            <div class="profile-indicator {}">{status_class}</div>
            <div class="profile-info">
                <div class="profile-name">{profile.name.clone()}</div>
                <div class="profile-meta">
                    {format!("Port: {}", profile.cdp_port)}
                </div>
            </div>
        </div>
    }
}
