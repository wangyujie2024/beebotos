//! Reusable star rating display component

use leptos::prelude::*;

/// Display a 5-star rating with full, half, and empty stars
#[component]
pub fn StarRating(#[prop(into)] rating: f64) -> impl IntoView {
    let full = rating.floor() as usize;
    let half = if rating - rating.floor() >= 0.5 { 1 } else { 0 };
    let empty = 5usize.saturating_sub(full + half);
    let mut stars = String::new();
    stars.push_str(&"★".repeat(full));
    if half > 0 {
        stars.push('⯪');
    }
    stars.push_str(&"☆".repeat(empty));
    view! { <span class="star-rating">{stars}</span> }
}
