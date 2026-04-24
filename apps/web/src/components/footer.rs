use leptos::prelude::*;
use leptos::view;

use crate::i18n::I18nContext;

#[component]
pub fn Footer() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <footer class="footer">
            <div class="footer-content">
                <div class="footer-left">
                    <div class="footer-logo">
                        <span>"🐝"</span>
                        <span>"BeeBotOS"</span>
                    </div>
                    <div class="footer-divider"></div>
                    <p class="footer-text">
                        {move || format!("{} | v{}", i18n_stored.get_value().t("footer-copyright"), crate::version())}
                    </p>
                </div>
                <div class="footer-badges">
                    <span class="badge badge-primary">"OpenClaw v2026.3.13"</span>
                    <span class="badge badge-secondary">"Web4.0 Ready"</span>
                </div>
            </div>
        </footer>
    }
}
