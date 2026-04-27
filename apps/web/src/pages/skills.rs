//! Skills Marketplace Page
//!
//! Browse, install, and manage WASM skills from ClawHub/BeeHub or local
//! registry.

use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;

use crate::api::{InstallSkillRequest, SkillCategory, SkillInfo};
use crate::components::{Modal, StarRating};
use crate::i18n::I18nContext;
use crate::state::use_app_state;

#[component]
pub fn SkillsPage() -> impl IntoView {
    let app_state = use_app_state();
    let search_input = RwSignal::new(String::new());
    let active_search = RwSignal::new(String::new());
    let selected_hub = RwSignal::new(None::<String>);
    let selected_category = RwSignal::new(None::<SkillCategory>);
    let show_details = RwSignal::new(None::<SkillInfo>);
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

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
        <Title text={move || i18n_stored.get_value().t("skills-page-title")} />
        <div class="page skills-page">
            <div class="page-header">
                <div>
                    <h1>{move || i18n_stored.get_value().t("skills-title")}</h1>
                    <p class="page-description">{move || i18n_stored.get_value().t("skills-subtitle")}</p>
                </div>
            </div>

            <section class="skills-controls">
                // === Hub Selector (P2) ===
                <div class="hub-selector">
                    <span class="hub-label">{move || i18n_stored.get_value().t("skills-source")}</span>
                    <HubButton
                        label=i18n_stored.get_value().t("skills-source-local")
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
                        label=i18n_stored.get_value().t("skills-source-clawhub")
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
                        label=i18n_stored.get_value().t("skills-source-beehub")
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
                        placeholder={move || i18n_stored.get_value().t("skills-search-placeholder")}
                        prop:value=search_input
                        on:input=move |e| search_input.set(event_target_value(&e))
                        on:keyup=move |e| {
                            if e.key() == "Enter" {
                                perform_search();
                            }
                        }
                    />
                    <button class="search-btn" on:click=move |_| perform_search()>
                        {move || i18n_stored.get_value().t("skills-search-btn")}
                    </button>
                </div>

                <div class="category-filters">
                    <CategoryFilter
                        label=i18n_stored.get_value().t("skills-cat-all")
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
                        label=i18n_stored.get_value().t("skills-cat-trading")
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
                        label=i18n_stored.get_value().t("skills-cat-data")
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
                        label=i18n_stored.get_value().t("skills-cat-social")
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
                        label=i18n_stored.get_value().t("skills-cat-automation")
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
                        label=i18n_stored.get_value().t("skills-cat-analysis")
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
                                    view! { <SkillsEmpty hub=selected_hub.get() search=active_search.get() i18n=i18n_stored.get_value()/> }.into_any()
                                } else {
                                    view! {
                                        <SkillsGrid skills=filtered reload=reload_skills.clone() selected_hub=selected_hub.clone() on_show_details=move |s| show_details.set(Some(s)) i18n=i18n_stored.get_value()/>
                                    }.into_any()
                                }
                            }
                            Err(e) => view! { <SkillsError message=e.to_string() i18n=i18n_stored.get_value()/> }.into_any(),
                        }
                    })
                }}
            </Suspense>

            // === Skill Detail Modal (P2) ===
            {move || show_details.get().map(|skill| {
                view! {
                    <SkillDetailModal skill=skill on_close=move || show_details.set(None) i18n=i18n_stored.get_value()/>
                }
            })}
        </div>
    }
}

