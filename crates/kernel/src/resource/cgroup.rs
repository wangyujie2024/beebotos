//! Control Groups (CGroups)
//!
//! Resource grouping and hierarchical resource management.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{ResourceLimits, ResourceUsage};

/// Control group for resource management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CGroup {
    /// Group name/path
    pub name: String,
    /// Parent group name
    pub parent: Option<String>,
    /// Resource limits for this group
    pub limits: ResourceLimits,
    /// Processes in this group
    pub processes: Vec<u32>,
    /// Child group names
    pub children: Vec<String>,
    /// Enabled resource controllers
    pub controllers: Vec<Controller>,
}

/// Resource controller types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Controller {
    /// CPU time controller
    Cpu,
    /// Memory controller
    Memory,
    /// IO controller
    Io,
    /// Network controller
    Network,
    /// Process ID controller
    Pids,
}

impl CGroup {
    /// Create new cgroup with default controllers
    pub fn new(name: String) -> Self {
        Self {
            name,
            parent: None,
            limits: ResourceLimits::none(),
            processes: vec![],
            children: vec![],
            controllers: vec![Controller::Cpu, Controller::Memory, Controller::Pids],
        }
    }

    /// Set parent cgroup
    pub fn with_parent(mut self, parent: String) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Set resource limits
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Add process to cgroup
    pub fn add_process(&mut self, pid: u32) {
        if !self.processes.contains(&pid) {
            self.processes.push(pid);
        }
    }

    /// Remove process from cgroup
    pub fn remove_process(&mut self, pid: u32) {
        self.processes.retain(|&p| p != pid);
    }

    /// Add child cgroup
    pub fn add_child(&mut self, child: String) {
        if !self.children.contains(&child) {
            self.children.push(child);
        }
    }

    /// Remove child cgroup
    pub fn remove_child(&mut self, child: &str) {
        self.children.retain(|c| c != child);
    }

    /// Enable resource controller
    pub fn enable_controller(&mut self, controller: Controller) {
        if !self.controllers.contains(&controller) {
            self.controllers.push(controller);
        }
    }

    /// Disable resource controller
    pub fn disable_controller(&mut self, controller: Controller) {
        self.controllers.retain(|&c| c != controller);
    }
}

/// Manages cgroup hierarchy
pub struct CGroupManager {
    groups: HashMap<String, CGroup>,
    hierarchy: HashMap<String, Vec<String>>,
}

impl CGroupManager {
    /// Create new cgroup manager with root group
    pub fn new() -> Self {
        let mut manager = Self {
            groups: HashMap::new(),
            hierarchy: HashMap::new(),
        };

        let root = CGroup::new("/".to_string());
        manager.groups.insert("/".to_string(), root);
        manager.hierarchy.insert("/".to_string(), vec![]);

        manager
    }

    /// Create new cgroup under parent
    pub fn create_group(
        &mut self,
        name: &str,
        parent: Option<&str>,
    ) -> Result<&CGroup, CGroupError> {
        if self.groups.contains_key(name) {
            return Err(CGroupError::AlreadyExists);
        }

        let parent_name = parent.unwrap_or("/");
        if !self.groups.contains_key(parent_name) {
            return Err(CGroupError::ParentNotFound);
        }

        let mut group = CGroup::new(name.to_string()).with_parent(parent_name.to_string());

        if let Some(parent_group) = self.groups.get(parent_name) {
            group.limits = parent_group.limits.clone();
        }

        self.groups.insert(name.to_string(), group);

        if let Some(parent_group) = self.groups.get_mut(parent_name) {
            parent_group.add_child(name.to_string());
        }

        self.hierarchy.insert(name.to_string(), vec![]);

        self.groups
            .get(name)
            .ok_or_else(|| CGroupError::NotFound(name.to_string()))
    }

