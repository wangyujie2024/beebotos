use crate::i18n::I18nContext;
use leptos::prelude::*;
use leptos::view;

/// Full page loading spinner
#[component]
pub fn PageLoading() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="page-loading">
            <div class="spinner-large"></div>
            <p>{move || i18n_stored.get_value().t("loading-page")}</p>
        </div>
    }
}

/// Card skeleton for grid layouts
#[component]
pub fn CardSkeleton() -> impl IntoView {
    view! {
        <div class="card skeleton-card">
            <div class="skeleton-header">
                <div class="skeleton-avatar"></div>
                <div class="skeleton-title"></div>
            </div>
            <div class="skeleton-content">
                <div class="skeleton-line"></div>
                <div class="skeleton-line short"></div>
            </div>
            <div class="skeleton-footer">
                <div class="skeleton-badge"></div>
                <div class="skeleton-button"></div>
            </div>
        </div>
    }
}

/// List item skeleton
#[component]
pub fn ListItemSkeleton() -> impl IntoView {
    view! {
        <div class="list-item-skeleton">
            <div class="skeleton-icon"></div>
            <div class="skeleton-content">
                <div class="skeleton-line"></div>
                <div class="skeleton-line short"></div>
            </div>
        </div>
    }
}

/// Table skeleton
#[component]
pub fn TableSkeleton(rows: usize) -> impl IntoView {
    view! {
        <div class="table-skeleton">
            <div class="table-header-skeleton">
                <div class="skeleton-cell"></div>
                <div class="skeleton-cell"></div>
                <div class="skeleton-cell"></div>
                <div class="skeleton-cell"></div>
            </div>
            {vec![0; rows].into_iter().map(|_| view! {
                <div class="table-row-skeleton">
                    <div class="skeleton-cell"></div>
                    <div class="skeleton-cell"></div>
                    <div class="skeleton-cell"></div>
                    <div class="skeleton-cell"></div>
                </div>
            }).collect::<Vec<_>>()}
        </div>
    }
}

/// Stats card skeleton
#[component]
pub fn StatsCardSkeleton() -> impl IntoView {
    view! {
        <div class="stat-card skeleton">
            <div class="skeleton-value"></div>
            <div class="skeleton-label"></div>
        </div>
    }
}

/// Content placeholder with shimmer effect
#[component]
pub fn ShimmerPlaceholder() -> impl IntoView {
    view! {
        <div class="shimmer-wrapper">
            <div class="shimmer"></div>
        </div>
    }
}

/// Progressive loading state
#[component]
pub fn ProgressiveLoading(
    #[prop(into)] message: String,
    #[prop(default = 0)] progress: i32,
) -> impl IntoView {
    view! {
        <div class="progressive-loading">
            <div class="spinner"></div>
            <p>{message}</p>
            {move || if progress > 0 {
                view! {
                    <div class="progress-bar">
                        <div class="progress-fill" style={format!("width: {}%", progress)}></div>
                    </div>
                    <span class="progress-text">{format!("{}%", progress)}</span>
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}
        </div>
    }
}

/// Inline loading spinner
#[component]
pub fn InlineLoading(#[prop(optional)] text: Option<String>) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <span class="inline-loading">
            <span class="spinner-small"></span>
            {text.map(|t| view! { <span>{t}</span> }.into_any()).unwrap_or_else(|| view! { <span>{move || i18n_stored.get_value().t("loading-inline")}</span> }.into_any())}
        </span>
    }
}

/// Skeleton grid for card layouts
#[component]
pub fn SkeletonGrid(count: usize, columns: usize) -> impl IntoView {
    view! {
        <div class="grid" style={format!("grid-template-columns: repeat({}, 1fr)", columns)}>
            {vec![0; count].into_iter().map(|_| view! {
                <CardSkeleton/>
            }).collect::<Vec<_>>()}
        </div>
    }
}

/// Content fade in wrapper
#[component]
pub fn FadeIn(children: Children) -> impl IntoView {
    view! {
        <div class="fade-in">
            {children()}
        </div>
    }
}
