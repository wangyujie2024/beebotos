use crate::i18n::{I18nContext, Locale};
use crate::state::use_app_state;
use crate::utils::ThemeToggle;
use gloo_storage::Storage;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

#[component]
pub fn Nav() -> impl IntoView {
    let _app_state = use_app_state();
    let is_menu_open = RwSignal::new(false);
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <nav class="nav">
            <div class="nav-container">
                <A href="/" attr:class="nav-logo">
                    <span>"🐝"</span>
                    <span>"BeeBotOS"</span>
                </A>

                // Mobile menu toggle
                <button
                    class="nav-menu-toggle"
                    on:click=move |_| is_menu_open.update(|open| *open = !*open)
                >
                    {move || if is_menu_open.get() { "✕" } else { "☰" }}
                </button>

                // Desktop navigation
                <ul class="nav-links desktop-nav">
                    <li><A href="/agents">{move || i18n_stored.get_value().t("nav-agents")}</A></li>
                    <li><A href="/dao">{move || i18n_stored.get_value().t("nav-dao")}</A></li>
                    <li><A href="/skills">{move || i18n_stored.get_value().t("nav-skills")}</A></li>
                    <li><A href="/settings">{move || i18n_stored.get_value().t("nav-settings")}</A></li>
                </ul>

                <div class="nav-actions desktop-nav">
                    <LanguageToggle i18n=i18n_stored.get_value() />
                    <ThemeToggle />
                    <NavAuthSection />
                </div>
            </div>

            // Mobile navigation
            <Show when=move || is_menu_open.get()>
                <div class="mobile-nav">
                    <ul class="nav-links">
                        <li><A href="/agents" on:click=move |_| is_menu_open.set(false)>{move || i18n_stored.get_value().t("nav-agents")}</A></li>
                        <li><A href="/dao" on:click=move |_| is_menu_open.set(false)>{move || i18n_stored.get_value().t("nav-dao")}</A></li>
                        <li><A href="/skills" on:click=move |_| is_menu_open.set(false)>{move || i18n_stored.get_value().t("nav-skills")}</A></li>
                        <li><A href="/settings" on:click=move |_| is_menu_open.set(false)>{move || i18n_stored.get_value().t("nav-settings")}</A></li>
                    </ul>

                    <div class="nav-actions mobile-nav-actions">
                        <LanguageToggle i18n=i18n_stored.get_value() />
                        <ThemeToggle />
                        <NavAuthSection />
                    </div>
                </div>
            </Show>
        </nav>
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
            class="lang-toggle"
            on:click=move |_| {
                let new_locale = match i18n.get_locale() {
                    Locale::ZhCN => Locale::En,
                    _ => Locale::ZhCN,
                };
                i18n.set_locale(new_locale);
            }
            title="切换语言 / Switch Language"
        >
            {locale_label}
        </button>
    }
}

#[component]
fn NavAuthSection() -> impl IntoView {
    let app_state = use_app_state();
    let i18n = use_context::<I18nContext>().expect("i18n context not found");

    // Store in StoredValue to allow cloning without move issues
    let app_state_stored = StoredValue::new(app_state);
    let i18n_stored = StoredValue::new(i18n);

    let is_authenticated = Signal::derive(move || app_state_stored.get_value().is_authenticated());
    let user_name = Signal::derive(move || {
        app_state_stored
            .get_value()
            .user()
            .with(|u| u.as_ref().map(|u| u.name.clone()).unwrap_or_default())
    });
    let unread_count = Signal::derive(move || app_state_stored.get_value().unread_count());

    view! {
        {move || if is_authenticated.get() {
            view! {
                <div class="nav-user">
                    <button class="nav-notification-btn">
                        "🔔"
                        {move || {
                            let count = unread_count.get();
                            if count > 0 {
                                view! {
                                    <span class="notification-badge">{count}</span>
                                }.into_any()
                            } else {
                                view! { <></> }.into_any()
                            }
                        }}
                    </button>
                    <span class="user-name">
                        {user_name.get()}
                    </span>
                    <LogoutButton app_state={app_state_stored} i18n=i18n_stored.get_value() />
                </div>
            }.into_any()
        } else {
            view! {
                <div class="nav-auth-buttons">
                    <A href="/login" attr:class="btn btn-primary btn-sm">
                        {move || i18n_stored.get_value().t("action-login")}
                    </A>
                    <A href="/register" attr:class="btn btn-secondary btn-sm">
                        {move || i18n_stored.get_value().t("action-register")}
                    </A>
                </div>
            }.into_any()
        }}
    }
}

#[component]
fn LogoutButton(app_state: StoredValue<crate::state::AppState>, i18n: I18nContext) -> impl IntoView {
    // Get navigate inside the component to avoid Send issues
    let navigate = use_navigate();
    let i18n_label = i18n.clone();

    view! {
        <button
            class="btn btn-sm btn-secondary"
            on:click=move |_| {
                let app_state = app_state.get_value();
                let navigate = navigate.clone();
                let i18n = i18n.clone();
                spawn_local(async move {
                    // Call logout API
                    let auth_service = app_state.auth_service();
                    match auth_service.logout().await {
                        Ok(_) => {
                            // Clear local auth state
                            app_state.auth.logout();

                            // Clear any session storage items
                            let _ = gloo_storage::SessionStorage::raw().remove_item("redirect_after_login");

                            // Navigate to home page
                            navigate("/", Default::default());

                            // Show logout notification
                            app_state.notify(
                                crate::state::notification::NotificationType::Info,
                                &i18n.t("notification-info"),
                                &i18n.t("logout-success")
                            );
                        }
                        Err(e) => {
                            app_state.notify(
                                crate::state::notification::NotificationType::Error,
                                &i18n.t("notification-error"),
                                &format!("Logout failed: {}", e)
                            );
                        }
                    }
                });
            }
        >
            {move || i18n_label.t("action-logout")}
        </button>
    }
}
