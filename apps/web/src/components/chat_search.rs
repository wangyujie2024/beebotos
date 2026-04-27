//! Chat Search Component
//!
//! Provides message search functionality for chat interfaces

use crate::i18n::I18nContext;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Search result item
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub message: String,
    pub sender: String,
    pub timestamp: String,
    pub channel: String,
    pub highlighted_text: String,
}

/// Chat Search Component
#[component]
pub fn ChatSearch(
    #[prop(into)] on_search: Callback<String>,
    #[prop(into)] on_result_click: Callback<String>,
    results: Signal<Vec<SearchResult>>,
    is_loading: Signal<bool>,
) -> impl IntoView {
    let search_query = RwSignal::new(String::new());
    let is_expanded = RwSignal::new(false);
    let selected_index = RwSignal::new(0);
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    // Debounced search
    let debounced_search = move || {
        let query = search_query.get();
        if query.len() >= 2 {
            on_search.run(query);
        }
    };

    view! {
        <div class="chat-search">
            <div class="chat-search-input-wrapper">
                <input
                    type="text"
                    class="chat-search-input"
                    placeholder=move || i18n_stored.get_value().t("search-messages-placeholder")
                    prop:value=search_query
                    on:focus=move |_| is_expanded.set(true)
                    on:input=move |e| {
                        search_query.set(event_target_value(&e));
                        debounced_search();
                    }
                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                        match ev.key().as_str() {
                            "ArrowDown" => {
                                ev.prevent_default();
                                selected_index.update(|i| {
                                    let max = results.get().len().saturating_sub(1);
                                    *i = (*i + 1).min(max);
                                });
                            }
                            "ArrowUp" => {
                                ev.prevent_default();
                                selected_index.update(|i| *i = i.saturating_sub(1));
                            }
                            "Enter" => {
                                ev.prevent_default();
                                let results_vec = results.get();
                                if let Some(result) = results_vec.get(selected_index.get()) {
                                    on_result_click.run(result.id.clone());
                                    is_expanded.set(false);
                                }
                            }
                            "Escape" => {
                                is_expanded.set(false);
                            }
                            _ => {}
                        }
                    }
                />
                <span class="search-icon">"🔍"</span>
                {move || if is_loading.get() {
                    view! { <span class="search-loading">"⏳"</span> }.into_any()
                } else {
                    view! { <></> }.into_any()
                }}
            </div>

            <Show when=move || is_expanded.get() && !results.get().is_empty()>
                <div class="chat-search-results">
                    <div class="search-results-header">
                        <span>{move || format!("{} {}", results.get().len(), i18n_stored.get_value().t("search-results"))}</span>
                        <button
                            class="btn btn-sm btn-ghost"
                            on:click=move |_| is_expanded.set(false)
                        >
                            {move || i18n_stored.get_value().t("action-close")}
                        </button>
                    </div>
                    <div class="search-results-list">
                        {move || {
                            results.get().into_iter()
                                .enumerate()
                                .map(|(idx, result)| {
                                    let is_selected = selected_index.get() == idx;
                                    let result_id = result.id.clone();
                                    view! {
                                        <div
                                            class=format!(
                                                "search-result-item {}",
                                                if is_selected { "selected" } else { "" }
                                            )
                                            on:click=move |_| {
                                                on_result_click.run(result_id.clone());
                                                is_expanded.set(false);
                                            }
                                            on:mouseenter=move |_| selected_index.set(idx)
                                        >
                                            <div class="search-result-header">
                                                <span class="search-result-sender">{result.sender}</span>
                                                <span class="search-result-time">{result.timestamp}</span>
                                            </div>
                                            <div class="search-result-message">
                                                {result.highlighted_text}
                                            </div>
                                            <div class="search-result-channel">
                                                {move || format!("{} {}", i18n_stored.get_value().t("search-in"), result.channel)}
                                            </div>
                                        </div>
                                    }
                                })
                                .collect::<Vec<_>>()
                        }}
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// Advanced search filters
#[derive(Clone, Debug, Default)]
pub struct SearchFilters {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub sender: Option<String>,
    pub channel: Option<String>,
    pub has_attachments: bool,
}

/// Advanced Chat Search with filters
#[component]
pub fn AdvancedChatSearch(
    #[prop(into)] on_search: Callback<(String, SearchFilters)>,
) -> impl IntoView {
    let search_query = RwSignal::new(String::new());
    let show_filters = RwSignal::new(false);
    let filters = RwSignal::new(SearchFilters::default());
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="advanced-chat-search">
            <div class="search-input-row">
                <input
                    type="text"
                    class="search-input"
                    placeholder=move || i18n_stored.get_value().t("search-messages-placeholder")
                    prop:value=search_query
                    on:input=move |e| search_query.set(event_target_value(&e))
                />
                <button
                    class="btn btn-secondary"
                    on:click=move |_| show_filters.update(|v| *v = !*v)
                >
                    {move || i18n_stored.get_value().t("action-filter")}
                </button>
                <button
                    class="btn btn-primary"
                    on:click=move |_| {
                        on_search.run((search_query.get(), filters.get()));
                    }
                >
                    {move || i18n_stored.get_value().t("action-search")}
                </button>
            </div>

            <Show when=move || show_filters.get()>
                <div class="search-filters">
                    <div class="filter-row">
                        <div class="filter-group">
                            <label>{move || i18n_stored.get_value().t("filter-from-date")}</label>
                            <input
                                type="date"
                                on:input=move |e| {
                                    filters.update(|f| f.date_from = Some(event_target_value(&e)));
                                }
                            />
                        </div>
                        <div class="filter-group">
                            <label>{move || i18n_stored.get_value().t("filter-to-date")}</label>
                            <input
                                type="date"
                                on:input=move |e| {
                                    filters.update(|f| f.date_to = Some(event_target_value(&e)));
                                }
                            />
                        </div>
                    </div>
                    <div class="filter-row">
                        <div class="filter-group">
                            <label>{move || i18n_stored.get_value().t("filter-sender")}</label>
                            <input
                                type="text"
                                placeholder=move || i18n_stored.get_value().t("filter-sender-placeholder")
                                on:input=move |e| {
                                    filters.update(|f| f.sender = Some(event_target_value(&e)));
                                }
                            />
                        </div>
                        <div class="filter-group">
                            <label>{move || i18n_stored.get_value().t("filter-channel")}</label>
                            <input
                                type="text"
                                placeholder=move || i18n_stored.get_value().t("filter-channel-placeholder")
                                on:input=move |e| {
                                    filters.update(|f| f.channel = Some(event_target_value(&e)));
                                }
                            />
                        </div>
                    </div>
                    <div class="filter-row">
                        <label class="checkbox-label">
                            <input
                                type="checkbox"
                                on:change=move |e| {
                                    let checked = event_target_checked(&e);
                                    filters.update(|f| f.has_attachments = checked);
                                }
                            />
                            {move || i18n_stored.get_value().t("filter-has-attachments")}
                        </label>
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// Message export dialog
#[component]
pub fn MessageExportDialog(
    #[prop(into)] on_export: Callback<ExportOptions>,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let export_format = RwSignal::new("json".to_string());
    let include_attachments = RwSignal::new(true);
    let date_range = RwSignal::new("all".to_string());
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="modal-overlay" on:click=move |_| on_close.run(())>
            <div class="modal" on:click=|ev| ev.stop_propagation()>
                <div class="modal-header">
                    <h2>{move || i18n_stored.get_value().t("export-messages-title")}</h2>
                    <button class="btn btn-icon" on:click=move |_| on_close.run(())>
                        "✕"
                    </button>
                </div>

                <div class="modal-body">
                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("export-format")}</label>
                        <select
                            prop:value=export_format
                            on:change=move |e| export_format.set(event_target_value(&e))
                        >
                            <option value="json">"JSON"</option>
                            <option value="csv">"CSV"</option>
                            <option value="txt">{move || i18n_stored.get_value().t("format-plain-text")}</option>
                            <option value="pdf">"PDF"</option>
                        </select>
                    </div>

                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("date-range")}</label>
                        <select
                            prop:value=date_range
                            on:change=move |e| date_range.set(event_target_value(&e))
                        >
                            <option value="all">{move || i18n_stored.get_value().t("range-all-messages")}</option>
                            <option value="today">{move || i18n_stored.get_value().t("range-today")}</option>
                            <option value="week">{move || i18n_stored.get_value().t("range-last-7-days")}</option>
                            <option value="month">{move || i18n_stored.get_value().t("range-last-30-days")}</option>
                            <option value="custom">{move || i18n_stored.get_value().t("range-custom")}</option>
                        </select>
                    </div>

                    <div class="form-group">
                        <label class="checkbox-label">
                            <input
                                type="checkbox"
                                prop:checked=include_attachments
                                on:change=move |e| {
                                    include_attachments.set(event_target_checked(&e));
                                }
                            />
                            {move || i18n_stored.get_value().t("include-attachments")}
                        </label>
                    </div>
                </div>

                <div class="modal-actions">
                    <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                        {move || i18n_stored.get_value().t("action-cancel")}
                    </button>
                    <button
                        class="btn btn-primary"
                        on:click=move |_| {
                            on_export.run(ExportOptions {
                                format: export_format.get(),
                                include_attachments: include_attachments.get(),
                                date_range: date_range.get(),
                            });
                        }
                    >
                        {move || i18n_stored.get_value().t("action-export")}
                    </button>
                </div>
            </div>
        </div>
    }
}

