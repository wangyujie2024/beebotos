//! DOM helper utilities for Leptos WASM frontend

use wasm_bindgen::JsCast;

/// Extract the `value` from an event target (input, textarea, or select)
pub fn event_target_value(ev: &leptos::ev::Event) -> String {
    ev.target()
        .and_then(|t| {
            t.dyn_into::<web_sys::HtmlInputElement>()
                .ok()
                .map(|e| e.value())
        })
        .or_else(|| {
            ev.target().and_then(|t| {
                t.dyn_into::<web_sys::HtmlTextAreaElement>()
                    .ok()
                    .map(|e| e.value())
            })
        })
        .or_else(|| {
            ev.target().and_then(|t| {
                t.dyn_into::<web_sys::HtmlSelectElement>()
                    .ok()
                    .map(|e| e.value())
            })
        })
        .unwrap_or_default()
}

/// Extract the `checked` state from an event target (checkbox/radio input)
pub fn event_target_checked(ev: &leptos::ev::Event) -> bool {
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|i| i.checked())
        .unwrap_or(false)
}

/// Trigger a file download in the browser
pub fn download_file(filename: &str, content: &str, _mime: &str) {
    let blob = web_sys::Blob::new_with_str_sequence(&js_sys::Array::of1(&content.into())).ok();
    if let Some(blob) = blob {
        let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap_or_default();
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Ok(anchor) = document.create_element("a") {
                    let anchor: web_sys::HtmlAnchorElement = anchor.dyn_into().unwrap();
                    anchor.set_href(&url);
                    anchor.set_download(filename);
                    anchor.set_attribute("style", "display:none").ok();
                    let body = document.body().unwrap();
                    let _ = body.append_child(&anchor);
                    anchor.click();
                    let _ = body.remove_child(&anchor);
                }
            }
        }
        web_sys::Url::revoke_object_url(&url).ok();
    }
}
