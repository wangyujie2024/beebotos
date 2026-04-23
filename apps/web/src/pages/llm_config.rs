//! LLM Configuration & Monitoring Page
//!
//! Displays global LLM configuration and real-time metrics from Gateway.

use crate::api::{LlmConfigService, LlmGlobalConfig, LlmMetricsResponse, LlmHealthResponse};
use crate::components::{BarChart, InlineLoading, InfoItem, PieChart};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[component]
pub fn LlmConfigPage() -> impl IntoView {
    let client = crate::api::create_client();
    let llm_service = LlmConfigService::new(client);
    let service_stored = StoredValue::new(llm_service);

    let config: RwSignal<Option<LlmGlobalConfig>> = RwSignal::new(None);
    let metrics: RwSignal<Option<LlmMetricsResponse>> = RwSignal::new(None);
    let health: RwSignal<Option<LlmHealthResponse>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let loading = RwSignal::new(true);

    let fetch_all = move || {
        let service = service_stored.get_value();
        loading.set(true);
        error.set(None);
        spawn_local(async move {
            match service.get_config().await {
                Ok(c) => config.set(Some(c)),
                Err(e) => error.set(Some(format!("Config: {}", e))),
            }
            match service.get_metrics().await {
                Ok(m) => metrics.set(Some(m)),
                Err(e) => {
                    let msg = error.get().unwrap_or_default();
                    error.set(Some(format!("{} Metrics: {}", msg, e)));
                }
            }
            match service.get_health().await {
                Ok(h) => health.set(Some(h)),
                Err(e) => {
                    let msg = error.get().unwrap_or_default();
                    error.set(Some(format!("{} Health: {}", msg, e)));
                }
            }
            loading.set(false);
        });
    };

    let fetch_stored = StoredValue::new(fetch_all);

    // Initial fetch
    Effect::new(move |_| {
        fetch_stored.get_value()();
    });

    // Auto-refresh metrics every 10s
    let should_poll = RwSignal::new(true);

    Effect::new(move |_| {
        let should_poll = should_poll;

        // Sync polling state with document visibility
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            let hidden = document.hidden();
            should_poll.set(!hidden);

            let doc_for_handler = document.clone();
            let visibility_handler = Closure::wrap(Box::new(move || {
                let hidden = doc_for_handler.hidden();
                should_poll.set(!hidden);
            }) as Box<dyn FnMut()>);
            let _ = document.add_event_listener_with_callback(
                "visibilitychange",
                visibility_handler.as_ref().unchecked_ref(),
            );
            visibility_handler.forget();
        }

        // Stop polling when component unmounts
        on_cleanup(move || {
            should_poll.set(false);
        });

        spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(10_000).await;
                if !should_poll.get() {
                    break;
                }
                let service = service_stored.get_value();
                if let Ok(m) = service.get_metrics().await {
                    metrics.set(Some(m));
                }
                if let Ok(h) = service.get_health().await {
                    health.set(Some(h));
                }
            }
        });
    });

    view! {
        <Title text="LLM Configuration - BeeBotOS" />
        <div class="page llm-config-page">
            <div class="page-header">
                <h1>"LLM Configuration"</h1>
                <p class="page-description">"Global LLM settings and real-time monitoring"</p>
            </div>

            {move || if loading.get() {
                view! { <InlineLoading /> }.into_any()
            } else if let Some(err) = error.get() {
                view! {
                    <div class="error-state">
                        <div class="error-icon">"⚠️"</div>
                        <p>{err}</p>
                        <button class="btn btn-primary" on:click=move |_| fetch_stored.get_value()()>
                            "Retry"
                        </button>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="llm-config-grid">
                        // Global Config Card
                        {config.get().map(|cfg| view! {
                            <section class="card llm-section">
                                <h2>"Global Configuration"</h2>
                                <div class="info-grid">
                                    <InfoItem class="info-row" label="Default Provider" value=cfg.default_provider />
                                </div>
                            </section>
                        })}

                        // Provider Cards
                        {config.get().map(|cfg| view! {
                            <section class="card llm-section">
                                <h2>"Providers"</h2>
                                <div class="provider-cards">
                                    {cfg.providers.into_iter().map(|p| {
                                        let health_status = health.get()
                                            .and_then(|h| h.providers.iter().find(|ph| ph.name == p.name).cloned());
                                        view! {
                                            <div class="provider-card">
                                                <div class="provider-header">
                                                    <h3>{p.name.clone()}</h3>
                                                    {health_status.map(|h| view! {
                                                        <span class=format!("health-badge {}", if h.healthy { "healthy" } else { "unhealthy" })>
                                                            {if h.healthy { "● Healthy".to_string() } else { format!("● {} failures", h.consecutive_failures) }}
                                                        </span>
                                                    })}
                                                </div>
                                                <div class="info-grid">
                                                    <InfoItem class="info-row" label="Model" value=p.model />
                                                    <InfoItem class="info-row" label="Base URL" value=p.base_url />
                                                    <InfoItem class="info-row" label="API Key" value=p.api_key_masked />
                                                    <InfoItem class="info-row" label="Protocol" value=p.protocol />
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </section>
                        })}

                        // Metrics Card
                        {metrics.get().map(|m| view! {
                            <section class="card llm-section">
                                <h2>"Real-time Metrics"</h2>
                                <p class="timestamp">{format!("Last updated: {}", m.timestamp)}</p>
                                <div class="metrics-grid">
                                    <MetricCard
                                        label="Total Requests"
                                        value=m.summary.total_requests.to_string()
                                        delta=Some(format!("{:.1}% success", m.summary.success_rate_percent))
                                    />
                                    <MetricCard
                                        label="Successful"
                                        value=m.summary.successful_requests.to_string()
                                        delta=None
                                    />
                                    <MetricCard
                                        label="Failed"
                                        value=m.summary.failed_requests.to_string()
                                        delta=None
                                    />
                                    <MetricCard
                                        label="Total Tokens"
                                        value=m.tokens.total_tokens.to_string()
                                        delta=Some(format!("{} in / {} out", m.tokens.input_tokens, m.tokens.output_tokens))
                                    />
                                </div>
                                <h3>"Latency"</h3>
                                <div class="latency-bars">
                                    <LatencyBar label="Avg" value=m.latency.average_ms max=1000.0 />
                                    <LatencyBar label="P50" value=m.latency.p50_ms max=1000.0 />
                                    <LatencyBar label="P95" value=m.latency.p95_ms max=1000.0 />
                                    <LatencyBar label="P99" value=m.latency.p99_ms max=1000.0 />
                                </div>

                                <h3>"Visual Overview"</h3>
                                <div class="charts-grid">
                                    <PieChart
                                        title="Request Distribution"
                                        labels=vec!["Success".to_string(), "Failed".to_string()]
                                        values=vec![m.summary.successful_requests as f64, m.summary.failed_requests as f64]
                                    />
                                    <PieChart
                                        title="Token Usage"
                                        labels=vec!["Input".to_string(), "Output".to_string()]
                                        values=vec![m.tokens.input_tokens as f64, m.tokens.output_tokens as f64]
                                    />
                                    <BarChart
                                        title="Latency Percentiles (ms)"
                                        labels=vec!["Avg".to_string(), "P50".to_string(), "P95".to_string(), "P99".to_string()]
                                        values=vec![m.latency.average_ms, m.latency.p50_ms, m.latency.p95_ms, m.latency.p99_ms]
                                    />
                                </div>
                            </section>
                        })}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn MetricCard(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    delta: Option<String>,
) -> impl IntoView {
    view! {
        <div class="metric-card">
            <div class="metric-value">{value}</div>
            <div class="metric-label">{label}</div>
            {delta.map(|d| view! { <div class="metric-delta">{d}</div> })}
        </div>
    }
}

#[component]
fn LatencyBar(
    #[prop(into)] label: String,
    value: f64,
    max: f64,
) -> impl IntoView {
    let pct = (value / max * 100.0).min(100.0);
    let color_class = if pct < 30.0 {
        "latency-good"
    } else if pct < 70.0 {
        "latency-warning"
    } else {
        "latency-danger"
    };

    view! {
        <div class="latency-bar">
            <span class="latency-label">{label}</span>
            <span class="latency-value">{format!("{:.0}ms", value)}</span>
            <div class="latency-track">
                <div class=format!("latency-fill {}", color_class) style=format!("width: {}%", pct)></div>
            </div>
        </div>
    }
}