/// Export options
#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub format: String,
    pub include_attachments: bool,
    pub date_range: String,
}

/// Pinned messages panel
#[component]
pub fn PinnedMessagesPanel(
    #[prop(into)] messages: Signal<Vec<PinnedMessage>>,
    #[prop(into)] on_unpin: Callback<String>,
    #[prop(into)] on_jump: Callback<String>,
) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="pinned-messages-panel">
            <div class="pinned-header">
                <h3>{move || format!("📌 {}", i18n_stored.get_value().t("pinned-messages"))}</h3>
            </div>
            <div class="pinned-list">
                {move || {
                    messages.get().into_iter()
                        .map(|msg| {
                            let msg_id = msg.id.clone();
                            let msg_id_unpin = msg.id.clone();
                            view! {
                                <div class="pinned-message">
                                    <div class="pinned-content">
                                        <div class="pinned-author">{msg.author}</div>
                                        <div class="pinned-text">{msg.content}</div>
                                        <div class="pinned-time">{msg.timestamp}</div>
                                    </div>
                                    <div class="pinned-actions">
                                        <button
                                            class="btn btn-sm btn-ghost"
                                            on:click=move |_| on_jump.run(msg_id.clone())
                                            title=move || i18n_stored.get_value().t("jump-to-message")
                                        >
                                            "➡️"
                                        </button>
                                        <button
                                            class="btn btn-sm btn-ghost"
                                            on:click=move |_| on_unpin.run(msg_id_unpin.clone())
                                            title=move || i18n_stored.get_value().t("unpin-message")
                                        >
                                            "📌"
                                        </button>
                                    </div>
                                </div>
                            }
                        })
                        .collect::<Vec<_>>()
                }}
            </div>
        </div>
    }
}