    /// Delete cgroup and remove from parent
    pub fn delete_group(&mut self, name: &str) -> Result<(), CGroupError> {
        if name == "/" {
            return Err(CGroupError::CannotDeleteRoot);
        }

        // Get parent name first, then release borrow
        let parent_name = {
            let group = self
                .groups
                .get(name)
                .ok_or_else(|| CGroupError::NotFound(name.to_string()))?;

            if !group.children.is_empty() {
                return Err(CGroupError::HasChildren);
            }

            group.parent.clone()
        }; // Immutable borrow released here

        if let Some(ref parent_name) = parent_name {
            if let Some(parent) = self.groups.get_mut(parent_name) {
                parent.remove_child(name);
            }
        }

        self.groups.remove(name);
        self.hierarchy.remove(name);

        Ok(())
    }

    /// Get reference to cgroup
    pub fn get_group(&self, name: &str) -> Option<&CGroup> {
        self.groups.get(name)
    }

    /// Get mutable reference to cgroup
    pub fn get_group_mut(&mut self, name: &str) -> Option<&mut CGroup> {
        self.groups.get_mut(name)
    }

    /// Move process between cgroups
    pub fn move_process(&mut self, pid: u32, from: &str, to: &str) -> Result<(), CGroupError> {
        let from_group = self
            .groups
            .get_mut(from)
            .ok_or_else(|| CGroupError::NotFound(from.to_string()))?;
        from_group.remove_process(pid);

        let to_group = self
            .groups
            .get_mut(to)
            .ok_or_else(|| CGroupError::NotFound(to.to_string()))?;
        to_group.add_process(pid);

        Ok(())
    }

    /// Calculate aggregate resource usage for cgroup
    pub fn get_group_usage(
        &self,
        name: &str,
        process_usage: &HashMap<u32, ResourceUsage>,
    ) -> ResourceUsage {
        let mut total = ResourceUsage::new();

        if let Some(group) = self.groups.get(name) {
            for &pid in &group.processes {
                if let Some(usage) = process_usage.get(&pid) {
                    total.add(usage);
                }
            }

            for child_name in &group.children {
                let child_usage = self.get_group_usage(child_name, process_usage);
                total.add(&child_usage);
            }
        }

        total
    }

    /// List all cgroup names
    pub fn list_groups(&self) -> Vec<&str> {
        self.groups.keys().map(|s| s.as_str()).collect()
    }

    /// Get cgroup hierarchy as list
    pub fn get_hierarchy(&self, name: &str) -> Vec<&CGroup> {
        let mut result = vec![];

        if let Some(group) = self.groups.get(name) {
            result.push(group);

            for child_name in &group.children {
                result.extend(self.get_hierarchy(child_name));
            }
        }

        result
    }
}

/// Cgroup operation errors
#[derive(Debug, Clone)]
pub enum CGroupError {
    /// Group already exists
    AlreadyExists,
    /// Group not found
    NotFound(String),
    /// Parent group not found
    ParentNotFound,
    /// Group has children
    HasChildren,
    /// Cannot delete root group
    CannotDeleteRoot,
    /// Invalid group name
    InvalidName,
}

impl std::fmt::Display for CGroupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CGroupError::AlreadyExists => write!(f, "CGroup already exists"),
            CGroupError::NotFound(name) => write!(f, "CGroup not found: {}", name),
            CGroupError::ParentNotFound => write!(f, "Parent cgroup not found"),
            CGroupError::HasChildren => write!(f, "CGroup has children"),
            CGroupError::CannotDeleteRoot => write!(f, "Cannot delete root cgroup"),
            CGroupError::InvalidName => write!(f, "Invalid cgroup name"),
        }
    }
}

impl std::error::Error for CGroupError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cgroup_create() {
        let mut manager = CGroupManager::new();
        let group = manager.create_group("/apps", None).unwrap();

        assert_eq!(group.name, "/apps");
        assert_eq!(group.parent, Some("/".to_string()));
    }

    #[test]
    fn test_cgroup_hierarchy() {
        let mut manager = CGroupManager::new();
        manager.create_group("/apps", None).unwrap();
        manager.create_group("/apps/web", Some("/apps")).unwrap();

        let apps = manager.get_group("/apps").unwrap();
        assert!(apps.children.contains(&"/apps/web".to_string()));
    }
}
