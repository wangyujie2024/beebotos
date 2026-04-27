use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;
use leptos_router::components::A;

use crate::api::{CreateProposalRequest, DaoSummary, ProposalInfo, ProposalStatus};
use crate::components::Modal;
use crate::state::notification::NotificationType;
use crate::state::use_app_state;

#[component]
pub fn DaoPage() -> impl IntoView {
    let app_state = use_app_state();
    let app_state_clone1 = app_state.clone();
    let app_state_clone2 = app_state.clone();

    // Fetch DAO summary - use LocalResource for CSR
    let dao_summary = LocalResource::new(move || {
        let service = app_state_clone1.dao_service();
        let loading = app_state_clone1.loading();
        async move {
            loading.dao.set(true);
            let result = service.get_summary().await;
            loading.dao.set(false);
            result
        }
    });

    // Fetch proposals
    let proposals = LocalResource::new(move || {
        let service = app_state_clone2.dao_service();
        async move { service.list_proposals().await }
    });

    // Create proposal modal state
    let create_open = RwSignal::new(false);
    let create_title = RwSignal::new(String::new());
    let create_desc = RwSignal::new(String::new());
    let create_type = RwSignal::new("general".to_string());
    let create_saving = RwSignal::new(false);
    let create_error = RwSignal::new(None::<String>);

    let on_create = move || {
        let req = CreateProposalRequest {
            title: create_title.get(),
            description: create_desc.get(),
            proposal_type: create_type.get(),
        };
        create_saving.set(true);
        create_error.set(None);
        let service = app_state.dao_service();
        spawn_local(async move {
            match service.create_proposal(req).await {
                Ok(_) => {
                    create_saving.set(false);
                    create_open.set(false);
                    create_title.set(String::new());
                    create_desc.set(String::new());
                    proposals.refetch();
                }
                Err(e) => {
                    create_saving.set(false);
                    create_error.set(Some(format!("Failed to create proposal: {}", e)));
                }
            }
        });
    };

    view! {
        <Title text="DAO Governance - BeeBotOS" />
        <div class="page dao-page">
            <div class="page-header">
                <div>
                    <h1>"DAO Governance"</h1>
                    <p class="page-description">"Participate in community-driven decision making"</p>
                </div>
                <A href="/dao/treasury" attr:class="btn btn-secondary">
                    "View Treasury →"
                </A>
            </div>

            <Suspense fallback=|| view! { <DaoSummaryLoading/> }>
                {move || Suspend::new(async move {
                    match dao_summary.await {
                        Ok(data) => view! { <DaoSummaryView summary=data/> }.into_any(),
                        Err(_) => view! { <DaoSummaryPlaceholder/> }.into_any(),
                    }
                })}
            </Suspense>

            <section class="proposals-section">
                <div class="section-header">
                    <h2>"Governance Proposals"</h2>
                    <button class="btn btn-primary" on:click=move |_| create_open.set(true)>"+ New Proposal"</button>
                </div>

                // Create Proposal Modal
                {move || {
                    let on_create = on_create.clone();
                    if create_open.get() {
                    view! {
                        <Modal title="Create Proposal" on_close=move || create_open.set(false)>
                            <div class="modal-body">
                                {move || create_error.get().map(|msg| view! {
                                    <div class="alert alert-error">{msg}</div>
                                })}
                                <div class="form-group">
                                    <label>"Title"</label>
                                    <input
                                        type="text"
                                        prop:value=create_title
                                        on:input=move |e| create_title.set(event_target_value(&e))
                                        placeholder="Proposal title"
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"Description"</label>
                                    <textarea
                                        prop:value=create_desc
                                        on:input=move |e| create_desc.set(event_target_value(&e))
                                        placeholder="Describe your proposal..."
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"Type"</label>
                                    <select
                                        prop:value=create_type
                                        on:change=move |e| create_type.set(event_target_value(&e))
                                    >
                                        <option value="general">"General"</option>
                                        <option value="funding">"Funding"</option>
                                        <option value="upgrade">"Upgrade"</option>
                                        <option value="parameter">"Parameter"</option>
                                    </select>
                                </div>
                            </div>
                            <div class="modal-footer">
                                <button class="btn btn-secondary" on:click=move |_| create_open.set(false)>"Cancel"</button>
                                <button
                                    class="btn btn-primary"
                                    on:click={
                                        let on_create = on_create.clone();
                                        move |_| on_create()
                                    }
                                    disabled=create_saving
                                >
                                    {move || if create_saving.get() { "Creating..." } else { "Create Proposal" }}
                                </button>
                            </div>
                        </Modal>
                    }.into_any()
                } else {
                    ().into_any()
                }
                }}

                <Suspense fallback=|| view! { <ProposalsLoading/> }>
                    {move || Suspend::new(async move {
                        match proposals.await {
                            Ok(data) => {
                                if data.is_empty() {
                                    view! { <ProposalsEmpty/> }.into_any()
                                } else {
                                    view! { <ProposalsList proposals=data/> }.into_any()
                                }
                            }
                            Err(e) => view! {
                                <div class="error-message">
                                    {"Failed to load proposals: "}{e.to_string()}
                                </div>
                            }.into_any(),
                        }
                    })}
                </Suspense>
            </section>
        </div>
    }
}

