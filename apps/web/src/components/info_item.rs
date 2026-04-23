//! Reusable key-value info display component

use leptos::prelude::*;

/// Display a label-value pair in an info grid
#[component]
pub fn InfoItem(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    #[prop(default = "info-item")] class: &'static str,
) -> impl IntoView {
    view! {
        <div class=class>
            <span class="info-label">{label}</span>
            <span class="info-value">{value}</span>
        </div>
    }
}
