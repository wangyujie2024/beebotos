use crate::i18n::{I18nContext, Locale};
use crate::state::use_app_state;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_location, use_navigate};

#[component]
pub fn Sidebar() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);
    let location = use_location();
    let app_state = use_app_state();

    // Get current path
    let current_path = Signal::derive(move || {
        location.pathname.get()
    });

    // Collapsible group states
    let chat_collapsed = RwSignal::new(false);
    let control_collapsed = RwSignal::new(false);
    let agent_collapsed = RwSignal::new(false);
    let settings_collapsed = RwSignal::new(false);

    view! {
        <aside class="sidebar">
            // Logo Section
            <div class="sidebar-header">
                <A href="/" attr:class="logo">
                    <div class="logo-icon">"🐝"</div>
                    <span class="logo-text">"BeeBotOS"</span>
                </A>
            </div>

            // Navigation Menu
            <nav class="sidebar-nav">
                // Chat Group
                <div class="nav-group" class:collapsed=move || chat_collapsed.get()>
                    <div class="nav-group-header" on:click=move |_| chat_collapsed.update(|v| *v = !*v)>
                        <span class="group-icon">"💬"</span>
                        <span class="group-title">{move || i18n_stored.get_value().t("nav-section-chat")}</span>
                        <span class="group-arrow">"▼"</span>
                    </div>
                    <div class="nav-group-items">
                        <NavItem
                            href="/chat"
                            icon="💬"
                            label=move || i18n_stored.get_value().t("nav-chat")
                            current_path=current_path
                        />
                        <NavItem
                            href="/browser"
                            icon="🌐"
                            label=move || i18n_stored.get_value().t("nav-browser")
                            current_path=current_path
                        />
                    </div>
                </div>

                // Control Group
                <div class="nav-group" class:collapsed=move || control_collapsed.get()>
                    <div class="nav-group-header" on:click=move |_| control_collapsed.update(|v| *v = !*v)>
                        <span class="group-icon">"🎛️"</span>
                        <span class="group-title">{move || i18n_stored.get_value().t("nav-section-control")}</span>
                        <span class="group-arrow">"▼"</span>
                    </div>
                    <div class="nav-group-items">
                        <NavItem
                            href="/channels"
                            icon="📡"
                            label=move || i18n_stored.get_value().t("nav-channels")
                            current_path=current_path
                        />
                        <NavItem
                            href="/dao"
                            icon="🏛️"
                            label=move || i18n_stored.get_value().t("nav-dao")
                            current_path=current_path
                        />
                        <NavItem
                            href="/dao/treasury"
                            icon="💰"
                            label=move || i18n_stored.get_value().t("nav-treasury")
                            current_path=current_path
                        />
                        <NavItem
                            href="/skills"
                            icon="🔌"
                            label=move || i18n_stored.get_value().t("nav-skills")
                            current_path=current_path
                        />
                        <NavItem
                            href="/skill-instances"
                            icon="🤖"
                            label=move || i18n_stored.get_value().t("nav-skill-instances")
                            current_path=current_path
                        />
                    </div>
                </div>

                // Agents Group
                <div class="nav-group" class:collapsed=move || agent_collapsed.get()>
                    <div class="nav-group-header" on:click=move |_| agent_collapsed.update(|v| *v = !*v)>
                        <span class="group-icon">"🤖"</span>
                        <span class="group-title">{move || i18n_stored.get_value().t("nav-section-agents")}</span>
                        <span class="group-arrow">"▼"</span>
                    </div>
                    <div class="nav-group-items">
                        <NavItem
                            href="/agents"
                            icon="🤖"
                            label=move || i18n_stored.get_value().t("nav-agents")
                            current_path=current_path
                        />
                    </div>
                </div>

                // Settings Group
                <div class="nav-group" class:collapsed=move || settings_collapsed.get()>
                    <div class="nav-group-header" on:click=move |_| settings_collapsed.update(|v| *v = !*v)>
                        <span class="group-icon">"⚙️"</span>
                        <span class="group-title">{move || i18n_stored.get_value().t("nav-section-settings")}</span>
                        <span class="group-arrow">"▼"</span>
                    </div>
                    <div class="nav-group-items">
                        <NavItem
                            href="/models"
                            icon="🧠"
                            label=move || i18n_stored.get_value().t("nav-models")
                            current_path=current_path
                        />
                        <NavItem
                            href="/settings"
                            icon="⚙️"
                            label=move || i18n_stored.get_value().t("nav-settings")
                            current_path=current_path
                        />
                    </div>
                </div>
            </nav>

            // Sidebar Footer
            <div class="sidebar-footer">
                <UserMenu app_state=app_state i18n=i18n_stored.get_value() />
                <LanguageToggle i18n=i18n_stored.get_value() />
            </div>
        </aside>
    }
}

