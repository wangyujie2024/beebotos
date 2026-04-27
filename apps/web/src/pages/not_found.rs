use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;
use leptos_router::components::A;

use crate::i18n::I18nContext;

#[component]
pub fn NotFound() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_clone1 = i18n.clone();
    let i18n_clone2 = i18n.clone();
    let i18n_clone3 = i18n.clone();
    let i18n_clone4 = i18n.clone();

    view! {
        <Title text={move || format!("{} - BeeBotOS", i18n_clone1.t("error-404-title"))} />
        <div class="page not-found">
            <h1>{move || i18n.t("error-404-title")}</h1>
            <p>{move || i18n_clone2.t("error-404-message")}</p>
            <p>{move || i18n_clone3.t("error-404-description")}</p>
            <A href="/" attr:class="btn btn-primary btn-lg">
                {move || i18n_clone4.t("error-go-home")}
            </A>
        </div>
    }
}
