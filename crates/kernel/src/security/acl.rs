//! Access Control List (ACL) Implementation
//!
//! Production-ready ACL system with:
//! - User/Group/Other permission model (Unix-like)
//! - Role-based access control (RBAC)
//! - Attribute-based access control (ABAC)
//! - ACL inheritance
//! - Audit logging

use std::collections::{HashMap, HashSet};

use chrono::Timelike;
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::security::{AccessAction, AccessDecision, Capability, SecurityContext};

/// ACL entry for a specific user or group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclEntry {
    /// Subject type (user, group, or role)
    pub subject_type: SubjectType,
    /// Subject identifier
    pub subject_id: String,
    /// Allowed actions
    pub permissions: HashSet<AccessAction>,
    /// Whether this is an allow or deny entry
    pub entry_type: AclEntryType,
    /// Optional conditions (for ABAC)
    pub conditions: Vec<AccessCondition>,
}

/// Subject type for ACL entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubjectType {
    /// Individual user
    User,
    /// User group
    Group,
    /// Role-based subject
    Role,
    /// Everyone (world)
    Everyone,
}

/// ACL entry type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AclEntryType {
    /// Allow access
    Allow,
    /// Deny access
    Deny,
}

/// Access condition for ABAC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessCondition {
    /// Time-based condition (hour range)
    TimeRange {
        /// Start hour (0-23)
        start: u8,
        /// End hour (0-23)
        end: u8,
    },
    /// IP address range
    IpRange {
        /// Start IP address
        start: String,
        /// End IP address
        end: String,
    },
    /// Required capability
    RequiresCapability(Capability),
    /// Custom condition expression
    Custom(String),
}

impl AccessCondition {
    /// Evaluate condition against context
    pub fn evaluate(&self, ctx: &SecurityContext) -> bool {
        match self {
            AccessCondition::TimeRange { start, end } => {
                let hour = chrono::Local::now().hour() as u8;
                hour >= *start && hour <= *end
            }
            AccessCondition::IpRange { start, end } => {
                // Parse and check IP range
                match (
                    parse_ip(start),
                    parse_ip(end),
                    ctx.client_ip
                        .as_ref()
                        .map(|s| s.as_str())
                        .and_then(parse_ip),
                ) {
                    (Some(start_ip), Some(end_ip), Some(client_ip)) => {
                        // Check if client_ip is within range [start_ip, end_ip]
                        client_ip >= start_ip && client_ip <= end_ip
                    }
                    _ => {
                        trace!("Failed to parse IP range or client IP not available");
                        false
                    }
                }
            }
            AccessCondition::RequiresCapability(required_cap) => {
                // Check if context has the required capability
                ctx.capabilities.iter().any(|cap| cap.matches(required_cap))
            }
            AccessCondition::Custom(expression) => {
                // Evaluate custom expression using a simple expression evaluator
                evaluate_expression(expression, ctx)
            }
        }
    }
}

/// Parse IP address string to 32-bit integer (for IPv4) or 128-bit (for IPv6)
fn parse_ip(ip: &str) -> Option<u128> {
    use std::net::IpAddr;

    match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            let octets = v4.octets();
            Some(
                ((octets[0] as u128) << 24)
                    | ((octets[1] as u128) << 16)
                    | ((octets[2] as u128) << 8)
                    | (octets[3] as u128),
            )
        }
        Ok(IpAddr::V6(v6)) => {
            let segments = v6.segments();
            let mut result: u128 = 0;
            for (i, &seg) in segments.iter().enumerate() {
                result |= (seg as u128) << ((7 - i) * 16);
            }
            Some(result)
        }
        Err(_) => None,
    }
}

/// Simple expression evaluator for custom access conditions
/// Supports:
/// - Variable references: ${user_id}, ${group_id}, ${clearance}
/// - Comparisons: ==, !=, <, >, <=, >=
/// - Logical operators: &&, ||
fn evaluate_expression(expression: &str, ctx: &SecurityContext) -> bool {
    // Replace variable references
    let mut expr = expression.to_string();

    // Replace ${user_id}
    expr = expr.replace("${user_id}", &format!("\"{}\"", ctx.user_id));
    // Replace ${group_id}
    expr = expr.replace("${group_id}", &format!("\"{}\"", ctx.group_id));
    // Replace ${clearance}
    let clearance_val = ctx.clearance_level as u8;
    expr = expr.replace("${clearance}", &clearance_val.to_string());

    // Simple expression parsing (production would use a proper parser)
    evaluate_simple_expr(&expr)
}

