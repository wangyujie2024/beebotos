//! Channels Management Page
//!
//! Reference: /data/copaw-style-web channel management design

use crate::api::{ChannelService, ChannelInfo, ChannelStatus, WeChatQrResponse, QrStatusResponse, ChannelConfig};
use wasm_bindgen::JsCast;
use crate::components::InlineLoading;
use crate::i18n::I18nContext;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;

#[component]
pub fn ChannelsPage() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    // Channel service
    let client = crate::api::create_client();
    let channel_service = ChannelService::new(client);
    let service_stored = StoredValue::new(channel_service);

    // Channels resource
    let channels = LocalResource::new(move || {
        let service = service_stored.get_value();
        async move {
            service.list().await.unwrap_or_default()
        }
    });

    // Selected channel for configuration
    let (selected_channel, set_selected_channel) = signal::<Option<ChannelInfo>>(None);

    // QR code state for WeChat
    let (qr_code, set_qr_code) = signal::<Option<WeChatQrResponse>>(None);
    let (qr_loading, set_qr_loading) = signal(false);
    let (qr_error, set_qr_error) = signal::<Option<String>>(None);
    let (qr_status, set_qr_status) = signal::<Option<QrStatusResponse>>(None);
    let (qr_polling, set_qr_polling) = signal(false);

    // Config panel open state
    let (config_panel_open, set_config_panel_open) = signal(false);

    // Action feedback
    let (action_message, set_action_message) = signal::<Option<String>>(None);
    let (action_error, set_action_error) = signal::<Option<String>>(None);

    // Config form state (bound to inputs)
    let (base_url, set_base_url) = signal(String::new());
    let (bot_token, set_bot_token) = signal(String::new());
    let (auto_reconnect, set_auto_reconnect) = signal(true);

    // Refresh channels list
    let refresh_channels = move || {
        channels.refetch();
    };
    let refresh_stored = StoredValue::new(refresh_channels);

    view! {
        <div class="channels-page">
            // Page Header
            <div class="page-header">
                <h2>{move || i18n_stored.get_value().t("channels-title")}</h2>
                <p>{move || i18n_stored.get_value().t("channels-subtitle")}</p>
            </div>

            {move || action_message.get().map(|msg| view! {
                <div class="alert alert-success">{msg}</div>
            })}
            {move || action_error.get().map(|msg| view! {
                <div class="alert alert-error">{msg}</div>
            })}

            // Channels Grid
            <Suspense fallback=|| view! { <InlineLoading /> }>
                {move || {
                    channels.get().map(|channel_list| {
                        view! {
                            <div class="channels-grid">
                                {channel_list.into_iter().map(|channel| {
                                    let channel_for_click = channel.clone();
                                    let is_enabled = channel.enabled;
                                    let status_class = match channel.status {
                                        ChannelStatus::Connected => "status-active",
                                        ChannelStatus::Error => "status-error",
                                        _ => "",
                                    };

                                    view! {
                                        <div
                                            class="channel-card"
                                            on:click=move |_| {
                                                set_selected_channel.set(Some(channel_for_click.clone()));
                                                // Initialize form from channel config
                                                if let Some(ref cfg) = channel_for_click.config {
                                                    set_base_url.set(cfg.base_url.clone().unwrap_or_default());
                                                    set_bot_token.set(cfg.bot_token.clone().unwrap_or_default());
                                                    set_auto_reconnect.set(cfg.auto_reconnect.unwrap_or(true));
                                                } else {
                                                    set_base_url.set(String::new());
                                                    set_bot_token.set(String::new());
                                                    set_auto_reconnect.set(true);
                                                }
                                                set_config_panel_open.set(true);
                                                set_action_message.set(None);
                                                set_action_error.set(None);
                                            }
                                        >
                                            <div class={format!("channel-icon {}", channel.id)}>
                                                {channel.icon.clone()}
                                            </div>
                                            <div class="channel-info">
                                                <h3>{channel.name.clone()}</h3>
                                                <p>{channel.description.clone()}</p>
                                            </div>
                                            <div class="channel-status">
                                                <span class={format!("status-badge {}", status_class)}>
                                                    {if is_enabled {
                                                        i18n_stored.get_value().t("status-enabled")
                                                    } else {
                                                        i18n_stored.get_value().t("status-disabled")
                                                    }}
                                                </span>
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }
                    })
                }}
            </Suspense>

            // Configuration Panel (Slide-out)
            {move || {
                if config_panel_open.get() {
                    selected_channel.get().map(|channel| {
                        let channel_id_enable = channel.id.clone();
                        let channel_id_test = channel.id.clone();
                        let channel_id_save = channel.id.clone();

                        view! {
                            <>
                                // Overlay
                                <div
                                    class="overlay show"
                                    on:click=move |_| {
                                        set_qr_polling.set(false);
                                        set_config_panel_open.set(false);
                                    }
                                />
                                // Config Panel
                                <div class="config-panel open">
                                    <div class="config-panel-header">
                                        <h3>{format!("{} {}", channel.icon, channel.name)}</h3>
                                        <button
                                            class="close-btn"
                                            on:click=move |_| {
                                                set_qr_polling.set(false);
                                                set_config_panel_open.set(false);
                                            }
                                        >
                                            "✕"
                                        </button>
                                    </div>
                                    <div class="config-panel-body">
                                        // Channel Status Section
                                        <div class="config-section">
                                            <h4>{i18n_stored.get_value().t("channel-status")}</h4>
                                            <div class="status-display">
                                                <span class={format!("status-dot {}", match channel.status {
                                                    ChannelStatus::Connected => "connected",
                                                    ChannelStatus::Error => "error",
                                                    _ => "disconnected",
                                                })} />
                                                <span>{format!("{:?}", channel.status)}</span>
                                            </div>
                                            {channel.last_error.clone().map(|err| view! {
                                                <div class="error-message">{err}</div>
                                            })}
                                        </div>

                                        // Enable/Disable toggle
                                        <div class="config-section">
                                            <div class="form-group checkbox-group">
                                                <label class="checkbox-label">
                                                    <input
                                                        type="checkbox"
                                                        checked={channel.enabled}
                                                        on:change=move |e| {
                                                            let enabled = event_target_checked(&e);
                                                            let service = service_stored.get_value();
                                                            let id = channel_id_enable.clone();
                                                            let refresh = refresh_stored.get_value();
                                                            spawn_local(async move {
                                                                match service.set_enabled(&id, enabled).await {
                                                                    Ok(_) => {
                                                                        set_action_message.set(Some(
                                                                            if enabled { "Channel enabled".to_string() } else { "Channel disabled".to_string() }
                                                                        ));
                                                                        refresh();
                                                                    }
                                                                    Err(err) => {
                                                                        set_action_error.set(Some(format!("Failed to toggle: {}", err)));
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    />
                                                    <span>{i18n_stored.get_value().t("config-enabled")}</span>
                                                </label>
                                            </div>
                                        </div>

                                        // WeChat QR Code Section
                                        {if channel.id == "wechat" {
                                            Some(view! {
                                                <div class="config-section">
                                                    <h4>{i18n_stored.get_value().t("wechat-login")}</h4>
                                                    <p class="config-hint">
                                                        {i18n_stored.get_value().t("wechat-login-hint")}
                                                    </p>

                                                    {move || if qr_loading.get() {
                                                        view! { <InlineLoading /> }.into_any()
                                                    } else if let Some(error) = qr_error.get() {
                                                        view! {
                                                            <div class="error-message">
                                                                {error}
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        qr_code.get().map(|qr| view! {
                                                            <div class="qr-code-container">
                                                                {qr.qrcode_img_content.map(|url| view! {
                                                                    <div class="qr-link-box">
                                                                        <a href={url.clone()} target="_blank" rel="noopener" class="qr-link">
                                                                            <span class="qr-link-icon">[扫码]</span>
                                                                            <span>"点击打开微信扫码页面"</span>
                                                                        </a>
                                                                        <p class="qr-hint">"请使用微信扫描页面中的二维码"</p>
                                                                    </div>
                                                                })}
                                                                <p class="qr-text">{format!("二维码: {}", qr.qrcode.clone())}</p>
                                                                <p class="qr-expiry">
                                                                    {i18n_stored.get_value().t("qr-expires-in")}
                                                                    {format!(" {}s", qr.expires_in)}
                                                                </p>
                                                                {move || qr_status.get().map(|status| {
                                                                    let (icon, text, class) = match status.status.as_str() {
                                                                        "confirmed" => ("✅", "扫码成功，登录完成", "status-success"),
                                                                        "scanned" => ("📱", "已扫码，等待确认", "status-pending"),
                                                                        "expired" => ("❌", "二维码已过期，请重新获取", "status-error"),
                                                                        _ => ("⏳", "等待扫码...", "status-pending"),
                                                                    };
                                                                    view! {
                                                                        <p class={format!("qr-status {}", class)}>
                                                                            {icon} " " {text}
                                                                        </p>
                                                                    }
                                                                })}
                                                            </div>
                                                        }).into_any()
                                                    }}

                                                    <button
                                                        class="btn-primary btn-block"
                                                        on:click=move |_| {
                                                            set_qr_loading.set(true);
                                                            set_qr_polling.set(false);
                                                            set_qr_status.set(None);
                                                            let service = service_stored.get_value();
                                                            spawn_local(async move {
                                                                match service.get_wechat_qr().await {
                                                                    Ok(qr) => {
                                                                        set_qr_code.set(Some(qr.clone()));
                                                                        set_qr_error.set(None);
                                                                        set_qr_polling.set(true);
                                                                        // Start polling QR status
                                                                        let poll_service = service_stored.get_value();
                                                                        spawn_local(async move {
                                                                            loop {
                                                                                gloo_timers::future::TimeoutFuture::new(2000).await;
                                                                                if !qr_polling.get() {
                                                                                    break;
                                                                                }
                                                                                match poll_service.check_wechat_qr(&qr.qrcode).await {
                                                                                    Ok(status) => {
                                                                                        let should_stop = status.status == "confirmed" || status.status == "expired";
                                                                                        set_qr_status.set(Some(status));
                                                                                        if should_stop {
                                                                                            set_qr_polling.set(false);
                                                                                            break;
                                                                                        }
                                                                                    }
                                                                                    Err(e) => {
                                                                                        set_qr_error.set(Some(format!("轮询二维码状态失败: {:?}", e)));
                                                                                        set_qr_polling.set(false);
                                                                                        break;
                                                                                    }
                                                                                }
                                                                            }
                                                                        });
                                                                    }
                                                                    Err(e) => {
                                                                        set_qr_error.set(Some(format!("获取二维码失败: {:?}", e)));
                                                                    }
                                                                }
                                                                set_qr_loading.set(false);
                                                            });
                                                        }
                                                    >
                                                        {if qr_code.get().is_some() {
                                                            i18n_stored.get_value().t("action-refresh-qr")
                                                        } else {
                                                            i18n_stored.get_value().t("action-get-qr")
                                                        }}
                                                    </button>
                                                </div>
                                            })
                                        } else {
                                            None
                                        }}

                                        // Configuration Form
                                        <div class="config-section">
                                            <h4>{i18n_stored.get_value().t("channel-config")}</h4>
                                            <div class="form-group">
                                                <label>{i18n_stored.get_value().t("config-base-url")}</label>
                                                <input
                                                    type="text"
                                                    placeholder="https://ilinkai.weixin.qq.com"
                                                    prop:value=base_url
                                                    on:input=move |e| set_base_url.set(event_target_value(&e))
                                                />
                                            </div>
                                            <div class="form-group">
                                                <label>{i18n_stored.get_value().t("config-bot-token")}</label>
                                                <input
                                                    type="password"
                                                    placeholder="••••••••"
                                                    prop:value=bot_token
                                                    on:input=move |e| set_bot_token.set(event_target_value(&e))
                                                />
                                            </div>
                                            <div class="form-group">
                                                <label class="checkbox-label">
                                                    <input
                                                        type="checkbox"
                                                        checked=auto_reconnect
                                                        on:change=move |e| set_auto_reconnect.set(event_target_checked(&e))
                                                    />
                                                    <span>{i18n_stored.get_value().t("config-auto-reconnect")}</span>
                                                </label>
                                            </div>
                                        </div>

                                        // Actions
                                        <div class="config-actions">
                                            <button
                                                class="btn-secondary"
                                                on:click=move |_| {
                                                    let service = service_stored.get_value();
                                                    let id = channel_id_test.clone();
                                                    spawn_local(async move {
                                                        match service.test_connection(&id).await {
                                                            Ok(resp) => {
                                                                if resp.success {
                                                                    set_action_message.set(Some("Connection test passed".to_string()));
                                                                } else {
                                                                    set_action_error.set(Some("Connection test failed".to_string()));
                                                                }
                                                            }
                                                            Err(e) => {
                                                                set_action_error.set(Some(format!("Test failed: {}", e)));
                                                            }
                                                        }
                                                    });
                                                }
                                            >
                                                {i18n_stored.get_value().t("action-test")}
                                            </button>
                                            <button
                                                class="btn-primary"
                                                on:click=move |_| {
                                                    let service = service_stored.get_value();
                                                    let id = channel_id_save.clone();
                                                    let cfg = ChannelConfig {
                                                        base_url: Some(base_url.get()).filter(|s| !s.is_empty()),
                                                        bot_token: Some(bot_token.get()).filter(|s| !s.is_empty()),
                                                        auto_reconnect: Some(auto_reconnect.get()),
                                                        bot_base_url: None,
                                                        reconnect_interval_secs: None,
                                                        webhook_url: None,
                                                        api_key: None,
                                                        api_secret: None,
                                                        extra: None,
                                                    };
                                                    let refresh = refresh_stored.get_value();
                                                    spawn_local(async move {
                                                        match service.update(&id, cfg).await {
                                                            Ok(_) => {
                                                                set_action_message.set(Some("Configuration saved".to_string()));
                                                                set_config_panel_open.set(false);
                                                                refresh();
                                                            }
                                                            Err(e) => {
                                                                set_action_error.set(Some(format!("Save failed: {}", e)));
                                                            }
                                                        }
                                                    });
                                                }
                                            >
                                                {i18n_stored.get_value().t("action-save")}
                                            </button>
                                        </div>
                                    </div>
                                </div>
                            </>
                        }
                    })
                } else {
                    None
                }
            }}
        </div>
    }
}

fn event_target_value(ev: &leptos::ev::Event) -> String {
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|i| i.value())
        .unwrap_or_default()
}

fn event_target_checked(ev: &leptos::ev::Event) -> bool {
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|i| i.checked())
        .unwrap_or(false)
}
