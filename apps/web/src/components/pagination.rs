use leptos::prelude::*;
use leptos::view;
use std::sync::Arc;
use wasm_bindgen::JsCast;

#[derive(Clone, Debug)]
pub struct PaginationState {
    pub current_page: usize,
    pub total_pages: usize,
    pub page_size: usize,
    pub total_items: usize,
}

impl PaginationState {
    pub fn new(page_size: usize) -> Self {
        Self {
            current_page: 1,
            total_pages: 1,
            page_size,
            total_items: 0,
        }
    }

    pub fn offset(&self) -> usize {
        (self.current_page - 1) * self.page_size
    }

    pub fn has_next(&self) -> bool {
        self.current_page < self.total_pages
    }

    pub fn has_prev(&self) -> bool {
        self.current_page > 1
    }

    pub fn set_total(&mut self, total: usize) {
        self.total_items = total;
        self.total_pages = (total + self.page_size - 1) / self.page_size;
        if self.total_pages == 0 {
            self.total_pages = 1;
        }
        if self.current_page > self.total_pages {
            self.current_page = self.total_pages;
        }
    }

    pub fn page_range(&self) -> Vec<usize> {
        let mut pages = Vec::new();
        let start = self.current_page.saturating_sub(2).max(1);
        let end = (self.current_page + 2).min(self.total_pages);

        for i in start..=end {
            pages.push(i);
        }
        pages
    }
}

/// Pagination component
#[component]
pub fn Pagination(
    state: RwSignal<PaginationState>,
    on_change: impl Fn(usize) + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let on_change = Arc::new(on_change);

    let on_click_prev = {
        let on_change = on_change.clone();
        move |_| {
            let new_page = state.get().current_page.saturating_sub(1).max(1);
            state.update(|s| s.current_page = new_page);
            on_change(new_page);
        }
    };

    let on_click_next = {
        let on_change = on_change.clone();
        move |_| {
            let new_page = (state.get().current_page + 1).min(state.get().total_pages);
            state.update(|s| s.current_page = new_page);
            on_change(new_page);
        }
    };

    view! {
        <div class="pagination">
            <button
                class="pagination-btn"
                disabled=move || !state.get().has_prev()
                on:click=on_click_prev
            >
                "← Previous"
            </button>

            <div class="pagination-pages">
                {move || {
                    let pages = state.get().page_range();
                    pages.into_iter().map(|page| {
                        let is_active = state.get().current_page == page;
                        let on_change = on_change.clone();
                        view! {
                            <button
                                class={format!("pagination-page {}", if is_active { "active" } else { "" })}
                                on:click=move |_| {
                                    state.update(|s| s.current_page = page);
                                    on_change(page);
                                }
                            >
                                {page}
                            </button>
                        }
                    }).collect::<Vec<_>>()
                }}
            </div>

            <button
                class="pagination-btn"
                disabled=move || !state.get().has_next()
                on:click=on_click_next
            >
                "Next →"
            </button>

            <span class="pagination-info">
                {move || format!(
                    "Page {} of {} ({} items)",
                    state.get().current_page,
                    state.get().total_pages,
                    state.get().total_items
                )}
            </span>
        </div>
    }
}

/// Virtual list - simplified for CSR
#[component]
pub fn VirtualList<T, F, V>(
    items: Vec<T>,
    #[prop(default = 50)] item_height: usize,
    #[prop(default = 10)] _overscan: usize,
    view_fn: F,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    F: Fn(T) -> V + Clone + 'static,
    V: IntoView,
{
    let container_ref = NodeRef::new();
    let scroll_position = RwSignal::new(0_usize);
    let container_height = RwSignal::new(400_usize);

    let total_height = items.len() * item_height;

    view! {
        <div
            class="virtual-list-container"
            node_ref=container_ref
            style={format!("height: {}px; overflow-y: auto; position: relative;", container_height.get())}
            on:scroll=move |e| {
                if let Some(target) = e.target() {
                    if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                        scroll_position.set(el.scroll_top() as usize);
                    }
                }
            }
        >
            <div
                class="virtual-list-content"
                style={format!("height: {}px; position: relative;", total_height)}
            >
                {items.into_iter().enumerate().map(|(idx, item)| {
                    let offset = idx * item_height;
                    view! {
                        <div
                            class="virtual-list-item"
                            style={format!("position: absolute; top: {}px; height: {}px", offset, item_height)}
                        >
                            {view_fn(item)}
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}

/// Load more trigger for infinite scroll
#[component]
pub fn LoadMoreTrigger(
    on_load_more: impl Fn() + 'static,
    #[prop(optional)] loading: Option<RwSignal<bool>>,
) -> impl IntoView {
    let trigger_ref = NodeRef::new();
    let is_loading = loading.unwrap_or_else(|| RwSignal::new(false));

    let on_load_more = std::rc::Rc::new(on_load_more);

    Effect::new(move |_| {
        if let Some(trigger) = trigger_ref.get() {
            let trigger: web_sys::HtmlDivElement = trigger;
            if let Ok(element) = trigger.dyn_into::<web_sys::Element>() {
                let on_load_more = on_load_more.clone();
                let observer = web_sys::IntersectionObserver::new(
                    &wasm_bindgen::closure::Closure::wrap(Box::new(
                        move |entries: wasm_bindgen::JsValue| {
                            let Ok(entries) = entries.dyn_into::<js_sys::Array>() else {
                                return;
                            };
                            if let Some(entry) = entries
                                .get(0)
                                .dyn_into::<web_sys::IntersectionObserverEntry>()
                                .ok()
                            {
                                if entry.is_intersecting() && !is_loading.get() {
                                    on_load_more();
                                }
                            }
                        },
                    )
                        as Box<dyn FnMut(_)>)
                    .into_js_value()
                    .unchecked_ref(),
                )
                .ok();

                if let Some(observer) = observer {
                    observer.observe(&element);
                }
            }
        }
    });

    view! {
        <div
            class="load-more-trigger"
            node_ref=trigger_ref
        >
            {move || if is_loading.get() {
                view! {
                    <div class="loading-indicator">
                        <span class="spinner-small"></span>
                        <span>"Loading more..."</span>
                    </div>
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}
        </div>
    }
}

/// Page size selector
#[component]
pub fn PageSizeSelector(
    current_size: RwSignal<usize>,
    options: Vec<usize>,
    on_change: impl Fn(usize) + 'static,
) -> impl IntoView {
    view! {
        <div class="page-size-selector">
            <label>"Show:"</label>
            <select
                prop:value=move || current_size.get()
                on:change=move |e| {
                    let value = event_target_value(&e);
                    if let Ok(size) = value.parse::<usize>() {
                        current_size.set(size);
                        on_change(size);
                    }
                }
            >
                {options.into_iter().map(|size| view! {
                    <option value=size>{format!("{} per page", size)}</option>
                }).collect::<Vec<_>>()}
            </select>
        </div>
    }
}
