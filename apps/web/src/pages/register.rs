//! Register Page

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::api::AuthService;
use crate::i18n::I18nContext;
use crate::state::{use_auth_state, User};

#[component]
pub fn RegisterPage() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);
    let navigate = use_navigate();
    let auth_state = use_auth_state();

    let (username, set_username) = signal(String::new());
    let (email, set_email) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (confirm_password, set_confirm_password) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (is_loading, set_is_loading) = signal(false);

    let handle_submit = {
        let auth_state = auth_state.clone();
        let navigate = navigate.clone();
        move |_| {
            // Validation
            if username.get().is_empty() || password.get().is_empty() {
                set_error.set(Some(i18n_stored.get_value().t("register-error-empty")));
                return;
            }

            if password.get() != confirm_password.get() {
                set_error.set(Some(
                    i18n_stored
                        .get_value()
                        .t("register-error-password-mismatch"),
                ));
                return;
            }

            if password.get().len() < 6 {
                set_error.set(Some(
                    i18n_stored.get_value().t("register-error-password-short"),
                ));
                return;
            }

            set_is_loading.set(true);
            set_error.set(None);

            let auth = auth_state.clone();
            let nav = navigate.clone();
            let i18n = i18n_stored.get_value();

            spawn_local(async move {
                let client = crate::api::create_client();
                let auth_service = AuthService::new(client);

                // Try to register via API
                match auth_service
                    .register(&username.get(), &email.get(), &password.get())
                    .await
                {
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

                        // Navigate to home
                        nav("/", Default::default());
                    }
                    Err(e) => {
                        set_error.set(Some(format!("{}: {}", i18n.t("register-error-failed"), e)));
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
                <h2>{move || i18n_stored.get_value().t("register-title")}</h2>
                <p class="login-subtitle">{move || i18n_stored.get_value().t("register-subtitle")}</p>

                {move || error.get().map(|err| view! {
                    <div class="login-error">{err}</div>
                })}

                <div class="login-form">
                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("register-username")}</label>
                        <input
                            type="text"
                            value={username.get()}
                            on:input=move |e| set_username.set(event_target_value(&e))
                            placeholder={i18n_stored.get_value().t("register-username-placeholder")}
                            disabled=move || is_loading.get()
                        />
                    </div>
                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("register-email")}</label>
                        <input
                            type="email"
                            value={email.get()}
                            on:input=move |e| set_email.set(event_target_value(&e))
                            placeholder={i18n_stored.get_value().t("register-email-placeholder")}
                            disabled=move || is_loading.get()
                        />
                    </div>
                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("register-password")}</label>
                        <input
                            type="password"
                            value={password.get()}
                            on:input=move |e| set_password.set(event_target_value(&e))
                            placeholder={i18n_stored.get_value().t("register-password-placeholder")}
                            disabled=move || is_loading.get()
                        />
                    </div>
                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("register-confirm-password")}</label>
                        <input
                            type="password"
                            value={confirm_password.get()}
                            on:input=move |e| set_confirm_password.set(event_target_value(&e))
                            placeholder={i18n_stored.get_value().t("register-confirm-password-placeholder")}
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
                            i18n_stored.get_value().t("action-register")
                        }}
                    </button>
                </div>

                <div class="login-footer">
                    <p>
                        {move || i18n_stored.get_value().t("register-have-account")}
                        " "
                        <A href="/login" attr:class="login-link">
                            {move || i18n_stored.get_value().t("register-login-link")}
                        </A>
                    </p>
                </div>
            </div>
        </div>
    }
}
