use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::i18n::I18nContext;

#[component]
pub fn TopBar() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);
    let location = use_location();

    // Get current page title based on path
    let page_title = Signal::derive(move || {
        let path = location.pathname.get();
        let i18n = i18n_stored.get_value();

        if path.starts_with("/agents") {
            i18n.t("nav-agents")
        } else if path.starts_with("/dao/treasury") {
            i18n.t("nav-treasury")
        } else if path.starts_with("/dao") {
            i18n.t("nav-dao")
        } else if path.starts_with("/skills") {
            i18n.t("nav-skills")
        } else if path.starts_with("/settings") {
            i18n.t("nav-settings")
        } else if path.starts_with("/browser") {
            i18n.t("nav-browser")
        } else if path.starts_with("/chat") {
            i18n.t("nav-chat")
        } else {
            i18n.t("nav-home")
        }
    });

    view! {
        <header class="top-bar">
            <div class="top-bar-left">
                <button class="mobile-menu-btn" on:click=move |_| {
                    // Toggle sidebar on mobile
                    let sidebar = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.query_selector(".sidebar").ok())
                        .flatten();
                    if let Some(el) = sidebar {
                        let class_list = el.class_list();
                        let _ = class_list.toggle("open");
                    }
                }>
                    "☰"
                </button>
                <h1 class="page-title">{move || page_title.get()}</h1>
            </div>
            <div class="top-bar-right">
                <button class="icon-btn" title="Notifications">
                    "🔔"
                </button>
                <button class="icon-btn" title="Settings" on:click=move |_| {
                    let navigate = leptos_router::hooks::use_navigate();
                    navigate("/settings", Default::default());
                }>
                    "⚙️"
                </button>
            </div>
        </header>
    }
}