#[component]
fn DaoSummaryView(summary: DaoSummary) -> impl IntoView {
    view! {
        <section class="dao-summary">
            <div class="stat-card">
                <div class="stat-value">{summary.member_count}</div>
                <div class="stat-label">"DAO Members"</div>
            </div>
            <div class="stat-card">
                <div class="stat-value">{summary.active_proposals}</div>
                <div class="stat-label">"Active Proposals"</div>
            </div>
            <div class="stat-card">
                <div class="stat-value">{summary.user_voting_power}</div>
                <div class="stat-label">
                    {format!("Your Voting Power ({})", summary.token_symbol)}
                </div>
            </div>
            <div class="stat-card">
                <div class="stat-value">{summary.token_balance}</div>
                <div class="stat-label">
                    {format!("Your Balance ({})", summary.token_symbol)}
                </div>
            </div>
        </section>
    }
}

#[component]
fn DaoSummaryPlaceholder() -> impl IntoView {
    view! {
        <section class="dao-summary">
            <div class="stat-card skeleton">
                <div class="stat-value">"-"</div>
                <div class="stat-label">"DAO Members"</div>
            </div>
            <div class="stat-card skeleton">
                <div class="stat-value">"-"</div>
                <div class="stat-label">"Active Proposals"</div>
            </div>
            <div class="stat-card skeleton">
                <div class="stat-value">"-"</div>
                <div class="stat-label">"Your Voting Power"</div>
            </div>
            <div class="stat-card skeleton">
                <div class="stat-value">"-"</div>
                <div class="stat-label">"Your Balance"</div>
            </div>
        </section>
    }
}

#[component]
fn DaoSummaryLoading() -> impl IntoView {
    view! { <DaoSummaryPlaceholder/> }
}

