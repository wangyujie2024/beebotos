//! Command Palette Component
//!
//! Provides a quick command interface similar to VS Code's command palette
//! or OpenClaw's command panel.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::state::use_app_state;

/// Command palette item
#[derive(Clone, Debug)]
pub struct CommandItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub shortcut: Option<String>,
    pub action: CommandAction,
}

/// Command action type
#[derive(Clone, Debug)]
pub enum CommandAction {
    Navigate(String),
    Action(fn()),
    AsyncAction(fn() -> Pin<Box<dyn Future<Output = ()>>>),
}

use std::future::Future;
use std::pin::Pin;

/// Command Palette Component
#[component]
pub fn CommandPalette() -> impl IntoView {
    let _app_state = use_app_state();
    let is_open = RwSignal::new(false);
    let search_query = RwSignal::new(String::new());
    let selected_index = RwSignal::new(0);
    let navigate = use_navigate();

    // Define available commands
    let commands = vec![
        CommandItem {
            id: "nav-agents".to_string(),
            title: "Go to Agents".to_string(),
            description: "View and manage your agents".to_string(),
            icon: "🤖".to_string(),
            shortcut: Some("Ctrl+Shift+A".to_string()),
            action: CommandAction::Navigate("/agents".to_string()),
        },
        CommandItem {
            id: "nav-dao".to_string(),
            title: "Go to DAO".to_string(),
            description: "View proposals and governance".to_string(),
            icon: "🏛️".to_string(),
            shortcut: Some("Ctrl+Shift+D".to_string()),
            action: CommandAction::Navigate("/dao".to_string()),
        },
        CommandItem {
            id: "nav-skills".to_string(),
            title: "Go to Skills".to_string(),
            description: "Browse and install skills".to_string(),
            icon: "🔌".to_string(),
            shortcut: Some("Ctrl+Shift+S".to_string()),
            action: CommandAction::Navigate("/skills".to_string()),
        },
        CommandItem {
            id: "nav-treasury".to_string(),
            title: "Go to Treasury".to_string(),
            description: "Manage treasury and budgets".to_string(),
            icon: "💰".to_string(),
            shortcut: Some("Ctrl+Shift+T".to_string()),
            action: CommandAction::Navigate("/treasury".to_string()),
        },
        CommandItem {
            id: "nav-settings".to_string(),
            title: "Go to Settings".to_string(),
            description: "Configure your preferences".to_string(),
            icon: "⚙️".to_string(),
            shortcut: Some("Ctrl+,".to_string()),
            action: CommandAction::Navigate("/settings".to_string()),
        },
        CommandItem {
            id: "agent-create".to_string(),
            title: "Create New Agent".to_string(),
            description: "Create a new autonomous agent".to_string(),
            icon: "➕".to_string(),
            shortcut: Some("Ctrl+N".to_string()),
            action: CommandAction::Navigate("/agents".to_string()),
        },
        CommandItem {
            id: "toggle-theme".to_string(),
            title: "Toggle Theme".to_string(),
            description: "Switch between light and dark mode".to_string(),
            icon: "🌓".to_string(),
            shortcut: Some("Ctrl+Shift+L".to_string()),
            action: CommandAction::Action(|| {
                // Toggle theme logic
                let window = web_sys::window().unwrap();
                let document = window.document().unwrap();
                let body = document.body().unwrap();

                let current_class = body.class_name();
                if current_class.contains("dark-theme") {
                    body.set_class_name(&current_class.replace("dark-theme", "light-theme"));
                } else {
                    body.set_class_name(&current_class.replace("light-theme", "dark-theme"));
                }
            }),
        },
    ];

    // Store commands in a stable reference
    let commands = StoredValue::new(commands);

    // Filter commands based on search query
    let filtered_commands = move || {
        let query = search_query.get().to_lowercase();
        let cmds = commands.get_value();
        if query.is_empty() {
            cmds
        } else {
            cmds.iter()
                .filter(|cmd| {
                    cmd.title.to_lowercase().contains(&query)
                        || cmd.description.to_lowercase().contains(&query)
                })
                .cloned()
                .collect::<Vec<_>>()
        }
    };

    // Handle keyboard shortcut (Ctrl+K or Cmd+K)
    let handle_keydown = move |ev: web_sys::KeyboardEvent| {
        let ctrl = ev.ctrl_key();
        let meta = ev.meta_key();
        let k_key = ev.key() == "k";

        if (ctrl || meta) && k_key {
            ev.prevent_default();
            is_open.update(|v| *v = !*v);
        }

        // Close on Escape
        if ev.key() == "Escape" && is_open.get() {
            is_open.set(false);
        }
    };

    // Execute command - stored to allow multiple uses
    let execute_command = StoredValue::new(move |cmd: CommandItem| {
        match cmd.action {
            CommandAction::Navigate(path) => {
                navigate(&path, Default::default());
            }
            CommandAction::Action(f) => f(),
            CommandAction::AsyncAction(_) => {
                // Handle async action
            }
        }
        is_open.set(false);
        search_query.set(String::new());
    });

    view! {
        <div>
            // Keyboard shortcut listener
            <div
                class="command-palette-shortcut-hint"
                on:keydown=handle_keydown
            >
                <span class="hint">"Press Ctrl+K for commands"</span>
            </div>

            <Show when=move || is_open.get()>
                <div
                    class="command-palette-overlay"
                    on:click=move |_| is_open.set(false)
                >
                    <div
                        class="command-palette"
                        on:click=|ev| ev.stop_propagation()
                    >
                        <div class="command-palette-header">
                            <input
                                type="text"
                                class="command-palette-input"
                                placeholder="Type a command or search..."
                                prop:value=search_query
                                on:input=move |e| {
                                    search_query.set(event_target_value(&e));
                                    selected_index.set(0);
                                }
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    match ev.key().as_str() {
                                        "ArrowDown" => {
                                            ev.prevent_default();
                                            selected_index.update(|i| {
                                                let max = filtered_commands().len().saturating_sub(1);
                                                *i = (*i + 1).min(max);
                                            });
                                        }
                                        "ArrowUp" => {
                                            ev.prevent_default();
                                            selected_index.update(|i| *i = i.saturating_sub(1));
                                        }
                                        "Enter" => {
                                            ev.prevent_default();
                                            let cmds = filtered_commands();
                                            if let Some(cmd) = cmds.get(selected_index.get()) {
                                                execute_command.get_value()(cmd.clone());
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            />
                        </div>

                        <div class="command-palette-list">
                            {move || {
                                let cmds = filtered_commands();
                                if cmds.is_empty() {
                                    view! {
                                        <div class="command-palette-empty">
                                            "No commands found"
                                        </div>
                                    }.into_any()
                                } else {
                                    cmds.into_iter()
                                        .enumerate()
                                        .map(|(idx, cmd)| {
                                            let is_selected = selected_index.get() == idx;
                                            let cmd_clone = cmd.clone();
                                            view! {
                                                <div
                                                    class=format!(
                                                        "command-palette-item {}",
                                                        if is_selected { "selected" } else { "" }
                                                    )
                                                    on:click=move |_| execute_command.get_value()(cmd_clone.clone())
                                                    on:mouseenter=move |_| selected_index.set(idx)
                                                >
                                                    <span class="command-icon">{cmd.icon.clone()}</span>
                                                    <div class="command-info">
                                                        <div class="command-title">{cmd.title.clone()}</div>
                                                        <div class="command-description">{cmd.description.clone()}</div>
                                                    </div>
                                                    {cmd.shortcut.clone().map(|s| view! {
                                                        <span class="command-shortcut">{s}</span>
                                                    })}
                                                </div>
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                        .into_any()
                                }
                            }}
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// Command palette trigger button for mobile
#[component]
pub fn CommandPaletteButton() -> impl IntoView {
    let is_open = RwSignal::new(false);

    view! {
        <button
            class="btn btn-icon command-palette-btn"
            on:click=move |_| is_open.update(|v| *v = !*v)
            title="Open Command Palette"
        >
            "⌘"
        </button>
    }
}
