use beebotos_kernel::capabilities::CapabilitySet;
use beebotos_kernel::security::acl::{AccessControlList, AclEntry, AclEntryType, SubjectType};
use beebotos_kernel::security::*;

#[test]
fn test_access_control_list() {
    // AccessControlList::new now takes owner and group
    let mut acl = AccessControlList::new("alice".to_string(), "users".to_string());

    // Add entry using AclEntry struct
    acl.add_entry(AclEntry {
        subject_type: SubjectType::User,
        subject_id: "bob".to_string(),
        permissions: [AccessAction::Read].into_iter().collect(),
        entry_type: AclEntryType::Allow,
        conditions: Vec::new(),
    });

    // check_access expects SecurityContext, not string
    let alice_ctx = SecurityContext {
        user_id: "alice".to_string(),
        group_id: "users".to_string(),
        capabilities: vec![],
        clearance_level: ClearanceLevel::Internal,
        client_ip: None,
        session_id: None,
    };

    let bob_ctx = SecurityContext {
        user_id: "bob".to_string(),
        group_id: "users".to_string(),
        capabilities: vec![],
        clearance_level: ClearanceLevel::Internal,
        client_ip: None,
        session_id: None,
    };

    let charlie_ctx = SecurityContext {
        user_id: "charlie".to_string(),
        group_id: "users".to_string(),
        capabilities: vec![],
        clearance_level: ClearanceLevel::Internal,
        client_ip: None,
        session_id: None,
    };

    assert_eq!(
        acl.check_access(&alice_ctx, AccessAction::Read),
        AccessDecision::Allow
    );

    assert_eq!(
        acl.check_access(&bob_ctx, AccessAction::Read),
        AccessDecision::Allow
    );

    assert_eq!(
        acl.check_access(&bob_ctx, AccessAction::Write),
        AccessDecision::Deny
    );

    // Charlie is neither owner nor in group, but default permissions 0o644 allow
    // others to read
    assert_eq!(
        acl.check_access(&charlie_ctx, AccessAction::Read),
        AccessDecision::Allow
    );
}

#[test]
fn test_acl_permissions() {
    let mut acl = AccessControlList::new("owner".to_string(), "users".to_string());

    // Add entry with multiple permissions
    let mut perms = std::collections::HashSet::new();
    perms.insert(AccessAction::Read);
    perms.insert(AccessAction::Write);

    acl.add_entry(AclEntry {
        subject_type: SubjectType::User,
        subject_id: "user".to_string(),
        permissions: perms,
        entry_type: AclEntryType::Allow,
        conditions: Vec::new(),
    });

    // get_permissions method doesn't exist, verify via check_access instead
    let user_ctx = SecurityContext {
        user_id: "user".to_string(),
        group_id: "users".to_string(),
        capabilities: vec![],
        clearance_level: ClearanceLevel::Internal,
        client_ip: None,
        session_id: None,
    };

    assert_eq!(
        acl.check_access(&user_ctx, AccessAction::Read),
        AccessDecision::Allow
    );
    assert_eq!(
        acl.check_access(&user_ctx, AccessAction::Write),
        AccessDecision::Allow
    );
}

#[test]
fn test_capability_set() {
    use beebotos_kernel::capabilities::CapabilityLevel;

    let caps = CapabilitySet::standard().with_level(CapabilityLevel::L3NetworkOut);

    assert!(caps.has(CapabilityLevel::L1FileRead));
    assert!(caps.has(CapabilityLevel::L3NetworkOut));
    assert!(!caps.has(CapabilityLevel::L10SystemAdmin));
}

#[test]
fn test_capability_permissions() {
    let caps = CapabilitySet::standard().with_permission("custom:action");

    assert!(caps.has_permission("compute"));
    assert!(caps.has_permission("custom:action"));
    assert!(!caps.has_permission("invalid:permission"));
}

#[test]
fn test_security_manager() {
    let mut manager = SecurityManager::new();
    let policy = Box::new(DiscretionaryAccessControl::new());
    manager.register_policy(policy);

    let context = SecurityContext {
        user_id: "alice".to_string(),
        group_id: "users".to_string(),
        capabilities: vec![Capability::FileRead],
        clearance_level: ClearanceLevel::Internal,
        client_ip: None,
        session_id: None,
    };

    // Without proper RBAC setup, access will be denied
    // The test verifies the security manager works end-to-end
    let decision = manager.request_access(&context, "data/tmp/test", AccessAction::Read);
    // Since no RBAC roles are configured, access is denied by default
    assert_eq!(decision, AccessDecision::Deny);
}

#[test]
fn test_audit_log() {
    let log = AuditLog::new();

    let context = SecurityContext {
        user_id: "alice".to_string(),
        group_id: "users".to_string(),
        capabilities: vec![],
        clearance_level: ClearanceLevel::Internal,
        client_ip: None,
        session_id: None,
    };

    log.log_access_attempt(
        &context,
        "data/tmp/test",
        AccessAction::Read,
        AccessDecision::Allow,
    );

    // query takes AuditFilter struct, not individual options
    let filter = AuditFilter {
        subject_id: Some("alice".to_string()),
        object: None,
        start_time_ns: None,
        end_time_ns: None,
        limit: None,
    };
    let entries = log.query(filter).unwrap();
    assert_eq!(entries.len(), 1);
}
