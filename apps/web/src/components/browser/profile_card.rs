//! 浏览器配置卡片组件

use leptos::prelude::*;

use crate::browser::{BrowserProfile, ConnectionStatus};

/// 配置卡片组件
#[component]
pub fn ProfileCard(
    profile: BrowserProfile,
    status: ConnectionStatus,
    #[prop(optional)] on_connect: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_disconnect: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_edit: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_delete: Option<Box<dyn Fn()>>,
) -> impl IntoView {
    let is_connected = matches!(status, ConnectionStatus::Connected);
    let status_text = format!("{:?}", status);

    view! {
        <div
            class="profile-card"
            style:border-left-color=profile.color.clone()
        >
            <div class="profile-header">
                <h4 class="profile-name">{profile.name.clone()}</h4>
                <span class="profile-type">{format!("{:?}", profile.profile_type)}</span>
            </div>

            <div class="profile-details">
                <div class="detail-row">
                    <span class="detail-label">"CDP Port:"</span>
                    <span class="detail-value">{profile.cdp_port}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">"Status:"</span>
                    <span class={format!("status-badge {}", if is_connected { "connected" } else { "disconnected" })}>
                        {status_text}
                    </span>
                </div>
            </div>

            <div class="profile-actions">
                {if is_connected {
                    view! {
                        <button
                            class="btn btn-danger btn-sm"
                            on:click=move |_| {
                                if let Some(ref cb) = on_disconnect {
                                    cb();
                                }
                            }
                        >
                            "Disconnect"
                        </button>
                    }.into_any()
                } else {
                    view! {
                        <button
                            class="btn btn-primary btn-sm"
                            on:click=move |_| {
                                if let Some(ref cb) = on_connect {
                                    cb();
                                }
                            }
                        >
                            "Connect"
                        </button>
                    }.into_any()
                }}

                <button
                    class="btn btn-secondary btn-sm"
                    on:click=move |_| {
                        if let Some(ref cb) = on_edit {
                            cb();
                        }
                    }
                >
                    "Edit"
                </button>

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_card_creation() {
        let profile = BrowserProfile::new("Test", 9222);
        let status = ConnectionStatus::Connected;

        // Verify profile and status are created correctly
        assert_eq!(profile.name, "Test");
        assert!(matches!(status, ConnectionStatus::Connected));
    }
}