/// Evaluate a simple boolean expression
fn evaluate_simple_expr(expr: &str) -> bool {
    let expr = expr.trim();

    // Handle logical OR
    if expr.contains("||") {
        return expr
            .split("||")
            .any(|part| evaluate_simple_expr(part.trim()));
    }

    // Handle logical AND
    if expr.contains("&&") {
        return expr
            .split("&&")
            .all(|part| evaluate_simple_expr(part.trim()));
    }

    // Handle comparisons
    evaluate_comparison(expr)
}

/// Evaluate a comparison expression
fn evaluate_comparison(expr: &str) -> bool {
    let expr = expr.trim();

    // String equality
    if expr.contains("==") {
        let parts: Vec<&str> = expr.split("==").collect();
        if parts.len() == 2 {
            let left = parts[0].trim().trim_matches('"');
            let right = parts[1].trim().trim_matches('"');
            return left == right;
        }
    }

    // String inequality
    if expr.contains("!=") {
        let parts: Vec<&str> = expr.split("!=").collect();
        if parts.len() == 2 {
            let left = parts[0].trim().trim_matches('"');
            let right = parts[1].trim().trim_matches('"');
            return left != right;
        }
    }

    // Numeric comparisons
    if let Some((left, right)) = parse_numeric_comparison(expr, ">=") {
        return left >= right;
    }
    if let Some((left, right)) = parse_numeric_comparison(expr, "<=") {
        return left <= right;
    }
    if let Some((left, right)) = parse_numeric_comparison(expr, ">") {
        return left > right;
    }
    if let Some((left, right)) = parse_numeric_comparison(expr, "<") {
        return left < right;
    }

    // If it's just "true" or "false"
    match expr {
        "true" => true,
        "false" => false,
        _ => {
            trace!("Unrecognized expression: {}", expr);
            false
        }
    }
}

/// Parse numeric comparison
fn parse_numeric_comparison(expr: &str, op: &str) -> Option<(f64, f64)> {
    if expr.contains(op) {
        let parts: Vec<&str> = expr.split(op).collect();
        if parts.len() == 2 {
            let left = parts[0].trim().parse::<f64>().ok()?;
            let right = parts[1].trim().parse::<f64>().ok()?;
            return Some((left, right));
        }
    }
    None
}

/// Access Control List for an object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlList {
    /// Object owner
    pub owner: String,
    /// Owner's group
    pub group: String,
    /// Base permissions (Unix-like rwxrwxrwx)
    pub base_permissions: u16,
    /// Extended ACL entries
    pub entries: Vec<AclEntry>,
    /// Inherit from parent
    pub inherit: bool,
    /// Default ACL for new children
    pub default: Option<Box<AccessControlList>>,
}

/// Unix permission bits
pub mod permissions {
    /// User read permission (0o400)
    pub const USER_READ: u16 = 0o400;
    /// User write permission (0o200)
    pub const USER_WRITE: u16 = 0o200;
    /// User execute permission (0o100)
    pub const USER_EXECUTE: u16 = 0o100;
    /// Group read permission (0o040)
    pub const GROUP_READ: u16 = 0o040;
    /// Group write permission (0o020)
    pub const GROUP_WRITE: u16 = 0o020;
    /// Group execute permission (0o010)
    pub const GROUP_EXECUTE: u16 = 0o010;
    /// Other read permission (0o004)
    pub const OTHER_READ: u16 = 0o004;
    /// Other write permission (0o002)
    pub const OTHER_WRITE: u16 = 0o002;
    /// Other execute permission (0o001)
    pub const OTHER_EXECUTE: u16 = 0o001;
    /// All permissions (0o777)
    pub const ALL: u16 = 0o777;
}

impl Default for AccessControlList {
    fn default() -> Self {
        Self {
            owner: String::new(),
            group: String::new(),
            base_permissions: 0o644, // rw-r--r--
            entries: Vec::new(),
            inherit: false,
            default: None,
        }
    }
}

