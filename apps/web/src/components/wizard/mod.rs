//! Gateway Configuration Wizard Components

use crate::state::wizard::*;
use crate::utils::{event_target_checked, event_target_value};
use leptos::prelude::*;
use leptos::view;

/// Stepper showing current progress through wizard steps
#[component]
pub fn WizardStepper(
    current_step: RwSignal<usize>,
    #[prop(default = 10)] total_steps: usize,
) -> impl IntoView {
    let steps = vec![
        (1, "Welcome"),
        (2, "Server"),
        (3, "Database"),
        (4, "Security"),
        (5, "LLM Models"),
        (6, "Channels"),
        (7, "Blockchain"),
        (8, "Logging"),
        (9, "Review"),
        (10, "Deploy"),
    ];

    view! {
        <div class="wizard-stepper">
            <div class="stepper-track">
                {steps.into_iter().take(total_steps).map(|(step, label)| {
                    let is_active = move || current_step.get() == step;
                    let is_completed = move || current_step.get() > step;
                    let step_clone = step;
                    view! {
                        <div
                            class=move || format!("step {}",
                                if is_active() { "active" }
                                else if is_completed() { "completed" }
                                else { "" }
                            )
                            on:click=move |_| current_step.set(step_clone)
                        >
                            <div class="step-number">
                                {move || if is_completed() {
                                    "✓".into_any()
                                } else {
                                    step_clone.to_string().into_any()
                                }}
                            </div>
                            <div class="step-label">{label}</div>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}

/// Navigation buttons for wizard
#[component]
pub fn WizardNavigation(
    current_step: RwSignal<usize>,
    #[prop(default = 10)] total_steps: usize,
    can_proceed: Signal<bool>,
    #[prop(into)] on_back: Callback<()>,
    #[prop(into)] on_next: Callback<()>,
    #[prop(into)] on_finish: Callback<()>,
    is_submitting: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="wizard-navigation">
            <button
                class="btn btn-secondary"
                disabled=move || current_step.get() == 1
                on:click=move |_| on_back.run(())
            >
                "← Back"
            </button>

            <div class="step-indicator">
                {move || format!("Step {} of {}", current_step.get(), total_steps)}
            </div>

            {move || if current_step.get() == total_steps {
                view! {
                    <button
                        class="btn btn-primary"
                        disabled=is_submitting
                        on:click=move |_| on_finish.run(())
                    >
                        {move || if is_submitting.get() {
                            "Deploying...".into_any()
                        } else {
                            "Save & Export ▼".into_any()
                        }}
                    </button>
                }.into_any()
            } else {
                view! {
                    <button
                        class="btn btn-primary"
                        disabled=move || !can_proceed.get()
                        on:click=move |_| on_next.run(())
                    >
                        "Next →"
                    </button>
                }.into_any()
            }}
        </div>
    }
}

/// Secret input with visibility toggle and generate button
#[component]
pub fn SecretInput(
    value: RwSignal<String>,
    #[prop(optional)] placeholder: Option<String>,
    #[prop(optional, into)] on_generate: Option<Callback<()>>,
) -> impl IntoView {
    let show_password = RwSignal::new(false);
    let placeholder = placeholder.unwrap_or_else(|| "Enter secret...".to_string());

    view! {
        <div class="secret-input-wrapper">
            <input
                type=move || if show_password.get() { "text" } else { "password" }
                prop:value=value
                placeholder=placeholder.clone()
                on:input=move |e| value.set(event_target_value(&e))
            />
            <button
                type="button"
                class="btn btn-icon secret-toggle"
                on:click=move |_| show_password.update(|v| *v = !*v)
                title=move || if show_password.get() { "Hide" } else { "Show" }
            >
                {move || if show_password.get() { "🙈" } else { "👁" }}
            </button>
            {on_generate.map(|gen| {
                view! {
                    <button
                        type="button"
                        class="btn btn-icon secret-generate"
                        on:click=move |_| gen.run(())
                        title="Generate"
                    >
                        "🎲"
                    </button>
                }
            })}
        </div>
    }
}

/// Provider card for LLM configuration
#[component]
pub fn ProviderCard(
    #[prop(into)] provider: ProviderDraft,
    on_remove: impl Fn() + 'static,
) -> impl IntoView {
    view! {
        <div class="provider-card-config">
            <div class="provider-card-header">
                <h4>{provider.name.clone()}</h4>
                <button class="btn btn-icon btn-danger" on:click=move |_| on_remove()>
                    "🗑"
                </button>
            </div>
            <div class="form-group">
                <label>"API Key"</label>
                <input type="password" value=provider.api_key readonly />
            </div>
            <div class="form-group">
                <label>"Model"</label>
                <input type="text" value=provider.model readonly />
            </div>
            <div class="form-group">
                <label>"Base URL"</label>
                <input type="text" value=provider.base_url readonly />
            </div>
            <div class="form-row">
                <div class="form-group">
                    <label>"Temperature"</label>
                    <input type="text" value=format!("{:.1}", provider.temperature) readonly />
                </div>
                <div class="form-group">
                    <label>"Context Window"</label>
                    <input type="text" value=provider.context_window.map(|c| c.to_string()).unwrap_or_else(|| "Default".to_string()) readonly />
                </div>
            </div>
        </div>
    }
}

/// Configuration preview with tabbed TOML/ENV view
#[component]
pub fn ConfigPreview(
    toml_content: Signal<String>,
    env_content: Signal<String>,
) -> impl IntoView {
    let active_tab = RwSignal::new("toml".to_string());

    view! {
        <div class="config-preview">
            <div class="preview-tabs">
                <button
                    class=move || format!("tab {}", if active_tab.get() == "toml" { "active" } else { "" })
                    on:click=move |_| active_tab.set("toml".to_string())
                >
                    "TOML"
                </button>
                <button
                    class=move || format!("tab {}", if active_tab.get() == "env" { "active" } else { "" })
                    on:click=move |_| active_tab.set("env".to_string())
                >
                    "ENV"
                </button>
            </div>
            <div class="preview-content">
                {move || if active_tab.get() == "toml" {
                    view! {
                        <pre class="code-preview">{toml_content.get()}</pre>
                    }.into_any()
                } else {
                    view! {
                        <pre class="code-preview">{env_content.get()}</pre>
                    }.into_any()
                }}
            </div>
        </div>
    }
}

/// Channel platform configuration form
#[component]
pub fn ChannelPlatformForm(
    platform: RwSignal<PlatformDraft>,
) -> impl IntoView {
    let name = platform.get().name.clone();
    let enabled = RwSignal::new(platform.get().enabled);

    view! {
        <div class="platform-form">
            <div class="platform-header">
                <label class="checkbox-label">
                    <input
                        type="checkbox"
                        prop:checked=enabled
                        on:change=move |e| {
                            let checked = event_target_checked(&e);
                            enabled.set(checked);
                            platform.update(|p| p.enabled = checked);
                        }
                    />
                    <span class="platform-name">{name.clone()}</span>
                </label>
                <span class=move || format!("status-badge {}", if enabled.get() { "enabled" } else { "disabled" })>
                    {move || if enabled.get() { "Enabled" } else { "Disabled" }}
                </span>
            </div>
            {move || if enabled.get() {
                view! {
                    <div class="platform-settings">
                        <p class="form-help">{format!("Configure {} settings in beebotos.toml", name)}</p>
                    </div>
                }.into_any()
            } else {
                ().into_any()
            }}
        </div>
    }
}