#[component]
fn NavItem(
    href: &'static str,
    icon: &'static str,
    #[prop(into)]
    label: Signal<String>,
    current_path: Signal<String>,
) -> impl IntoView {
    let is_active = Signal::derive(move || {
        let path = current_path.get();
        if href == "/" {
            path == "/"
        } else {
            path.starts_with(href)
        }
    });

    let class_signal = Signal::derive(move || {
        if is_active.get() {
            "nav-item active"
        } else {
            "nav-item"
        }
    });

    view! {
        <A href={href} attr:class=class_signal>
            <span class="nav-item-icon">{icon}</span>
            <span>{move || label.get()}</span>
        </A>
    }
}

#[component]
fn LanguageToggle(i18n: I18nContext) -> impl IntoView {
    let i18n_for_label = i18n.clone();
    let locale_label = move || {
        match i18n_for_label.get_locale() {
            Locale::ZhCN => "🇨🇳 中文",
            _ => "🇺🇸 EN",
        }
    };

    view! {
        <button
            class="lang-toggle-sidebar"
            on:click=move |_| {
                let new_locale = match i18n.get_locale() {
                    Locale::ZhCN => Locale::En,
                    _ => Locale::ZhCN,
                };
                i18n.set_locale(new_locale);
            }
        >
            <span>{locale_label}</span>
            <span>"↻"</span>
        </button>
    }
}

#[component]
fn UserMenu(
    app_state: crate::state::AppState,
    i18n: I18nContext,
) -> impl IntoView {
    let app_state_stored = StoredValue::new(app_state);
    let i18n_stored = StoredValue::new(i18n);
    let navigate = use_navigate();

    let is_authenticated = Signal::derive(move || app_state_stored.get_value().is_authenticated());
    let user_name = Signal::derive(move || {
        app_state_stored
            .get_value()
            .user()
            .with(|u| u.as_ref().map(|u| u.name.clone()).unwrap_or_default())
    });

    view! {
        {move || if is_authenticated.get() {
            view! {
                <div class="user-menu">
                    <div class="user-info">
                        <div class="user-avatar">
                            {user_name.get().chars().next().unwrap_or('U').to_uppercase().to_string()}
                        </div>
                        <div class="user-details">
                            <span class="user-name">{user_name.get()}</span>
                            <span class="user-status">"Online"</span>
                        </div>
                    </div>
                    <button
                        class="btn btn-sm btn-secondary btn-block"
                        on:click={
                            let nav = navigate.clone();
                            move |_| {
                                app_state_stored.get_value().auth.logout();
                                nav("/login", Default::default());
                            }
                        }
                    >
                        {move || i18n_stored.get_value().t("action-logout")}
                    </button>
                </div>
            }.into_any()
        } else {
            view! {
                <A href="/login" attr:class="btn btn-primary btn-block">
                    {move || i18n_stored.get_value().t("action-login")}
                </A>
            }.into_any()
        }}
    }
}