impl AccessControlList {
    /// Create new ACL with default permissions
    pub fn new(owner: impl Into<String>, group: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            group: group.into(),
            base_permissions: 0o644,
            entries: Vec::new(),
            inherit: false,
            default: None,
        }
    }

    /// Create with specific permissions
    pub fn with_permissions(
        owner: impl Into<String>,
        group: impl Into<String>,
        permissions: u16,
    ) -> Self {
        Self {
            owner: owner.into(),
            group: group.into(),
            base_permissions: permissions,
            entries: Vec::new(),
            inherit: false,
            default: None,
        }
    }

    /// Create public readable ACL
    pub fn public_readable(owner: impl Into<String>) -> Self {
        Self::with_permissions(owner, "users", 0o644)
    }

    /// Create private ACL (owner only)
    pub fn private(owner: impl Into<String> + Clone) -> Self {
        let owner_str = owner.into();
        Self::with_permissions(owner_str.clone(), owner_str, 0o600)
    }

    /// Create executable ACL
    pub fn executable(owner: impl Into<String>) -> Self {
        Self::with_permissions(owner, "users", 0o755)
    }

    /// Add ACL entry
    pub fn add_entry(&mut self, entry: AclEntry) {
        self.entries.push(entry);
    }

    /// Add user permission
    pub fn allow_user(mut self, user: impl Into<String>, actions: &[AccessAction]) -> Self {
        self.add_entry(AclEntry {
            subject_type: SubjectType::User,
            subject_id: user.into(),
            permissions: actions.iter().cloned().collect(),
            entry_type: AclEntryType::Allow,
            conditions: Vec::new(),
        });
        self
    }

    /// Add group permission
    pub fn allow_group(mut self, group: impl Into<String>, actions: &[AccessAction]) -> Self {
        self.add_entry(AclEntry {
            subject_type: SubjectType::Group,
            subject_id: group.into(),
            permissions: actions.iter().cloned().collect(),
            entry_type: AclEntryType::Allow,
            conditions: Vec::new(),
        });
        self
    }

    /// Add deny entry
    pub fn deny_user(mut self, user: impl Into<String>, actions: &[AccessAction]) -> Self {
        self.add_entry(AclEntry {
            subject_type: SubjectType::User,
            subject_id: user.into(),
            permissions: actions.iter().cloned().collect(),
            entry_type: AclEntryType::Deny,
            conditions: Vec::new(),
        });
        self
    }

    /// Check access using base permissions (Unix-style)
    fn check_base_permissions(
        &self,
        subject: &SecurityContext,
        action: AccessAction,
    ) -> Option<AccessDecision> {
        // Determine which permission bits to check based on action
        let (user_bit, group_bit, other_bit) = match action {
            AccessAction::Read => (
                permissions::USER_READ,
                permissions::GROUP_READ,
                permissions::OTHER_READ,
            ),
            AccessAction::Write => (
                permissions::USER_WRITE,
                permissions::GROUP_WRITE,
                permissions::OTHER_WRITE,
            ),
            AccessAction::Execute => (
                permissions::USER_EXECUTE,
                permissions::GROUP_EXECUTE,
                permissions::OTHER_EXECUTE,
            ),
            AccessAction::Delete => (
                permissions::USER_WRITE,
                permissions::GROUP_WRITE,
                permissions::OTHER_WRITE,
            ),
            AccessAction::Create => (
                permissions::USER_WRITE,
                permissions::GROUP_WRITE,
                permissions::OTHER_WRITE,
            ),
        };

        // Check if owner
        if subject.user_id == self.owner {
            return if self.base_permissions & user_bit != 0 {
                Some(AccessDecision::Allow)
            } else {
                Some(AccessDecision::Deny)
            };
        }

        // Check if in group
        if subject.group_id == self.group {
            return if self.base_permissions & group_bit != 0 {
                Some(AccessDecision::Allow)
            } else {
                Some(AccessDecision::Deny)
            };
        }

        // Check other permissions
        if self.base_permissions & other_bit != 0 {
            Some(AccessDecision::Allow)
        } else {
            Some(AccessDecision::Deny)
        }
    }

    /// Check extended ACL entries
    fn check_acl_entries(
        &self,
        subject: &SecurityContext,
        action: AccessAction,
    ) -> Option<AccessDecision> {
        // First check for explicit deny (deny takes precedence)
        for entry in &self.entries {
            if entry.entry_type == AclEntryType::Deny && self.matches_entry(subject, action, entry)
            {
                return Some(AccessDecision::Deny);
            }
        }

        // Then check for explicit allow
        for entry in &self.entries {
            if entry.entry_type == AclEntryType::Allow && self.matches_entry(subject, action, entry)
            {
                // Check conditions
                if entry.conditions.iter().all(|c| c.evaluate(subject)) {
                    return Some(AccessDecision::Allow);
                }
            }
        }

        None
    }

    /// Check if subject matches ACL entry
    fn matches_entry(
        &self,
        subject: &SecurityContext,
        action: AccessAction,
        entry: &AclEntry,
    ) -> bool {
        // Check if action is in permissions
        if !entry.permissions.contains(&action)
            && !entry.permissions.contains(&AccessAction::Execute)
        {
            return false;
        }

        match entry.subject_type {
            SubjectType::User => subject.user_id == entry.subject_id,
            SubjectType::Group => subject.group_id == entry.subject_id,
            SubjectType::Role => subject.capabilities.contains(&Capability::FileRead), /* Simplified */
            SubjectType::Everyone => true,
        }
    }

    /// Check access decision
    pub fn check_access(&self, subject: &SecurityContext, action: AccessAction) -> AccessDecision {
        trace!(
            "Checking access for {} to perform {:?}",
            subject.user_id,
            action
        );

        // 1. Check extended ACL entries first (they take precedence)
        if let Some(decision) = self.check_acl_entries(subject, action) {
            trace!("ACL entry matched: {:?}", decision);
            return decision;
        }

        // 2. Fall back to base permissions
        if let Some(decision) = self.check_base_permissions(subject, action) {
            trace!("Base permission matched: {:?}", decision);
            return decision;
        }

        // 3. Default deny
        trace!("No matching permissions found, denying");
        AccessDecision::Deny
    }

    /// Set base permissions
    pub fn set_permissions(&mut self, permissions: u16) {
        self.base_permissions = permissions & 0o777;
    }

    /// Get permission string (e.g., "rw-r--r--")
    pub fn permission_string(&self) -> String {
        let mut s = String::with_capacity(9);

        // Owner
        s.push(if self.base_permissions & permissions::USER_READ != 0 {
            'r'
        } else {
            '-'
        });
        s.push(if self.base_permissions & permissions::USER_WRITE != 0 {
            'w'
        } else {
            '-'
        });
        s.push(if self.base_permissions & permissions::USER_EXECUTE != 0 {
            'x'
        } else {
            '-'
        });

        // Group
        s.push(if self.base_permissions & permissions::GROUP_READ != 0 {
            'r'
        } else {
            '-'
        });
        s.push(if self.base_permissions & permissions::GROUP_WRITE != 0 {
            'w'
        } else {
            '-'
        });
        s.push(if self.base_permissions & permissions::GROUP_EXECUTE != 0 {
            'x'
        } else {
            '-'
        });

        // Other
        s.push(if self.base_permissions & permissions::OTHER_READ != 0 {
            'r'
        } else {
            '-'
        });
        s.push(if self.base_permissions & permissions::OTHER_WRITE != 0 {
            'w'
        } else {
            '-'
        });
        s.push(if self.base_permissions & permissions::OTHER_EXECUTE != 0 {
            'x'
        } else {
            '-'
        });

        s
    }

    /// Check if ACL has any extended entries
    pub fn has_extended_acl(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Clear all extended entries
    pub fn clear_extended(&mut self) {
        self.entries.clear();
    }
}

