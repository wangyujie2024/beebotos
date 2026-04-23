//! Reusable Modal component

use leptos::prelude::*;

/// A generic modal dialog with overlay, click-outside-to-close, and header
#[component]
pub fn Modal(
    #[prop(into)] title: String,
    #[prop(into)] on_close: Callback<()>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="modal-overlay" on:click=move |_| on_close.run(())>
            <div class="modal" on:click=move |e| e.stop_propagation()>
                <div class="modal-header">
                    <h3>{title}</h3>
                    <button class="close-btn" on:click=move |_| on_close.run(())>
                        "✕"
                    </button>
                </div>
                {children()}
            </div>
        </div>
    }
}