/// Pinned message
#[derive(Clone, Debug)]
pub struct PinnedMessage {
    pub id: String,
    pub content: String,
    pub author: String,
    pub timestamp: String,
}

/// Slash command suggestion
#[derive(Clone, Debug)]
pub struct SlashCommand {
    pub command: String,
    pub description: String,
    pub args: Vec<String>,
}

/// Slash command input
#[component]
pub fn SlashCommandInput(
    commands: Vec<SlashCommand>,
    #[prop(into)] on_command: Callback<String>,
) -> impl IntoView {
    let input_value = RwSignal::new(String::new());
    let show_suggestions = RwSignal::new(false);
    let selected_suggestion = RwSignal::new(0);
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    // Filter commands based on input
    let commands = StoredValue::new(commands);
    let filtered_commands = move || {
        let input = input_value.get();
        if input.starts_with('/') {
            let query = input[1..].to_lowercase();
            commands
                .get_value()
                .iter()
                .filter(|cmd| cmd.command.to_lowercase().starts_with(&query))
                .cloned()
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    };

    view! {
        <div class="slash-command-input">
            <input
                type="text"
                class="chat-input"
                placeholder=move || i18n_stored.get_value().t("slash-command-placeholder")
                prop:value=input_value
                on:input=move |e| {
                    let value = event_target_value(&e);
                    input_value.set(value.clone());
                    show_suggestions.set(value.starts_with('/'));
                    selected_suggestion.set(0);
                }
                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    match ev.key().as_str() {
                        "ArrowDown" => {
                            ev.prevent_default();
                            selected_suggestion.update(|i| {
                                let max = filtered_commands().len().saturating_sub(1);
                                *i = (*i + 1).min(max);
                            });
                        }
                        "ArrowUp" => {
                            ev.prevent_default();
                            selected_suggestion.update(|i| *i = i.saturating_sub(1));
                        }
                        "Enter" => {
                            ev.prevent_default();
                            let cmds = filtered_commands();
                            if let Some(cmd) = cmds.get(selected_suggestion.get()) {
                                on_command.run(format!("/{} ", cmd.command));
                                input_value.set(format!("/{} ", cmd.command));
                                show_suggestions.set(false);
                            } else {
                                on_command.run(input_value.get());
                                input_value.set(String::new());
                            }
                        }
                        "Escape" => {
                            show_suggestions.set(false);
                        }
                        _ => {}
                    }
                }
            />

            <Show when=move || show_suggestions.get() && !filtered_commands().is_empty()>
                <div class="slash-suggestions">
                    {move || {
                        filtered_commands().into_iter()
                            .enumerate()
                            .map(|(idx, cmd)| {
                                let is_selected = selected_suggestion.get() == idx;
                                let cmd_str = cmd.command.clone();
                                view! {
                                    <div
                                        class=format!(
                                            "slash-suggestion {}",
                                            if is_selected { "selected" } else { "" }
                                        )
                                        on:click=move |_| {
                                            on_command.run(format!("/{} ", cmd_str));
                                            input_value.set(format!("/{} ", cmd_str));
                                            show_suggestions.set(false);
                                        }
                                        on:mouseenter=move |_| selected_suggestion.set(idx)
                                    >
                                        <span class="slash-cmd">{format!("/ {}", cmd.command)}</span>
                                        <span class="slash-desc">{cmd.description}</span>
                                    </div>
                                }
                            })
                            .collect::<Vec<_>>()
                    }}
                </div>
            </Show>
        </div>
    }
}
