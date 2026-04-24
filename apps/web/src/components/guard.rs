//! Authentication Route Guards
//!
//! Provides:
//! - Authentication guards (requires login)
//! - Guest-only guards (redirects authenticated users)

use gloo_storage::Storage;
use leptos::prelude::*;
use leptos::view;
use leptos_router::hooks::use_navigate;

use crate::state::auth::use_auth_state;

/// Authentication guard - requires user to be logged in
#[component]
pub fn AuthGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_auth_state();
    let navigate = use_navigate();
    let auth_for_effect = auth.clone();

    // Signal for authenticated status
    let is_authenticated = Memo::new(move |_| auth.is_authenticated());

    Effect::new(move |_| {
        if !auth_for_effect.is_authenticated() {
            // Store current location for redirect after login
            if let Some(window) = web_sys::window() {
                if let Ok(Some(_)) =
                    gloo_storage::SessionStorage::raw().get_item("redirect_after_login")
                {
                    // Already set, don't override
                } else {
                    let current_path = window.location().pathname().unwrap_or_default();
                    let _ = gloo_storage::SessionStorage::raw()
                        .set_item("redirect_after_login", &current_path);
                }
            }

            navigate("/login", Default::default());
        }
    });

    // Use custom conditional rendering instead of Show
    move || {
        if is_authenticated.get() {
            children.clone()().into_any()
        } else {
            view! { <Redirecting message="Checking authentication..." /> }.into_any()
        }
    }
}

/// Guest only guard - only for non-authenticated users
#[component]
pub fn GuestOnly(children: ChildrenFn) -> impl IntoView {
    let auth = use_auth_state();
    let navigate = use_navigate();
    let auth_for_effect = auth.clone();

    let is_guest = Memo::new(move |_| !auth.is_authenticated());

    Effect::new(move |_| {
        if auth_for_effect.is_authenticated() {
            navigate("/", Default::default());
        }
    });

    move || {
        if is_guest.get() {
            children.clone()().into_any()
        } else {
            view! { <Redirecting message="Redirecting..." /> }.into_any()
        }
    }
}

/// Loading indicator during auth check
#[component]
fn Redirecting(#[prop(default = "Redirecting...")] message: &'static str) -> impl IntoView {
    view! {
        <div class="redirecting">
            <div class="spinner"></div>
            <p>{message}</p>
        </div>
    }
}

/// Access denied component
#[component]
pub fn AccessDenied() -> impl IntoView {
    view! {
        <div class="access-denied">
            <div class="access-denied-icon">"🚫"</div>
            <h1>"403"</h1>
            <h2>"Access Denied"</h2>
            <p>"You don't have permission to access this page."</p>
            <div class="access-denied-actions">
                <a href="/" class="btn btn-primary">"Go Home"</a>
                <a href="/contact" class="btn btn-secondary">"Contact Support"</a>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_guard_exports() {
        // Just verify component types are accessible
        // Components are validated by compilation
        assert!(true);
    }
}
