//! Agent domain state management
//!
//! Separated to avoid re-renders when other domains change

use leptos::prelude::*;

use crate::api::{AgentInfo, AgentStatus};

/// Agent domain state
#[derive(Clone, Debug)]
pub struct AgentState {
    /// List of agents
    pub list: RwSignal<Vec<AgentInfo>>,
    /// Selected agent for detail view
    pub selected: RwSignal<Option<AgentInfo>>,
    /// Loading state for agent operations
    pub is_loading: RwSignal<bool>,
    /// Loading state for list specifically
    pub is_list_loading: RwSignal<bool>,
    /// Error state
    pub error: RwSignal<Option<String>>,
    /// Pagination state
    pub pagination: RwSignal<AgentPagination>,
    /// Filters
    pub filters: RwSignal<AgentFilters>,
    /// Real-time status updates (WebSocket simulation)
    pub status_updates: RwSignal<Option<AgentStatusUpdate>>,
}

#[derive(Clone, Debug, Default)]
pub struct AgentPagination {
    pub current_page: usize,
    pub page_size: usize,
    pub total_items: usize,
    pub total_pages: usize,
}

#[derive(Clone, Debug, Default)]
pub struct AgentFilters {
    pub status_filter: Option<AgentStatus>,
    pub search_query: String,
    pub sort_by: AgentSortBy,
    pub sort_order: SortOrder,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum AgentSortBy {
    #[default]
    Name,
    CreatedAt,
    UpdatedAt,
    Status,
    TaskCount,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

#[derive(Clone, Debug)]
pub struct AgentStatusUpdate {
    pub agent_id: String,
    pub new_status: AgentStatus,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl AgentState {
    pub fn new() -> Self {
        Self {
            list: RwSignal::new(Vec::new()),
            selected: RwSignal::new(None),
            is_loading: RwSignal::new(false),
            is_list_loading: RwSignal::new(false),
            error: RwSignal::new(None),
            pagination: RwSignal::new(AgentPagination::default()),
            filters: RwSignal::new(AgentFilters::default()),
            status_updates: RwSignal::new(None),
        }
    }

    /// Get filtered and sorted agents
    pub fn get_filtered_agents(&self) -> Vec<AgentInfo> {
        let agents = self.list.get();
        let filters = self.filters.get();

        let mut result: Vec<_> = agents
            .into_iter()
            .filter(|agent| {
                // Status filter
                if let Some(status) = &filters.status_filter {
                    agent.status == *status
                } else {
                    true
                }
            })
            .filter(|agent| {
                // Search filter
                if filters.search_query.is_empty() {
                    true
                } else {
                    let query = filters.search_query.to_lowercase();
                    agent.name.to_lowercase().contains(&query)
                        || agent.id.to_lowercase().contains(&query)
                        || agent
                            .description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&query))
                            .unwrap_or(false)
                }
            })
            .collect();

        // Sort
        result.sort_by(|a, b| {
            let cmp = match filters.sort_by {
                AgentSortBy::Name => a.name.cmp(&b.name),
                AgentSortBy::CreatedAt => a.created_at.cmp(&b.created_at),
                AgentSortBy::UpdatedAt => a.updated_at.cmp(&b.updated_at),
                AgentSortBy::Status => format!("{:?}", a.status).cmp(&format!("{:?}", b.status)),
                AgentSortBy::TaskCount => a.task_count.cmp(&b.task_count),
            };

            match filters.sort_order {
                SortOrder::Asc => cmp,
                SortOrder::Desc => cmp.reverse(),
            }
        });

        result
    }

    /// Get paginated agents
    pub fn get_paginated_agents(&self) -> Vec<AgentInfo> {
        let filtered = self.get_filtered_agents();
        let pagination = self.pagination.get();

        let start = (pagination.current_page - 1) * pagination.page_size;
        let end = (start + pagination.page_size).min(filtered.len());

        filtered.into_iter().skip(start).take(end - start).collect()
    }

    /// Update agent in list
    pub fn update_agent(&self, updated: AgentInfo) {
        let updated_id = updated.id.clone();
        self.list.update(|agents| {
            if let Some(idx) = agents.iter().position(|a| a.id == updated_id) {
                agents[idx] = updated.clone();
            }
        });

        // Also update selected if matches
        self.selected.update(|selected| {
            if let Some(s) = selected {
                if s.id == updated.id {
                    *selected = Some(updated.clone());
                }
            }
        });
    }

    /// Remove agent from list
    pub fn remove_agent(&self, agent_id: &str) {
        self.list.update(|agents| {
            agents.retain(|a| a.id != agent_id);
        });

        // Clear selected if matches
        self.selected.update(|selected| {
            if let Some(s) = selected {
                if s.id == agent_id {
                    *selected = None;
                }
            }
        });
    }

    /// Add or update agent
    pub fn upsert_agent(&self, agent: AgentInfo) {
        self.list.update(|agents| {
            if let Some(idx) = agents.iter().position(|a| a.id == agent.id) {
                agents[idx] = agent;
            } else {
                agents.push(agent);
            }
        });
    }

    /// Set pagination
    pub fn set_pagination(&self, total: usize) {
        self.pagination.update(|p| {
            p.total_items = total;
            p.total_pages = (total + p.page_size - 1) / p.page_size;
            if p.total_pages == 0 {
                p.total_pages = 1;
            }
        });
    }

    /// Update status (for real-time updates)
    pub fn update_status(&self, agent_id: String, new_status: AgentStatus) {
        self.list.update(|agents| {
            if let Some(agent) = agents.iter_mut().find(|a| a.id == agent_id) {
                agent.status = new_status.clone();
            }
        });

        self.selected.update(|selected| {
            if let Some(agent) = selected {
                if agent.id == agent_id {
                    agent.status = new_status.clone();
                }
            }
        });

        self.status_updates.set(Some(AgentStatusUpdate {
            agent_id,
            new_status,
            timestamp: chrono::Utc::now(),
        }));
    }

    /// Clear error
    pub fn clear_error(&self) {
        self.error.set(None);
    }

    /// Get agent by ID from cache
    pub fn get_agent_by_id(&self, id: &str) -> Option<AgentInfo> {
        self.list
            .with(|agents| agents.iter().find(|a| a.id == id).cloned())
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPagination {
    pub fn with_page_size(page_size: usize) -> Self {
        Self {
            page_size,
            ..Default::default()
        }
    }

    pub fn has_next(&self) -> bool {
        self.current_page < self.total_pages
    }

    pub fn has_prev(&self) -> bool {
        self.current_page > 1
    }

    pub fn next_page(&mut self) {
        if self.has_next() {
            self.current_page += 1;
        }
    }

    pub fn prev_page(&mut self) {
        if self.has_prev() {
            self.current_page -= 1;
        }
    }

    pub fn go_to_page(&mut self, page: usize) {
        self.current_page = page.clamp(1, self.total_pages);
    }
}

/// Provide agent state
pub fn provide_agent_state() {
    provide_context(AgentState::new());
}

/// Use agent state
pub fn use_agent_state() -> AgentState {
    use_context::<AgentState>().expect("AgentState not provided")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_agent(id: &str, name: &str, status: AgentStatus) -> AgentInfo {
        AgentInfo {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            status,
            capabilities: vec![],
            created_at: None,
            updated_at: None,
            task_count: Some(0),
            uptime_percent: None,
        }
    }

    #[test]
    fn test_filter_by_status() {
        // Note: This would need proper signal mocking for full test
        let state = AgentState::new();
        state.list.set(vec![
            create_test_agent("1", "Agent1", AgentStatus::Running),
            create_test_agent("2", "Agent2", AgentStatus::Idle),
        ]);
        state.filters.set(AgentFilters {
            status_filter: Some(AgentStatus::Running),
            ..Default::default()
        });

        let filtered = state.get_filtered_agents();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "1");
    }
}
