//! 浏览器视口组件

use leptos::prelude::*;

use crate::browser::ConnectionStatus;

/// 浏览器视口组件
#[component]
pub fn BrowserViewport(
    url: String,
    status: ConnectionStatus,
    #[prop(optional)] is_loading: Option<bool>,
    #[prop(optional)] _on_navigate: Option<Box<dyn Fn(String)>>,
    #[prop(optional)] on_refresh: Option<Box<dyn Fn()>>,
) -> impl IntoView {
    let is_connected = matches!(status, ConnectionStatus::Connected);
    let is_loading = is_loading.unwrap_or(false);

    view! {
        <div class="browser-viewport-container">
            // 地址栏
            <div class="browser-address-bar">
                <button
                    class="btn btn-icon"
                    on:click=move |_| {
                        if let Some(ref cb) = on_refresh {
                            cb();
                        }
                    }
                    disabled=!is_connected
                >
                    "⟳"
                </button>

                <input
                    type="text"
                    class="browser-url-input"
                    prop:value=url.clone()
                    placeholder="Enter URL..."
                    disabled=!is_connected
                />

                <button
                    class="btn btn-primary"
                    disabled=!is_connected
                >
                    "Go"
                </button>
            </div>

            // 视口内容
            <div class="browser-viewport">
                {if is_loading {
                    view! {
                        <div class="browser-loading">
                            <div class="loading-spinner"></div>
                            <p>"Loading..."</p>
                        </div>
                    }.into_any()
                } else if is_connected {
                    view! {
                        <iframe
                            src=url.clone()
                            class="browser-iframe"
                            title="Browser Viewport"
                        />
                    }.into_any()
                } else {
                    view! {
                        <div class="browser-placeholder">
                            <div class="placeholder-icon">"🌐"</div>
                            <h3>"No Browser Connected"</h3>
                            <p>"Select a profile to connect"</p>
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
}

/// 浏览器工具栏组件
#[component]
pub fn BrowserToolbar(
    url: String,
    #[prop(optional)] _on_navigate: Option<Box<dyn Fn(String)>>,
    #[prop(optional)] on_back: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_forward: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_refresh: Option<Box<dyn Fn()>>,
) -> impl IntoView {
    view! {
        <div class="browser-toolbar">
            <div class="toolbar-navigation">
                <button
                    class="btn btn-icon"
                    on:click=move |_| {
                        if let Some(ref cb) = on_back {
                            cb();
                        }
                    }
                >
                    "←"
                </button>
                <button
                    class="btn btn-icon"
                    on:click=move |_| {
                        if let Some(ref cb) = on_forward {
                            cb();
                        }
                    }
                >
                    "→"
                </button>
                <button
                    class="btn btn-icon"
                    on:click=move |_| {
                        if let Some(ref cb) = on_refresh {
                            cb();
                        }
                    }
                >
                    "⟳"
                </button>
            </div>

            <div class="toolbar-address">
                <input
                    type="text"
                    class="url-input"
                    prop:value=url
                    placeholder="Enter URL..."
                />
            </div>

            <div class="toolbar-actions">
                <button class="btn btn-icon">"⋮"</button>
            </div>
        </div>
    }
}