#[component]
fn ProposalsList(proposals: Vec<ProposalInfo>) -> impl IntoView {
    let (active, other): (Vec<_>, Vec<_>) = proposals
        .into_iter()
        .partition(|p| matches!(p.status, ProposalStatus::Active));

    view! {
        <div class="proposals-container">
            {move || if !active.is_empty() {
                view! {
                    <div class="proposals-group">
                        <h3>"Active Proposals"</h3>
                        <div class="proposals-list">
                            {active.clone().into_iter().map(|p| view! { <ProposalCard proposal=p/> }).collect::<Vec<_>>()}
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}

            {move || if !other.is_empty() {
                view! {
                    <div class="proposals-group">
                        <h3>"Past Proposals"</h3>
                        <div class="proposals-list">
                            {other.clone().into_iter().map(|p| view! { <ProposalCard proposal=p/> }).collect::<Vec<_>>()}
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}
        </div>
    }
}

#[component]
fn ProposalCard(#[prop(into)] proposal: ProposalInfo) -> impl IntoView {
    let _app_state = use_app_state();
    let status_class = match proposal.status {
        ProposalStatus::Active => "status-active",
        ProposalStatus::Passed => "status-passed",
        ProposalStatus::Rejected => "status-rejected",
        ProposalStatus::Executed => "status-executed",
        ProposalStatus::Pending => "status-pending",
    };

    // Store proposal data in signals to avoid clone issues
    let proposal_id = proposal.id.clone();
    let proposal_id_for = proposal.id.clone();
    let proposal_id_against = proposal.id.clone();
    let votes_for = proposal.votes_for;
    let votes_against = proposal.votes_against;
    let is_active = proposal.status == ProposalStatus::Active;

    // Signal to track voting state (to prevent double voting)
    let user_voted = RwSignal::new(proposal.user_voted);
    let is_voting = RwSignal::new(false);

    view! {
        <div class="card proposal-card">
            <div class="proposal-header">
                <div class="proposal-title">
                    <h4>{proposal.title.clone()}</h4>
                    <span class=format!("status-badge {}", status_class)>
                        {format!("{:?}", proposal.status)}
                    </span>
                </div>
                <div class="proposal-meta">
                    <span>"By "{proposal.proposer.clone()}</span>
                    <span>"Ends: "{proposal.ends_at.clone()}</span>
                </div>
            </div>

            <p class="proposal-description">{proposal.description.clone()}</p>

            <ProposalVotingSection
                _proposal_id={proposal_id}
                proposal_id_for={proposal_id_for}
                proposal_id_against={proposal_id_against}
                votes_for={votes_for}
                votes_against={votes_against}
                is_active={is_active}
                user_voted={user_voted}
                is_voting={is_voting}
            />
        </div>
    }
}

#[component]
fn ProposalVotingSection(
    _proposal_id: String,
    proposal_id_for: String,
    proposal_id_against: String,
    votes_for: u64,
    votes_against: u64,
    is_active: bool,
    user_voted: RwSignal<Option<bool>>,
    is_voting: RwSignal<bool>,
) -> impl IntoView {
    let _app_state = use_app_state();
    let total_votes = votes_for + votes_against;
    let for_percent = if total_votes > 0 {
        (votes_for as f64 / total_votes as f64) * 100.0
    } else {
        0.0
    };

    view! {
        <div class={if is_active { "proposal-voting" } else { "proposal-results" }}>
            <div class="vote-bar">
                <div
                    class="vote-bar-for"
                    style={format!("width: {}%", for_percent)}
                ></div>
            </div>
            <div class="vote-stats">
                <span>{format!("{} For", votes_for)}</span>
                <span>{format!("{} Against", votes_against)}</span>
            </div>

            {move || {
                let voted = user_voted.get();
                if is_active {
                    if voted.is_none() {
                        view! {
                            <div class="vote-actions">
                                <VoteButton
                                    proposal_id={proposal_id_for.clone()}
                                    vote_for={true}
                                    is_voting={is_voting}
                                    user_voted={user_voted}
                                />
                                <VoteButton
                                    proposal_id={proposal_id_against.clone()}
                                    vote_for={false}
                                    is_voting={is_voting}
                                    user_voted={user_voted}
                                />
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="voted-badge">
                                {if voted == Some(true) {
                                    "✓ You voted For".into_any()
                                } else {
                                    "✓ You voted Against".into_any()
                                }}
                            </div>
                        }.into_any()
                    }
                } else {
                    view! { <></> }.into_any()
                }
            }}
        </div>
    }
}

#[component]
fn VoteButton(
    proposal_id: String,
    vote_for: bool,
    is_voting: RwSignal<bool>,
    user_voted: RwSignal<Option<bool>>,
) -> impl IntoView {
    let app_state = use_app_state();
    let btn_class = if vote_for {
        "btn btn-success"
    } else {
        "btn btn-danger"
    };
    let label = if vote_for { "Vote For" } else { "Vote Against" };

    view! {
        <button
            class={btn_class}
            disabled=is_voting
            on:click={
                let proposal_id = proposal_id.clone();
                move |_| {
                    let app_state = app_state.clone();
                    let proposal_id = proposal_id.clone();
                    is_voting.set(true);
                    spawn_local(async move {
                        let dao_service = app_state.dao_service();
                        match dao_service.vote(&proposal_id, vote_for, 1).await {
                            Ok(_) => {
                                user_voted.set(Some(vote_for));
                                app_state.notify(
                                    NotificationType::Success,
                                    "Vote Submitted",
                                    "Your vote has been recorded successfully"
                                );
                                dao_service.invalidate_proposals_cache();
                            }
                            Err(e) => {
                                app_state.notify(
                                    NotificationType::Error,
                                    "Vote Failed",
                                    format!("Failed to submit vote: {}", e)
                                );
                            }
                        }
                        is_voting.set(false);
                    });
                }
            }
        >
            {move || if is_voting.get() {
                if vote_for { "Voting..." } else { "Voting..." }
            } else {
                label
            }}
        </button>
    }
}

#[component]
fn ProposalsLoading() -> impl IntoView {
    view! {
        <div class="proposals-list">
            <div class="card proposal-card skeleton">
                <div class="skeleton-header"></div>
                <div class="skeleton-line"></div>
            </div>
            <div class="card proposal-card skeleton">
                <div class="skeleton-header"></div>
                <div class="skeleton-line"></div>
            </div>
        </div>
    }
}

#[component]
fn ProposalsEmpty() -> impl IntoView {
    view! {
        <div class="empty-state">
            <div class="empty-icon">"🏛️"</div>
            <h3>"No proposals yet"</h3>
            <p>"Be the first to create a governance proposal"</p>
        </div>
    }
}