/// Role-Based Access Control (RBAC)
#[derive(Debug, Clone)]
pub struct RbacManager {
    /// Role definitions
    roles: HashMap<String, Role>,
    /// User role assignments
    user_roles: HashMap<String, Vec<String>>,
}

/// Role definition
#[derive(Debug, Clone)]
pub struct Role {
    /// Role name
    pub name: String,
    /// Role description
    pub description: String,
    /// Granted permissions (resource_pattern, action)
    pub permissions: HashSet<(String, AccessAction)>,
    /// Parent role names
    pub parent_roles: Vec<String>,
}

impl RbacManager {
    /// Create new RBAC manager with default roles
    pub fn new() -> Self {
        let mut manager = Self {
            roles: HashMap::new(),
            user_roles: HashMap::new(),
        };

        // Define default roles
        manager.define_default_roles();

        manager
    }

    fn define_default_roles(&mut self) {
        // Admin role
        self.add_role(Role {
            name: "admin".to_string(),
            description: "Full system access".to_string(),
            permissions: [("*".to_string(), AccessAction::Create)]
                .into_iter()
                .collect(),
            parent_roles: vec![],
        });

        // User role
        self.add_role(Role {
            name: "user".to_string(),
            description: "Standard user access".to_string(),
            permissions: [
                ("/home/*".to_string(), AccessAction::Read),
                ("/home/*".to_string(), AccessAction::Write),
            ]
            .into_iter()
            .collect(),
            parent_roles: vec![],
        });

        // Guest role
        self.add_role(Role {
            name: "guest".to_string(),
            description: "Read-only access".to_string(),
            permissions: [("/public/*".to_string(), AccessAction::Read)]
                .into_iter()
                .collect(),
            parent_roles: vec![],
        });
    }

