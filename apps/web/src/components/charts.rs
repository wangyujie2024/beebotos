//! Simple canvas-based chart components

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Render a simple bar chart on canvas
#[component]
pub fn BarChart(
    #[prop(into)] labels: Vec<String>,
    #[prop(into)] values: Vec<f64>,
    #[prop(into)] title: String,
    #[prop(default = 300)] width: u32,
    #[prop(default = 200)] height: u32,
) -> impl IntoView {
    let canvas_ref: NodeRef<leptos::html::Canvas> = NodeRef::new();

    Effect::new(move |_| {
        if let Some(canvas) = canvas_ref.get() {
            let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
            canvas.set_width(width);
            canvas.set_height(height);

            if let Ok(ctx) = canvas.get_context("2d").unwrap().unwrap().dyn_into::<web_sys::CanvasRenderingContext2d>() {
                // Clear
                ctx.set_fill_style_str("#1a1a2e");
                ctx.fill_rect(0.0, 0.0, width as f64, height as f64);

                if values.is_empty() {
                    return;
                }

                let max_val = values.iter().copied().fold(0.0, f64::max).max(1.0);
                let bar_count = values.len() as f64;
                let padding = 40.0;
                let chart_w = width as f64 - padding * 2.0;
                let chart_h = height as f64 - padding * 2.0;
                let bar_w = chart_w / bar_count * 0.6;
                let gap = chart_w / bar_count * 0.4;

                // Title
                ctx.set_fill_style_str("#e0e0e0");
                ctx.set_font("14px sans-serif");
                let _ = ctx.fill_text(&title, padding, 20.0);

                // Bars
                for (i, (label, val)) in labels.iter().zip(values.iter()).enumerate() {
                    let x = padding + i as f64 * (bar_w + gap) + gap / 2.0;
                    let bar_h = (val / max_val) * chart_h;
                    let y = padding + chart_h - bar_h;

                    // Bar color
                    let hue = 200.0 + (i as f64 * 40.0) % 160.0;
                    ctx.set_fill_style_str(&format!("hsl({}, 70%, 50%)", hue));
                    ctx.fill_rect(x, y, bar_w, bar_h);

                    // Label
                    ctx.set_fill_style_str("#aaa");
                    ctx.set_font("10px sans-serif");
                    let _ = ctx.fill_text(label, x, padding + chart_h + 12.0);

                    // Value
                    ctx.set_fill_style_str("#fff");
                    let _ = ctx.fill_text(&format!("{:.0}", val), x + bar_w / 4.0, y - 4.0);
                }

                // Axis lines
                ctx.set_stroke_style_str("#444");
                ctx.begin_path();
                ctx.move_to(padding, padding);
                ctx.line_to(padding, padding + chart_h);
                ctx.line_to(padding + chart_w, padding + chart_h);
                ctx.stroke();
            }
        }
    });

    view! { <canvas node_ref=canvas_ref style=format!("width:{}px;height:{}px", width, height) /> }
}

/// Render a simple pie/donut chart on canvas
#[component]
pub fn PieChart(
    #[prop(into)] labels: Vec<String>,
    #[prop(into)] values: Vec<f64>,
    #[prop(into)] title: String,
    #[prop(default = 250)] width: u32,
    #[prop(default = 250)] height: u32,
) -> impl IntoView {
    let canvas_ref: NodeRef<leptos::html::Canvas> = NodeRef::new();

    Effect::new(move |_| {
        if let Some(canvas) = canvas_ref.get() {
            let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
            canvas.set_width(width);
            canvas.set_height(height);

            if let Ok(ctx) = canvas.get_context("2d").unwrap().unwrap().dyn_into::<web_sys::CanvasRenderingContext2d>() {
                ctx.set_fill_style_str("#1a1a2e");
                ctx.fill_rect(0.0, 0.0, width as f64, height as f64);

                let total: f64 = values.iter().sum();
                if total <= 0.0 {
                    return;
                }

                let cx = width as f64 / 2.0;
                let cy = height as f64 / 2.0;
                let radius = (width.min(height) as f64 / 2.0) - 30.0;
                let mut start_angle = -std::f64::consts::FRAC_PI_2;

                // Title
                ctx.set_fill_style_str("#e0e0e0");
                ctx.set_font("14px sans-serif");
                let _ = ctx.fill_text(&title, 10.0, 20.0);

                for (i, (label, val)) in labels.iter().zip(values.iter()).enumerate() {
                    let slice_angle = (val / total) * 2.0 * std::f64::consts::PI;
                    let end_angle = start_angle + slice_angle;

                    let hue = 200.0 + (i as f64 * 50.0) % 160.0;
                    ctx.set_fill_style_str(&format!("hsl({}, 70%, 55%)", hue));
                    ctx.begin_path();
                    ctx.move_to(cx, cy);
                    ctx.arc(cx, cy, radius, start_angle, end_angle).unwrap();
                    ctx.close_path();
                    ctx.fill();

                    // Label
                    let mid_angle = start_angle + slice_angle / 2.0;
                    let lx = cx + (radius * 0.7) * mid_angle.cos();
                    let ly = cy + (radius * 0.7) * mid_angle.sin();
                    ctx.set_fill_style_str("#fff");
                    ctx.set_font("11px sans-serif");
                    let pct = (val / total * 100.0) as u32;
                    let _ = ctx.fill_text(&format!("{}% {}", pct, label), lx - 20.0, ly);

                    start_angle = end_angle;
                }

                // Donut hole
                ctx.set_fill_style_str("#1a1a2e");
                ctx.begin_path();
                ctx.arc(cx, cy, radius * 0.4, 0.0, 2.0 * std::f64::consts::PI).unwrap();
                ctx.fill();
            }
        }
    });

    view! { <canvas node_ref=canvas_ref style=format!("width:{}px;height:{}px", width, height) /> }
}
