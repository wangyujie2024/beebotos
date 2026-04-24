//! Skills Marketplace Page
//!
//! Browse, install, and manage WASM skills from ClawHub/BeeHub or local
//! registry.

use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;

use crate::api::{InstallSkillRequest, SkillCategory, SkillInfo};
use crate::components::{Modal, StarRating};
use crate::state::use_app_state;

#[component]
pub fn SkillsPage() -> impl IntoView {
    let app_state = use_app_state();
    let search_input = RwSignal::new(String::new());
    let active_search = RwSignal::new(String::new());
    let selected_hub = RwSignal::new(None::<String>);
    let selected_category = RwSignal::new(None::<SkillCategory>);
    let show_details = RwSignal::new(None::<SkillInfo>);

    // Fetch skills - use LocalResource for CSR
    let skills = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let service = app_state.skill_service();
            let hub = selected_hub.get();
            let search = active_search.get();
            let app_state = app_state.clone();
            async move {
                app_state.loading().skills.set(true);
                let result = service
                    .list(hub.as_deref().filter(|h| !h.is_empty()), Some(&search))
                    .await;
                app_state.loading().skills.set(false);
                result
            }
        }
    });

    // Helper to reload skills after install/uninstall or search/hub change
    let reload_skills = {
        let skills = skills.clone();
        move || {
            skills.refetch();
        }
    };

    let perform_search = {
        let active_search = active_search.clone();
        let search_input = search_input.clone();
        let reload = reload_skills.clone();
        move || {
            active_search.set(search_input.get());
            reload();
        }
    };

    view! {
        <Title text="Skills - BeeBotOS" />
        <div class="page skills-page">
            <div class="page-header">
                <div>
                    <h1>"Skill Marketplace"</h1>
                    <p class="page-description">"Browse and install community-built skills to extend your agents"</p>
                </div>
            </div>

            // === Skill Type Info Banner (P3) ===
            <div class="info-banner">
                <span class="info-icon">"ℹ️"</span>
                <span>
                    "This marketplace shows "
                    <strong>"WASM Runtime Skills"</strong>
                    " (skill.wasm + skill.yaml). Prompt templates in the skills/ directory are separate LLM role definitions and not shown here."
                </span>
            </div>

            <section class="skills-controls">
                // === Hub Selector (P2) ===
                <div class="hub-selector">
                    <span class="hub-label">"Source:"</span>
                    <HubButton
                        label="Local"
                        is_active={
                            let selected = selected_hub.clone();
                            move || selected.get().is_none()
                        }
                        on_click={
                            let selected = selected_hub.clone();
                            let reload = reload_skills.clone();
                            move || { selected.set(None); reload(); }
                        }
                    />
                    <HubButton
                        label="ClawHub"
                        is_active={
                            let selected = selected_hub.clone();
                            move || selected.get().as_deref() == Some("clawhub")
                        }
                        on_click={
                            let selected = selected_hub.clone();
                            let reload = reload_skills.clone();
                            move || { selected.set(Some("clawhub".to_string())); reload(); }
                        }
                    />
                    <HubButton
                        label="BeeHub"
                        is_active={
                            let selected = selected_hub.clone();
                            move || selected.get().as_deref() == Some("beehub")
                        }
                        on_click={
                            let selected = selected_hub.clone();
                            let reload = reload_skills.clone();
                            move || { selected.set(Some("beehub".to_string())); reload(); }
                        }
                    />
                </div>

                // === Search Bar with Button (P2) ===
                <div class="search-bar">
                    <input
                        type="text"
                        placeholder="Search skills..."
                        prop:value=search_input
                        on:input=move |e| search_input.set(event_target_value(&e))
                        on:keyup=move |e| {
                            if e.key() == "Enter" {
                                perform_search();
                            }
                        }
                    />
                    <button class="search-btn" on:click=move |_| perform_search()>
                        "🔍 Search"
                    </button>
                </div>

                <div class="category-filters">
                    <CategoryFilter
                        label="All"
                        is_active={
                            let selected = selected_category;
                            move || selected.get().is_none()
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(None)
                        }
                    />
                    <CategoryFilter
                        label="Trading"
                        is_active={
                            let selected = selected_category;
                            move || selected.get() == Some(SkillCategory::Trading)
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(Some(SkillCategory::Trading))
                        }
                    />
                    <CategoryFilter
                        label="Data"
                        is_active={
                            let selected = selected_category;
                            move || selected.get() == Some(SkillCategory::Data)
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(Some(SkillCategory::Data))
                        }
                    />
                    <CategoryFilter
                        label="Social"
                        is_active={
                            let selected = selected_category;
                            move || selected.get() == Some(SkillCategory::Social)
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(Some(SkillCategory::Social))
                        }
                    />
                    <CategoryFilter
                        label="Automation"
                        is_active={
                            let selected = selected_category;
                            move || selected.get() == Some(SkillCategory::Automation)
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(Some(SkillCategory::Automation))
                        }
                    />
                    <CategoryFilter
                        label="Analysis"
                        is_active={
                            let selected = selected_category;
                            move || selected.get() == Some(SkillCategory::Analysis)
                        }
                        on_click={
                            let selected = selected_category;
                            move || selected.set(Some(SkillCategory::Analysis))
                        }
                    />
                </div>
            </section>

            <Suspense fallback=|| view! { <SkillsLoading/> }>
                {move || {
                    Suspend::new(async move {
                        match skills.await {
                            Ok(data) => {
                                let filtered: Vec<_> = data.into_iter()
                                    .filter(|s| {
                                        let matches_category = selected_category.with(|c| {
                                            c.as_ref().map(|cat| {
                                                let tag = format!("{:?}", cat).to_lowercase();
                                                s.tags.iter().any(|t| t.to_lowercase() == tag) ||
                                                s.capabilities.iter().any(|cap| cap.to_lowercase().contains(&tag))
                                            }).unwrap_or(true)
                                        });
                                        matches_category
                                    })
                                    .collect();

                                if filtered.is_empty() {
                                    view! { <SkillsEmpty/> }.into_any()
                                } else {
                                    view! {
                                        <SkillsGrid skills=filtered reload=reload_skills.clone() selected_hub=selected_hub.clone() on_show_details=move |s| show_details.set(Some(s))/>
                                    }.into_any()
                                }
                            }
                            Err(e) => view! { <SkillsError message=e.to_string()/> }.into_any(),
                        }
                    })
                }}
            </Suspense>

            // === Skill Detail Modal (P2) ===
            {move || show_details.get().map(|skill| {
                view! {
                    <SkillDetailModal skill=skill on_close=move || show_details.set(None)/>
                }
            })}
        </div>
    }
}