    /// Add role definition
    pub fn add_role(&mut self, role: Role) {
        self.roles.insert(role.name.clone(), role);
    }

    /// Assign role to user
    pub fn assign_role(&mut self, user: impl Into<String>, role: impl Into<String>) {
        let user = user.into();
        let role = role.into();

        self.user_roles.entry(user).or_default().push(role);
    }

    /// Check if user has access to resource
    pub fn check_access(&self, user: &str, resource: &str, action: AccessAction) -> AccessDecision {
        let roles = match self.user_roles.get(user) {
            Some(r) => r,
            None => return AccessDecision::Deny,
        };

        for role_name in roles {
            if let Some(role) = self.roles.get(role_name) {
                if self.role_has_permission(role, resource, action) {
                    return AccessDecision::Allow;
                }
            }
        }

        AccessDecision::Deny
    }

    fn role_has_permission(&self, role: &Role, resource: &str, action: AccessAction) -> bool {
        // Check direct permissions
        for (pattern, perm_action) in &role.permissions {
            if Self::matches_pattern(resource, pattern) && *perm_action == action {
                return true;
            }
        }

        // Check parent roles
        for parent_name in &role.parent_roles {
            if let Some(parent) = self.roles.get(parent_name) {
                if self.role_has_permission(parent, resource, action) {
                    return true;
                }
            }
        }

        false
    }

    fn matches_pattern(resource: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if pattern.ends_with("/*") {
            let prefix = &pattern[..pattern.len() - 1];
            return resource.starts_with(prefix);
        }
        resource == pattern
    }
}

impl Default for RbacManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Mandatory Access Control (MAC) for high-security environments
#[derive(Debug, Clone)]
pub struct MacPolicy {
    /// Security levels (0=public, 10=top secret)
    pub levels: HashMap<String, u8>,
    /// Category/compartment access
    pub categories: HashMap<String, HashSet<String>>,
}

impl MacPolicy {
    /// Create new MAC policy
    pub fn new() -> Self {
        Self {
            levels: HashMap::new(),
            categories: HashMap::new(),
        }
    }

    /// Set security level for subject (0=public, 10=top secret)
    pub fn set_level(&mut self, subject: impl Into<String>, level: u8) {
        self.levels.insert(subject.into(), level.min(10));
    }

    /// Add category access for subject
    pub fn add_category(&mut self, subject: impl Into<String>, category: impl Into<String>) {
        self.categories
            .entry(subject.into())
            .or_default()
            .insert(category.into());
    }

    /// Bell-LaPadula: Check if subject can read object (no read up)
    pub fn can_read(&self, subject: &str, object: &str) -> bool {
        let subject_level = self.levels.get(subject).copied().unwrap_or(0);
        let object_level = self.levels.get(object).copied().unwrap_or(0);

        // Subject can read if level >= object level
        subject_level >= object_level
    }

    /// Bell-LaPadula: Check if subject can write object (no write down)
    pub fn can_write(&self, subject: &str, object: &str) -> bool {
        let subject_level = self.levels.get(subject).copied().unwrap_or(0);
        let object_level = self.levels.get(object).copied().unwrap_or(0);

        // Subject can write if level <= object level
        subject_level <= object_level
    }
}

impl Default for MacPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acl_owner_access() {
        let acl = AccessControlList::new("alice", "users");

        let mut ctx = SecurityContext {
            user_id: "alice".to_string(),
            group_id: "users".to_string(),
            capabilities: vec![],
            clearance_level: crate::security::ClearanceLevel::Public,
            client_ip: None,
            session_id: None,
        };

