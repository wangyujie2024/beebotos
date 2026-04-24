//! Login Page

use gloo_storage::Storage;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::api::AuthService;
use crate::i18n::I18nContext;
use crate::state::{use_auth_state, User};

#[component]
pub fn LoginPage() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);
    let navigate = use_navigate();
    let auth_state = use_auth_state();

    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (is_loading, set_is_loading) = signal(false);

    // Real login handler
    let handle_submit = {
        let auth_state = auth_state.clone();
        let navigate = navigate.clone();
        move |_| {
            if username.get().is_empty() || password.get().is_empty() {
                set_error.set(Some(i18n_stored.get_value().t("login-error-empty")));
                return;
            }

            set_is_loading.set(true);
            set_error.set(None);

            let auth = auth_state.clone();
            let nav = navigate.clone();
            let i18n = i18n_stored.get_value();
            let user_name = username.get();
            let pass = password.get();

            spawn_local(async move {
                let client = crate::api::create_client();
                let auth_service = AuthService::new(client);

                match auth_service.login(&user_name, &pass).await {
                    Ok(response) => {
                        // Convert UserInfo to User
                        let user = User {
                            id: response.user.id,
                            name: response.user.name,
                            email: response.user.email,
                            avatar: response.user.avatar,
                            wallet_address: response.user.wallet_address,
                        };

                        // Set authenticated state
                        auth.set_authenticated(
                            user,
                            response.access_token,
                            response.refresh_token,
                            response.expires_in,
                        );

                        // Navigate to redirect target or home
                        let redirect_path = gloo_storage::SessionStorage::raw()
                            .get_item("redirect_after_login")
                            .ok()
                            .flatten()
                            .filter(|p| !p.is_empty() && p != "/login" && p != "/register")
                            .unwrap_or_else(|| "/".to_string());
                        let _ =
                            gloo_storage::SessionStorage::raw().remove_item("redirect_after_login");
                        nav(&redirect_path, Default::default());
                    }
                    Err(e) => {
                        set_error.set(Some(format!("{}: {}", i18n.t("login-error-failed"), e)));
                    }
                }

                set_is_loading.set(false);
            });
        }
    };

    view! {
        <div class="login-page">
            <div class="login-container">
                <div class="login-logo">
                    <div class="logo-icon-large">"🐝"</div>
                    <h1>"BeeBotOS"</h1>
                </div>
                <h2>{move || i18n_stored.get_value().t("login-title")}</h2>
                <p class="login-subtitle">{move || i18n_stored.get_value().t("login-subtitle")}</p>

                {move || error.get().map(|err| view! {
                    <div class="login-error">{err}</div>
                })}

                <div class="login-form">
                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("login-username")}</label>
                        <input
                            type="text"
                            value={username.get()}
                            on:input=move |e| set_username.set(event_target_value(&e))
                            placeholder={i18n_stored.get_value().t("login-username-placeholder")}
                            disabled=move || is_loading.get()
                        />
                    </div>
                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("login-password")}</label>
                        <input
                            type="password"
                            value={password.get()}
                            on:input=move |e| set_password.set(event_target_value(&e))
                            placeholder={i18n_stored.get_value().t("login-password-placeholder")}
                            disabled=move || is_loading.get()
                        />
                    </div>
                    <button
                        class="btn-primary btn-block btn-lg"
                        on:click=handle_submit
                        disabled=move || is_loading.get()
                    >
                        {if is_loading.get() {
                            i18n_stored.get_value().t("action-loading")
                        } else {
                            i18n_stored.get_value().t("action-login")
                        }}
                    </button>
                </div>

                <div class="login-footer">
                    <p>
                        {move || i18n_stored.get_value().t("login-no-account")}
                        " "
                        <A href="/register" attr:class="login-link">
                            {move || i18n_stored.get_value().t("login-register-link")}
                        </A>
                    </p>
                </div>
            </div>
        </div>
    }
}
