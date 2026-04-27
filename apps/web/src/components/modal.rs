//! Reusable Modal component

use crate::i18n::I18nContext;
use leptos::prelude::*;

/// A generic modal dialog with overlay, click-outside-to-close, and header
#[component]
pub fn Modal(
    #[prop(into)] title: String,
    #[prop(into)] on_close: Callback<()>,
    children: Children,
) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="modal-overlay" on:click=move |_| on_close.run(())>
            <div class="modal" on:click=move |e| e.stop_propagation()>
                <div class="modal-header">
                    <h3>{title}</h3>
                    <button
                        class="close-btn"
                        on:click=move |_| on_close.run(())
                        title=move || i18n_stored.get_value().t("action-close")
                    >
                        "✕"
                    </button>
                </div>
                {children()}
            </div>
        </div>
    }
}