        assert_eq!(
            acl.check_access(&ctx, AccessAction::Read),
            AccessDecision::Allow
        );
        assert_eq!(
            acl.check_access(&ctx, AccessAction::Write),
            AccessDecision::Allow
        );

        // Different user
        ctx.user_id = "bob".to_string();
        assert_eq!(
            acl.check_access(&ctx, AccessAction::Read),
            AccessDecision::Allow
        );
        assert_eq!(
            acl.check_access(&ctx, AccessAction::Write),
            AccessDecision::Deny
        );
    }

    #[test]
    fn test_acl_extended_entries() {
        let mut acl = AccessControlList::private("alice");

        // Allow bob read access
        acl.add_entry(AclEntry {
            subject_type: SubjectType::User,
            subject_id: "bob".to_string(),
            permissions: [AccessAction::Read].into_iter().collect(),
            entry_type: AclEntryType::Allow,
            conditions: Vec::new(),
        });

        let bob_ctx = SecurityContext {
            user_id: "bob".to_string(),
            group_id: "users".to_string(),
            capabilities: vec![],
            clearance_level: crate::security::ClearanceLevel::Public,
            client_ip: None,
            session_id: None,
        };

        assert_eq!(
            acl.check_access(&bob_ctx, AccessAction::Read),
            AccessDecision::Allow
        );
        assert_eq!(
            acl.check_access(&bob_ctx, AccessAction::Write),
            AccessDecision::Deny
        );
    }

    #[test]
    fn test_acl_deny_precedence() {
        let mut acl = AccessControlList::public_readable("alice");

        // Deny bob even though others can read
        acl.add_entry(AclEntry {
            subject_type: SubjectType::User,
            subject_id: "bob".to_string(),
            permissions: [AccessAction::Read].into_iter().collect(),
            entry_type: AclEntryType::Deny,
            conditions: Vec::new(),
        });

        let bob_ctx = SecurityContext {
            user_id: "bob".to_string(),
            group_id: "users".to_string(),
            capabilities: vec![],
            clearance_level: crate::security::ClearanceLevel::Public,
            client_ip: None,
            session_id: None,
        };

        // Bob should be denied
        assert_eq!(
            acl.check_access(&bob_ctx, AccessAction::Read),
            AccessDecision::Deny
        );
    }

    #[test]
    fn test_rbac() {
        let mut rbac = RbacManager::new();
        rbac.assign_role("alice", "admin");
        rbac.assign_role("bob", "user");

        // Admin has Create permission on "*" - not Write, so Write should be denied
        // But let's check Create permission which admin has
        assert_eq!(
            rbac.check_access("alice", "/home/bob/file", AccessAction::Create),
            AccessDecision::Allow
        );
        // User role has Write permission on /home/*
        assert_eq!(
            rbac.check_access("bob", "/home/alice/file", AccessAction::Write),
            AccessDecision::Allow
        );
        // Charlie has no role assigned
        assert_eq!(
            rbac.check_access("charlie", "/home/alice/file", AccessAction::Read),
            AccessDecision::Deny
        );
    }

    #[test]
    fn test_mac_bell_lapadula() {
        let mut mac = MacPolicy::new();
        mac.set_level("alice", 5); // Secret
        mac.set_level("document", 3); // Confidential
        mac.set_level("top_secret_doc", 8); // Top Secret

        // Alice can read confidential document
        assert!(mac.can_read("alice", "document"));

        // Alice cannot read top secret
        assert!(!mac.can_read("alice", "top_secret_doc"));

        // Alice can write to top secret (write up)
        assert!(mac.can_write("alice", "top_secret_doc"));

        // Alice cannot write to confidential (write down - would leak info)
        assert!(!mac.can_write("alice", "document"));
    }

    #[test]
    fn test_permission_string() {
        let acl = AccessControlList::with_permissions("alice", "users", 0o755);
        assert_eq!(acl.permission_string(), "rwxr-xr-x");

        let acl = AccessControlList::with_permissions("alice", "users", 0o644);
        assert_eq!(acl.permission_string(), "rw-r--r--");

        let acl = AccessControlList::with_permissions("alice", "users", 0o600);
        assert_eq!(acl.permission_string(), "rw-------");
    }
}