#[component]
fn HubButton(
    #[prop(into)] label: String,
    is_active: impl Fn() -> bool + Clone + Send + Sync + 'static,
    on_click: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <button
            class=move || format!("hub-btn {}", if is_active() { "active" } else { "" })
            on:click=move |_| on_click()
        >
            {label}
        </button>
    }
}

#[component]
fn CategoryFilter(
    #[prop(into)] label: String,
    is_active: impl Fn() -> bool + Clone + Send + Sync + 'static,
    on_click: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <button
            class=move || format!("category-filter {}", if is_active() { "active" } else { "" })
            on:click=move |_| on_click()
        >
            {label}
        </button>
    }
}

#[component]
fn SkillsGrid(
    skills: Vec<SkillInfo>,
    reload: impl Fn() + Clone + Send + Sync + 'static,
    selected_hub: RwSignal<Option<String>>,
    on_show_details: impl Fn(SkillInfo) + Clone + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <div class="skills-grid">
            {skills.into_iter().map(|skill| {
                view! {
                    <SkillCard skill=skill reload=reload.clone() selected_hub=selected_hub.clone() on_show_details=on_show_details.clone()/>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

#[component]
fn SkillCard(
    #[prop(into)] skill: SkillInfo,
    reload: impl Fn() + Clone + Send + Sync + 'static,
    selected_hub: RwSignal<Option<String>>,
    on_show_details: impl Fn(SkillInfo) + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let app_state = use_app_state();
    let is_installing = RwSignal::new(false);
    let is_uninstalling = RwSignal::new(false);

    let skill_sig = RwSignal::new(skill);
    let is_installed = move || skill_sig.get().installed;

    let category_icon = {
        let skill = skill_sig.get();
        if skill.tags.iter().any(|t| t.to_lowercase() == "trading")
            || skill
                .capabilities
                .iter()
                .any(|c| c.to_lowercase().contains("trade"))
        {
            "📈"
        } else if skill.tags.iter().any(|t| t.to_lowercase() == "data")
            || skill
                .capabilities
                .iter()
                .any(|c| c.to_lowercase().contains("data"))
        {
            "📊"
        } else if skill.tags.iter().any(|t| t.to_lowercase() == "social")
            || skill
                .capabilities
                .iter()
                .any(|c| c.to_lowercase().contains("social"))
        {
            "💬"
        } else if skill.tags.iter().any(|t| t.to_lowercase() == "automation")
            || skill
                .capabilities
                .iter()
                .any(|c| c.to_lowercase().contains("auto"))
        {
            "⚙️"
        } else if skill.tags.iter().any(|t| t.to_lowercase() == "analysis")
            || skill
                .capabilities
                .iter()
                .any(|c| c.to_lowercase().contains("analy"))
        {
            "🔍"
        } else {
            "📦"
        }
    };

    view! {
        <div class="card skill-card">
            <div class="skill-header">
                <div class="skill-icon">{category_icon}</div>
                <div class="skill-meta">
                    <h3>{skill_sig.get().name.clone()}</h3>
                    <div class="skill-stats">
                        <span class="skill-version">{format!("v{}", skill_sig.get().version)}</span>
                        {move || {
                            let s = skill_sig.get();
                            if s.downloads > 0 || s.rating > 0.0 {
                                view! {
                                    <span class="skill-popularity">
                                        {format!("{} downloads · ", s.downloads)}<StarRating rating=s.rating />{format!(" {}  ", s.rating)}
                                    </span>
                                }.into_any()
                            } else {
                                view! { <></> }.into_any()
                            }
                        }}
                        <span class="skill-tags">
                            {skill_sig.get().tags.first().cloned().unwrap_or_default()}
                        </span>
                    </div>
                </div>
                {move || if is_installed() {
                    view! {
                        <span class="installed-badge">"✓ Installed"</span>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }}
            </div>

            <p class="skill-description">{skill_sig.get().description.clone()}</p>

            <div class="skill-footer">
                <span class="skill-author">{format!("by {}", skill_sig.get().author)}</span>
                <div class="skill-actions">
                    <button
                        class="btn btn-secondary btn-sm"
                        on:click={
                            let skill = skill_sig.get();
                            move |_| on_show_details(skill.clone())
                        }
                    >
                        "Details"
                    </button>
                    {move || if is_installed() {
                        let app_state = app_state.clone();
                        let skill = skill_sig.get();
                        let reload = reload.clone();
                        view! {
                            <button
                                class="btn btn-danger btn-sm"
                                disabled=move || is_uninstalling.get()
                                on:click=move |_| {
                                    is_uninstalling.set(true);
                                    let service = app_state.skill_service();
                                    let app_state = app_state.clone();
                                    let skill_name = skill.name.clone();
                                    let skill_id = skill.id.clone();
                                    let reload = reload.clone();
                                    leptos::task::spawn_local(async move {
                                        match service.uninstall(&skill_id).await {
                                            Ok(()) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Success,
                                                    "Skill Uninstalled",
                                                    format!("{} removed successfully", skill_name),
                                                );
                                                reload();
                                            }
                                            Err(e) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Error,
                                                    "Uninstall Failed",
                                                    format!("Failed to uninstall {}: {}", skill_name, e),
                                                );
                                            }
                                        }
                                        is_uninstalling.set(false);
                                    });
                                }
                            >
                                {move || if is_uninstalling.get() {
                                    "Removing..."
                                } else {
                                    "Uninstall"
                                }}
                            </button>
                        }.into_any()
                    } else {
                        let app_state = app_state.clone();
                        let skill = skill_sig.get();
                        let reload = reload.clone();
                        view! {
                            <button
                                class="btn btn-primary btn-sm"
                                disabled=move || is_installing.get()
                                on:click=move |_| {
                                    is_installing.set(true);
                                    let service = app_state.skill_service();
                                    let app_state = app_state.clone();
                                    let skill_name = skill.name.clone();
                                    let skill_id = skill.id.clone();
                                    let reload = reload.clone();
                                    leptos::task::spawn_local(async move {
                                        let req = InstallSkillRequest {
                                            source: skill_id.clone(),
                                            agent_id: None,
                                            version: None,
                                            hub: selected_hub.get().filter(|h| !h.is_empty()),
                                        };
                                        match service.install(req).await {
                                            Ok(resp) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Success,
                                                    "Skill Installed",
                                                    format!("{} installed successfully", resp.name),
                                                );
                                                reload();
                                            }
                                            Err(e) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Error,
                                                    "Install Failed",
                                                    format!("Failed to install {}: {}", skill_name, e),
                                                );
                                            }
                                        }
                                        is_installing.set(false);
                                    });
                                }
                            >
                                {move || if is_installing.get() {
                                    "Installing..."
                                } else {
                                    "Install"
                                }}
                            </button>
                        }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}

// === Skill Detail Modal (P2) ===
#[component]
fn SkillDetailModal(
    #[prop(into)] skill: SkillInfo,
    on_close: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <Modal title=skill.name.clone() on_close=move || on_close()>
            <div class="modal-body">
                    <div class="detail-row">
                        <span class="detail-label">"Version:"</span>
                        <span class="detail-value">{format!("v{}", skill.version)}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">"Author:"</span>
                        <span class="detail-value">{skill.author.clone()}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">"License:"</span>
                        <span class="detail-value">{skill.license.clone()}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">"Downloads:"</span>
                        <span class="detail-value">{skill.downloads.to_string()}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">"Rating:"</span>
                        <span class="detail-value"><StarRating rating=skill.rating />{format!(" {}  ", skill.rating)}</span>
                    </div>
                    <div class="detail-section">
                        <span class="detail-label">"Description:"</span>
                        <p class="detail-description">{skill.description.clone()}</p>
                    </div>
                    <div class="detail-section">
                        <span class="detail-label">"Capabilities:"</span>
                        <div class="detail-tags">
                            {if skill.capabilities.is_empty() {
                                view! { <span class="tag empty">"None listed"</span> }.into_any()
                            } else {
                                skill.capabilities.iter().map(|c| {
                                    view! { <span class="tag capability">{c.clone()}</span> }
                                }).collect::<Vec<_>>().into_any()
                            }}
                        </div>
                    </div>
                    <div class="detail-section">
                        <span class="detail-label">"Tags:"</span>
                        <div class="detail-tags">
                            {if skill.tags.is_empty() {
                                view! { <span class="tag empty">"None listed"</span> }.into_any()
                            } else {
                                skill.tags.iter().map(|t| {
                                    view! { <span class="tag">{t.clone()}</span> }
                                }).collect::<Vec<_>>().into_any()
                            }}
                        </div>
                    </div>
            </div>
        </Modal>
    }
}

