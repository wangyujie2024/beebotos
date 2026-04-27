use crate::i18n::I18nContext;
use leptos::prelude::*;
use leptos::view;

/// Error boundary component that catches errors from children
#[component]
pub fn ErrorBoundary(
    #[prop(into)] fallback: Option<ViewFn>,
    children: ChildrenFn,
) -> impl IntoView {
    let errors = RwSignal::new(Vec::new());

    // Create error handler context
    provide_context(ErrorContext { errors });

    view! {
        <Show
            when=move || errors.with(|e| e.is_empty())
            fallback=fallback.clone().unwrap_or_else(|| ViewFn::from(default_error_view))
        >
            {children.clone()()}
        </Show>
    }
}

#[derive(Clone)]
pub struct ErrorContext {
    errors: RwSignal<Vec<AppError>>,
}

impl ErrorContext {
    pub fn catch_error(&self, error: impl std::fmt::Display) {
        let app_error = AppError {
            id: uuid::Uuid::new_v4().to_string(),
            message: error.to_string(),
            timestamp: chrono::Utc::now(),
        };
        self.errors.update(|e| e.push(app_error));
    }

    pub fn clear_errors(&self) {
        self.errors.set(Vec::new());
    }

    pub fn dismiss_error(&self, id: &str) {
        self.errors.update(|e| {
            e.retain(|err| err.id != id);
        });
    }
}

#[derive(Clone)]
pub struct AppError {
    pub id: String,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

fn default_error_view() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="error-boundary">
            <h2>{move || i18n_stored.get_value().t("error-generic")}</h2>
            <p>{move || i18n_stored.get_value().t("error-refresh-hint")}</p>
            <button on:click=move |_| {
                let _ = window().location().reload();
            }>
                {move || i18n_stored.get_value().t("action-refresh")}
            </button>
        </div>
    }
}

/// Hook to use error context
pub fn use_error_context() -> ErrorContext {
    use_context::<ErrorContext>().expect("ErrorContext not found")
}

/// Toast notification container
#[component]
pub fn ToastContainer() -> impl IntoView {
    let error_ctx = use_error_context();

    view! {
        <div class="toast-container">
            <For
                each=move || error_ctx.errors.get()
                key=|error| error.id.clone()
                children=move |error| {
                    let error_id = error.id.clone();
                    let error_ctx_clone = error_ctx.clone();
                    view! {
                        <Toast
                            message=error.message
                            on_dismiss=move || error_ctx_clone.dismiss_error(&error_id)
                        />
                    }
                }
            />
        </div>
    }
}

#[component]
fn Toast(#[prop(into)] message: String, on_dismiss: impl Fn() + 'static) -> impl IntoView {
    view! {
        <div class="toast toast-error">
            <span>{message}</span>
            <button on:click=move |_| on_dismiss()>"✕"</button>
        </div>
    }
}

/// Async error handler wrapper - simplified for CSR
/// Note: In CSR mode with Leptos 0.8, use LocalResource instead of this
/// component
#[component]
pub fn AsyncHandler<T, E, F, V>(
    #[prop(into)] _future: F,
    #[prop(optional)] _on_error: Option<Box<dyn Fn(E)>>,
    _children: impl Fn(T) -> V + 'static,
) -> impl IntoView
where
    T: Send + Sync + Clone + 'static,
    E: std::fmt::Display + Send + Sync + Clone + 'static,
    F: std::future::Future<Output = Result<T, E>> + Send + 'static,
    V: IntoView,
{
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    // This is a placeholder - in CSR mode you should use LocalResource directly
    view! {
        <div>{move || i18n_stored.get_value().t("error-async-handler")}</div>
    }
}

#[component]
fn LoadingSpinner() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="loading-spinner">
            <div class="spinner"></div>
            <span>{move || i18n_stored.get_value().t("loading-page")}</span>
        </div>
    }
}

#[component]
pub fn ErrorMessage(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <div class="error-message">
            <span class="error-icon">"⚠️"</span>
            <span>{message}</span>
        </div>
    }
}

/// Global error handler
#[component]
pub fn GlobalErrorHandler(children: ChildrenFn) -> impl IntoView {
    provide_context(ErrorContext {
        errors: RwSignal::new(Vec::new()),
    });

    view! {
        <>
            {children.clone()()}
            <ToastContainer/>
        </>
    }
}
