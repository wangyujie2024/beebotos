//! 调试控制台组件

use leptos::prelude::*;

use crate::browser::debugger::{BrowserLogEntry, LogLevel};
use crate::i18n::I18nContext;
use crate::utils::event_target_value;

/// 调试控制台组件
#[component]
pub fn DebugConsole(
    logs: Vec<BrowserLogEntry>,
    #[prop(optional)] auto_scroll: Option<bool>,
    #[prop(optional)] filter_level: Option<LogLevel>,
    #[prop(optional)] on_clear: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_export: Option<Box<dyn Fn()>>,
) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    let _auto_scroll = auto_scroll.unwrap_or(true);

    // 过滤日志
    let filtered_logs: Vec<_> = logs
        .into_iter()
        .filter(|log| {
            if let Some(ref level) = filter_level {
                log.level >= *level
            } else {
                true
            }
        })
        .collect();

    view! {
        <div class="debug-console">
            <div class="debug-header">
                <h4>{move || i18n_stored.get_value().t("debug-console-title")}</h4>
                <div class="debug-actions">
                    <button
                        class="btn btn-sm"
                        on:click=move |_| {
                            if let Some(ref cb) = on_clear {
                                cb();
                            }
                        }
                    >
                        {move || i18n_stored.get_value().t("debug-console-clear")}
                    </button>
                    <button
                        class="btn btn-sm"
                        on:click=move |_| {
                            if let Some(ref cb) = on_export {
                                cb();
                            }
                        }
                    >
                        {move || i18n_stored.get_value().t("debug-console-export")}
                    </button>
                </div>
            </div>

            <div class="debug-logs">
                {if filtered_logs.is_empty() {
                    view! {
                        <div class="empty-logs">
                            <p>{move || i18n_stored.get_value().t("debug-console-empty")}</p>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <For
                            each=move || filtered_logs.clone()
                            key=|log| log.id.clone()
                            children=move |log| {
                                view! {
                                    <LogEntry log=log />
                                }
                            }
                        />
                    }.into_any()
                }}
            </div>
        </div>
    }
}

/// 日志条目组件
#[component]
fn LogEntry(log: BrowserLogEntry) -> impl IntoView {
    let level_class = match log.level {
        LogLevel::Debug => "log-debug",
        LogLevel::Info => "log-info",
        LogLevel::Warning => "log-warning",
        LogLevel::Error => "log-error",
        LogLevel::Critical => "log-critical",
    };

    let timestamp = &log.timestamp;
    let message = &log.message;
    let source = &log.source;

    view! {
        <div class={format!("log-entry {}", level_class)}>
            <span class="log-timestamp">{timestamp.clone()}</span>
            <span class="log-level">{format!("{:?}", log.level)}</span>
            <span class="log-source">{format!("{:?}", source)}</span>
            <span class="log-message">{message.clone()}</span>
        </div>
    }
}

/// 日志过滤器组件
#[component]
pub fn LogLevelFilter(
    current_level: Option<LogLevel>,
    on_change: Box<dyn Fn(Option<LogLevel>)>,
) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="log-level-filter">
            <label>{move || i18n_stored.get_value().t("debug-console-filter-level")}</label>
            <select
                on:change=move |ev| {
                    let value = event_target_value(&ev);
                    let level = match value.as_str() {
                        "debug" => Some(LogLevel::Debug),
                        "info" => Some(LogLevel::Info),
                        "warning" => Some(LogLevel::Warning),
                        "error" => Some(LogLevel::Error),
                        "critical" => Some(LogLevel::Critical),
                        _ => None,
                    };
                    on_change(level);
                }
            >
                <option value="" selected={current_level.is_none()}>{move || i18n_stored.get_value().t("debug-console-filter-all")}</option>
                <option value="debug" selected={current_level == Some(LogLevel::Debug)}>{move || i18n_stored.get_value().t("debug-console-filter-debug")}</option>
                <option value="info" selected={current_level == Some(LogLevel::Info)}>{move || i18n_stored.get_value().t("debug-console-filter-info")}</option>
                <option value="warning" selected={current_level == Some(LogLevel::Warning)}>{move || i18n_stored.get_value().t("debug-console-filter-warning")}</option>
                <option value="error" selected={current_level == Some(LogLevel::Error)}>{move || i18n_stored.get_value().t("debug-console-filter-error")}</option>
                <option value="critical" selected={current_level == Some(LogLevel::Critical)}>{move || i18n_stored.get_value().t("debug-console-filter-critical")}</option>
            </select>
        </div>
    }
}