#[component]
fn SkillsLoading() -> impl IntoView {
    view! {
        <div class="skills-grid">
            <div class="card skill-card skeleton">
                <div class="skeleton-header"></div>
                <div class="skeleton-line"></div>
                <div class="skeleton-line"></div>
            </div>
            <div class="card skill-card skeleton">
                <div class="skeleton-header"></div>
                <div class="skeleton-line"></div>
                <div class="skeleton-line"></div>
            </div>
            <div class="card skill-card skeleton">
                <div class="skeleton-header"></div>
                <div class="skeleton-line"></div>
                <div class="skeleton-line"></div>
            </div>
            <div class="card skill-card skeleton">
                <div class="skeleton-header"></div>
                <div class="skeleton-line"></div>
                <div class="skeleton-line"></div>
            </div>
        </div>
    }
}

#[component]
fn SkillsEmpty() -> impl IntoView {
    view! {
        <div class="empty-state">
            <div class="empty-icon">"📦"</div>
            <h3>"No skills found"</h3>
            <p>"Try adjusting your search or filters"</p>
        </div>
    }
}

#[component]
fn SkillsError(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <div class="error-state">
            <div class="error-icon">"⚠️"</div>
            <h3>"Failed to load skills"</h3>
            <p>{message}</p>
            <button
                class="btn btn-primary"
                on:click=move |_| { let _ = window().location().reload(); }
            >
                "Retry"
            </button>
        </div>
    }
}
