//! DAO domain state management

use leptos::prelude::*;

use crate::api::{DaoSummary, ProposalInfo, ProposalStatus};

/// DAO domain state
#[derive(Clone, Debug)]
pub struct DaoState {
    /// DAO summary info
    pub summary: RwSignal<Option<DaoSummary>>,
    /// List of proposals
    pub proposals: RwSignal<Vec<ProposalInfo>>,
    /// Selected proposal for detail view
    pub selected_proposal: RwSignal<Option<ProposalInfo>>,
    /// Loading states
    pub is_summary_loading: RwSignal<bool>,
    pub is_proposals_loading: RwSignal<bool>,
    pub is_voting: RwSignal<bool>,
    /// Error state
    pub error: RwSignal<Option<String>>,
    /// Proposal filters
    pub filters: RwSignal<ProposalFilters>,
    /// User's voting history
    pub voting_history: RwSignal<Vec<VoteRecord>>,
}

#[derive(Clone, Debug, Default)]
pub struct ProposalFilters {
    pub status_filter: Option<ProposalStatus>,
    pub search_query: String,
    pub show_only_user_votes: bool,
}

#[derive(Clone, Debug)]
pub struct VoteRecord {
    pub proposal_id: String,
    pub vote_for: bool,
    pub voting_power: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl DaoState {
    pub fn new() -> Self {
        Self {
            summary: RwSignal::new(None),
            proposals: RwSignal::new(Vec::new()),
            selected_proposal: RwSignal::new(None),
            is_summary_loading: RwSignal::new(false),
            is_proposals_loading: RwSignal::new(false),
            is_voting: RwSignal::new(false),
            error: RwSignal::new(None),
            filters: RwSignal::new(ProposalFilters::default()),
            voting_history: RwSignal::new(Vec::new()),
        }
    }

    /// Get filtered proposals
    pub fn get_filtered_proposals(&self) -> Vec<ProposalInfo> {
        let proposals = self.proposals.get();
        let filters = self.filters.get();

        proposals
            .into_iter()
            .filter(|p| {
                // Status filter
                if let Some(status) = &filters.status_filter {
                    p.status == *status
                } else {
                    true
                }
            })
            .filter(|p| {
                // Search filter
                if filters.search_query.is_empty() {
                    true
                } else {
                    let query = filters.search_query.to_lowercase();
                    p.title.to_lowercase().contains(&query)
                        || p.description.to_lowercase().contains(&query)
                }
            })
            .filter(|p| {
                // User votes filter
                if filters.show_only_user_votes {
                    p.user_voted.is_some()
                } else {
                    true
                }
            })
            .collect()
    }

    /// Get active proposals count
    pub fn active_proposals_count(&self) -> usize {
        self.proposals.with(|p| {
            p.iter()
                .filter(|prop| matches!(prop.status, ProposalStatus::Active))
                .count()
        })
    }

    /// Update proposal after vote
    pub fn record_vote(&self, proposal_id: String, vote_for: bool, voting_power: u64) {
        // Update proposal in list
        self.proposals.update(|proposals| {
            if let Some(prop) = proposals.iter_mut().find(|p| p.id == proposal_id) {
                prop.user_voted = Some(vote_for);
                if vote_for {
                    prop.votes_for += voting_power;
                } else {
                    prop.votes_against += voting_power;
                }
            }
        });

        // Update selected proposal if matches
        self.selected_proposal.update(|selected| {
            if let Some(prop) = selected {
                if prop.id == proposal_id {
                    prop.user_voted = Some(vote_for);
                    if vote_for {
                        prop.votes_for += voting_power;
                    } else {
                        prop.votes_against += voting_power;
                    }
                }
            }
        });

        // Add to voting history
        self.voting_history.update(|history| {
            history.push(VoteRecord {
                proposal_id,
                vote_for,
                voting_power,
                timestamp: chrono::Utc::now(),
            });
        });
    }

    /// Get proposals grouped by status
    pub fn get_proposals_by_status(&self) -> (Vec<ProposalInfo>, Vec<ProposalInfo>) {
        let filtered = self.get_filtered_proposals();
        filtered
            .into_iter()
            .partition(|p| matches!(p.status, ProposalStatus::Active))
    }

    /// Update proposal status (e.g., after execution)
    pub fn update_proposal_status(&self, proposal_id: &str, new_status: ProposalStatus) {
        self.proposals.update(|proposals| {
            if let Some(prop) = proposals.iter_mut().find(|p| p.id == proposal_id) {
                prop.status = new_status.clone();
            }
        });

        self.selected_proposal.update(|selected| {
            if let Some(prop) = selected {
                if prop.id == proposal_id {
                    prop.status = new_status;
                }
            }
        });
    }

    /// Calculate voting progress percentage
    pub fn get_voting_progress(&self, proposal_id: &str) -> Option<f64> {
        self.proposals.with(|proposals| {
            proposals.iter().find(|p| p.id == proposal_id).map(|p| {
                let total = p.votes_for + p.votes_against;
                if total == 0 {
                    0.0
                } else {
                    (p.votes_for as f64 / total as f64) * 100.0
                }
            })
        })
    }
}

impl Default for DaoState {
    fn default() -> Self {
        Self::new()
    }
}

/// Provide DAO state
pub fn provide_dao_state() {
    provide_context(DaoState::new());
}

/// Use DAO state
pub fn use_dao_state() -> DaoState {
    use_context::<DaoState>().expect("DaoState not provided")
}
