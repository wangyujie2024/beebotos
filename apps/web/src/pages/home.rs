use crate::i18n::I18nContext;
use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;
use leptos_router::components::A;
use serde::{Deserialize, Serialize};

#[component]
pub fn Home() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);
    let app_state = crate::state::use_app_state();

    // Use LocalResource for CSR since wasm futures are not Send
    let stats = LocalResource::new(move || {
        let client = app_state.api_client();
        async move { fetch_dashboard_stats(client).await }
    });

    view! {
        <Title text={move || i18n_stored.get_value().t("app-title")} />
        <div class="home">
            // Welcome Section
            <section class="welcome-section">
                <div class="welcome-content">
                    <h1>{move || i18n_stored.get_value().t("hero-title")}</h1>
                    <p>{move || i18n_stored.get_value().t("hero-subtitle")}</p>
                </div>
            </section>

            // Stats Grid
            <Suspense fallback=|| view! { <StatsLoading/> }>
                {move || {
                    Suspend::new(async move {
                        match stats.await {
                            Ok(data) => view! { <DashboardStats stats=data/> }.into_any(),
                            Err(_) => view! { <DashboardStatsPlaceholder/> }.into_any(),
                        }
                    })
                }}
            </Suspense>

            // Features Grid
            <section>
                <div class="section-title">
                    <h2>{move || i18n_stored.get_value().t("features-title")}</h2>
                </div>
                <div class="features-grid">
                    <FeatureCard
                        icon="🤖"
                        title_key="feature-agents-title"
                        desc_key="feature-agents-desc"
                    />
                    <FeatureCard
                        icon="🏛️"
                        title_key="feature-dao-title"
                        desc_key="feature-dao-desc"
                    />
                    <FeatureCard
                        icon="🔒"
                        title_key="feature-treasury-title"
                        desc_key="feature-treasury-desc"
                    />
                    <FeatureCard
                        icon="🔌"
                        title_key="feature-skills-title"
                        desc_key="feature-skills-desc"
                    />
                    <FeatureCard
                        icon="⚡"
                        title_key="feature-wasm-title"
                        desc_key="feature-wasm-desc"
                    />
                    <FeatureCard
                        icon="📊"
                        title_key="feature-analytics-title"
                        desc_key="feature-analytics-desc"
                    />
                </div>
            </section>

            // Quick Actions
            <section>
                <div class="section-title">
                    <h2>{move || i18n_stored.get_value().t("quick-actions-title")}</h2>
                </div>
                <div class="quick-actions-grid">
                    <QuickActionCard
                        icon="➕"
                        title_key="quick-action-create-agent-title"
                        desc_key="quick-action-create-agent-desc"
                        href="/agents"
                    />
                    <QuickActionCard
                        icon="📋"
                        title_key="quick-action-view-proposals-title"
                        desc_key="quick-action-view-proposals-desc"
                        href="/dao"
                    />
                    <QuickActionCard
                        icon="🛠️"
                        title_key="quick-action-install-skills-title"
                        desc_key="quick-action-install-skills-desc"
                        href="/skills"
                    />
                    <QuickActionCard
                        icon="💬"
                        title_key="quick-action-start-chat-title"
                        desc_key="quick-action-start-chat-desc"
                        href="/chat"
                    />
                </div>
            </section>
        </div>
    }
}

#[component]
fn DashboardStats(stats: DashboardStatsData) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <section class="stats-grid">
            <div class="stat-card">
                <div class="stat-icon">"🤖"</div>
                <div class="stat-value">{stats.total_agents}</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("nav-agents")}</div>
            </div>
            <div class="stat-card">
                <div class="stat-icon">"✅"</div>
                <div class="stat-value">{stats.total_tasks}</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("stats-tasks")}</div>
            </div>
            <div class="stat-card">
                <div class="stat-icon">"📈"</div>
                <div class="stat-value">{format!("{:.1}%", stats.uptime_percent)}</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("stats-uptime")}</div>
            </div>
            <div class="stat-card">
                <div class="stat-icon">"👥"</div>
                <div class="stat-value">{stats.community_members}</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("stats-members")}</div>
            </div>
        </section>
    }
}

#[component]
fn DashboardStatsPlaceholder() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <section class="stats-grid">
            <div class="stat-card skeleton">
                <div class="stat-icon">"🤖"</div>
                <div class="stat-value">"-"</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("nav-agents")}</div>
            </div>
            <div class="stat-card skeleton">
                <div class="stat-icon">"✅"</div>
                <div class="stat-value">"-"</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("stats-tasks")}</div>
            </div>
            <div class="stat-card skeleton">
                <div class="stat-icon">"📈"</div>
                <div class="stat-value">"-"</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("stats-uptime")}</div>
            </div>
            <div class="stat-card skeleton">
                <div class="stat-icon">"👥"</div>
                <div class="stat-value">"-"</div>
                <div class="stat-label">{move || i18n_stored.get_value().t("stats-members")}</div>
            </div>
        </section>
    }
}

#[component]
fn StatsLoading() -> impl IntoView {
    view! { <DashboardStatsPlaceholder/> }
}

#[component]
fn FeatureCard(
    #[prop(into)] icon: String,
    #[prop(into)] title_key: String,
    #[prop(into)] desc_key: String,
) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="feature-card">
            <div class="feature-icon">{icon}</div>
            <h3>{move || i18n_stored.get_value().t(&title_key)}</h3>
            <p>{move || i18n_stored.get_value().t(&desc_key)}</p>
        </div>
    }
}

#[component]
fn QuickActionCard(
    #[prop(into)] icon: String,
    #[prop(into)] title_key: String,
    #[prop(into)] desc_key: String,
    #[prop(into)] href: String,
) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <A href={href.clone()} attr:class="quick-action-card">
            <div class="quick-action-icon">{icon}</div>
            <div class="quick-action-content">
                <h4>{move || i18n_stored.get_value().t(&title_key)}</h4>
                <p>{move || i18n_stored.get_value().t(&desc_key)}</p>
            </div>
            <div class="quick-action-arrow">"→"</div>
        </A>
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct DashboardStatsData {
    total_agents: u32,
    total_tasks: u32,
    uptime_percent: f64,
    community_members: u32,
}

async fn fetch_dashboard_stats(client: crate::api::ApiClient) -> Result<DashboardStatsData, ()> {
    use crate::api::AgentService;
    use crate::api::DaoService;

    let agent_service = AgentService::new(client.clone());
    let dao_service = DaoService::new(client);

    // Fetch data sequentially (WASM-friendly)
    let agents_result = agent_service.list().await;
    let dao_result = dao_service.get_summary().await;

    match (agents_result, dao_result) {
        (Ok(agents), Ok(dao_summary)) => {
            // Calculate stats from real data
            let total_agents = agents.len() as u32;
            let total_tasks: u32 = agents.iter().map(|a| a.task_count.unwrap_or(0)).sum();
            let uptime_percent = if agents.is_empty() {
                100.0
            } else {
                agents
                    .iter()
                    .map(|a| a.uptime_percent.unwrap_or(100.0))
                    .sum::<f64>()
                    / agents.len() as f64
            };

            Ok(DashboardStatsData {
                total_agents,
                total_tasks,
                uptime_percent,
                community_members: dao_summary.member_count as u32,
            })
        }
        _ => {
            // Fallback to empty data if API fails
            Ok(DashboardStatsData {
                total_agents: 0,
                total_tasks: 0,
                uptime_percent: 100.0,
                community_members: 0,
            })
        }
    }
}