#[component]
fn HubButton(
    label: String,
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
    label: String,
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
    i18n: I18nContext,
) -> impl IntoView {
    view! {
        <div class="skills-grid">
            {skills.into_iter().map(|skill| {
                view! {
                    <SkillCard skill=skill reload=reload.clone() selected_hub=selected_hub.clone() on_show_details=on_show_details.clone() i18n=i18n.clone()/>
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
    i18n: I18nContext,
) -> impl IntoView {
    let app_state = use_app_state();
    let is_installing = RwSignal::new(false);
    let is_uninstalling = RwSignal::new(false);
    let i18n_stored = StoredValue::new(i18n);

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

    let category_class = match category_icon {
        "📈" => "category-trading",
        "📊" => "category-data",
        "💬" => "category-social",
        "⚙️" => "category-automation",
        "🔍" => "category-analysis",
        _ => "",
    };

    let card_class = if category_class.is_empty() {
        "card skill-card".to_string()
    } else {
        format!("card skill-card {}", category_class)
    };

    view! {
        <div class=card_class>
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
                        <span class="installed-badge">{move || i18n_stored.get_value().t("skills-installed-badge")}</span>
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
                        {move || i18n_stored.get_value().t("skills-btn-details")}
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
                                                    i18n_stored.get_value().t("skills-uninstall-success-title"),
                                                    format!("{} {}", skill_name, i18n_stored.get_value().t("skills-uninstall-success-msg")),
                                                );
                                                reload();
                                            }
                                            Err(e) => {
                                                app_state.notify(
                                                    crate::state::notification::NotificationType::Error,
                                                    i18n_stored.get_value().t("skills-uninstall-fail-title"),
                                                    format!("{} {}: {}", i18n_stored.get_value().t("skills-uninstall-fail-msg"), skill_name, e),
                                                );
                                            }
                                        }
                                        is_uninstalling.set(false);
                                    });
                                }
                            >
                                {move || if is_uninstalling.get() {
                                    i18n_stored.get_value().t("skills-removing")
                                } else {
                                    i18n_stored.get_value().t("skills-btn-uninstall")
                                }}
                            </button>
                        }.into_any()
                    } else {
                        let hub = selected_hub.get();
                        if hub.as_deref() == Some("clawhub") || hub.as_deref() == Some("beehub") {
                            let skill_id = skill_sig.get().id.clone();
                            let hub_url = match hub.as_deref() {
                                Some("clawhub") => format!("https://clawhub.ai/skills/{}", skill_id),
                                Some("beehub") => format!("https://beehub.io/skills/{}", skill_id),
                                _ => String::new(),
                            };
                            let app_state = app_state.clone();
                            let skill = skill_sig.get();
                            let reload = reload.clone();
                            view! {
                                <>
                                    <a
                                        class="btn btn-primary btn-sm"
                                        href=hub_url
                                        target="_blank"
                                    >
                                        {move || i18n_stored.get_value().t("skills-btn-view-hub")}
                                    </a>
                                    <button
                                        class="btn btn-success btn-sm"
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
                                                            i18n_stored.get_value().t("skills-install-success-title"),
                                                            format!("{} {}", resp.name, i18n_stored.get_value().t("skills-install-success-msg")),
                                                        );
                                                        reload();
                                                    }
                                                    Err(e) => {
                                                        app_state.notify(
                                                            crate::state::notification::NotificationType::Error,
                                                            i18n_stored.get_value().t("skills-install-fail-title"),
                                                            format!("{} {}: {}", i18n_stored.get_value().t("skills-install-fail-msg"), skill_name, e),
                                                        );
                                                    }
                                                }
                                                is_installing.set(false);
                                            });
                                        }
                                    >
                                        {move || if is_installing.get() {
                                            i18n_stored.get_value().t("skills-installing")
                                        } else {
                                            i18n_stored.get_value().t("skills-btn-install")
                                        }}
                                    </button>
                                </>
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
                                                        i18n_stored.get_value().t("skills-install-success-title"),
                                                        format!("{} {}", resp.name, i18n_stored.get_value().t("skills-install-success-msg")),
                                                    );
                                                    reload();
                                                }
                                                Err(e) => {
                                                    app_state.notify(
                                                        crate::state::notification::NotificationType::Error,
                                                        i18n_stored.get_value().t("skills-install-fail-title"),
                                                        format!("{} {}: {}", i18n_stored.get_value().t("skills-install-fail-msg"), skill_name, e),
                                                    );
                                                }
                                            }
                                            is_installing.set(false);
                                        });
                                    }
                                >
                                    {move || if is_installing.get() {
                                        i18n_stored.get_value().t("skills-installing")
                                    } else {
                                        i18n_stored.get_value().t("skills-btn-install")
                                    }}
                                </button>
                            }.into_any()
                        }
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
    i18n: I18nContext,
) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    view! {
        <Modal title=skill.name.clone() on_close=move || on_close()>
            <div class="modal-body">
                    <div class="detail-row">
                        <span class="detail-label">{move || i18n_stored.get_value().t("skills-detail-version")}</span>
                        <span class="detail-value">{format!("v{}", skill.version)}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">{move || i18n_stored.get_value().t("skills-detail-author")}</span>
                        <span class="detail-value">{skill.author.clone()}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">{move || i18n_stored.get_value().t("skills-detail-license")}</span>
                        <span class="detail-value">{skill.license.clone()}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">{move || i18n_stored.get_value().t("skills-detail-downloads")}</span>
                        <span class="detail-value">{skill.downloads.to_string()}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">{move || i18n_stored.get_value().t("skills-detail-rating")}</span>
                        <span class="detail-value"><StarRating rating=skill.rating />{format!(" {}  ", skill.rating)}</span>
                    </div>
                    <div class="detail-section">
                        <span class="detail-label">{move || i18n_stored.get_value().t("skills-detail-description")}</span>
                        <p class="detail-description">{skill.description.clone()}</p>
                    </div>
                    <div class="detail-section">
                        <span class="detail-label">{move || i18n_stored.get_value().t("skills-detail-capabilities")}</span>
                        <div class="detail-tags">
                            {if skill.capabilities.is_empty() {
                                view! { <span class="tag empty">{move || i18n_stored.get_value().t("skills-detail-none")}</span> }.into_any()
                            } else {
                                skill.capabilities.iter().map(|c| {
                                    view! { <span class="tag capability">{c.clone()}</span> }
                                }).collect::<Vec<_>>().into_any()
                            }}
                        </div>
                    </div>
                    <div class="detail-section">
                        <span class="detail-label">{move || i18n_stored.get_value().t("skills-detail-tags")}</span>
                        <div class="detail-tags">
                            {if skill.tags.is_empty() {
                                view! { <span class="tag empty">{move || i18n_stored.get_value().t("skills-detail-none")}</span> }.into_any()
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
fn SkillsEmpty(
    #[prop(default = None)] hub: Option<String>,
    #[prop(default = String::new())] search: String,
    i18n: I18nContext,
) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    view! {
        <div class="empty-state">
            <div class="empty-icon">"📦"</div>
            {match hub {
                Some(ref h) if search.is_empty() => view! {
                    <>
                        <h3>{format!("{}", i18n_stored.get_value().t("skills-empty-search").replace("{}", h))}</h3>
                        <p>{move || i18n_stored.get_value().t("skills-empty-search-desc")}</p>
                    </>
                }.into_any(),
                Some(ref h) => view! {
                    <>
                        <h3>{format!("{}", i18n_stored.get_value().t("skills-empty-noresults").replace("{}", h))}</h3>
                        <p>{move || i18n_stored.get_value().t("skills-empty-noresults-desc")}</p>
                    </>
                }.into_any(),
                None => view! {
                    <>
                        <h3>{move || i18n_stored.get_value().t("skills-empty-none")}</h3>
                        <p>{move || i18n_stored.get_value().t("skills-empty-none-desc")}</p>
                    </>
                }.into_any(),
            }}
        </div>
    }
}

#[component]
fn SkillsError(#[prop(into)] message: String, i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    let is_hub_unavailable = message.contains("502") || message.contains("503") || message.contains("unavailable");
    view! {
        <div class="error-state">
            <div class="error-icon">"⚠️"</div>
            <h3>{move || i18n_stored.get_value().t("skills-error-title")}</h3>
            {if is_hub_unavailable {
                view! {
                    <>
                        <p>{move || i18n_stored.get_value().t("skills-error-unavailable")}</p>
                        <p class="text-muted">{move || i18n_stored.get_value().t("skills-error-unavailable-hint")}</p>
                    </>
                }.into_any()
            } else {
                view! { <p>{message}</p> }.into_any()
            }}
            <button
                class="btn btn-primary"
                on:click=move |_| {
                    let window = web_sys::window().expect("window not available");
                    let _ = window.location().reload();
                }
            >
                {move || i18n_stored.get_value().t("skills-error-retry")}
            </button>
        </div>
    }
}
