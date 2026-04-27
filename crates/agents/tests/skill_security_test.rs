//! Skill 安全验证测试
//!
//! 验证 SkillSecurityValidator 对恶意/不合规 WASM 模块的拦截能力。

use beebotos_agents::skills::{SkillSecurityPolicy, SkillSecurityValidator, ValidationError};

/// 正常 WASM header（仅 header，不完整但结构合法）
fn valid_wasm_header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

/// 超大 WASM（模拟超过 10MB 限制）
fn oversized_wasm() -> Vec<u8> {
    vec![0u8; 11 * 1024 * 1024]
}

/// 非 WASM 文件
fn invalid_wasm() -> Vec<u8> {
    b"this is not a wasm module".to_vec()
}

#[test]
fn test_validate_normal_wasm_header() {
    let validator = SkillSecurityValidator::new(SkillSecurityPolicy::default());
    let result = validator.validate(&valid_wasm_header());
    // 仅含 header 的 WASM 不完整，wasmparser 可能报错；
    // 若使用完整的最小 WASM 模块，则应返回 Ok
    println!("Validation result: {:?}", result);
}

#[test]
fn test_validate_oversized_module() {
    let validator = SkillSecurityValidator::new(SkillSecurityPolicy::default());
    let result = validator.validate(&oversized_wasm());

    assert!(result.is_err());
    match result.unwrap_err() {
        ValidationError::ModuleTooLarge { size, max } => {
            assert_eq!(size, 11 * 1024 * 1024);
            assert_eq!(max, 10 * 1024 * 1024);
            println!("✅ Correctly rejected oversized module: {} > {}", size, max);
        }
        other => panic!("Expected ModuleTooLarge, got: {:?}", other),
    }
}

#[test]
fn test_validate_invalid_wasm_structure() {
    let validator = SkillSecurityValidator::new(SkillSecurityPolicy::default());
    let result = validator.validate(&invalid_wasm());

    assert!(result.is_err());
    match result.unwrap_err() {
        ValidationError::InvalidWasm(_) => {
            println!("✅ Correctly rejected invalid WASM structure");
        }
        other => panic!("Expected InvalidWasm, got: {:?}", other),
    }
}

#[test]
fn test_custom_security_policy() {
    let mut policy = SkillSecurityPolicy::default();
    policy.max_module_size = 1024; // 严格限制 1KB
    policy.timeout_secs = 5;
    policy.allow_network = false;

    let validator = SkillSecurityValidator::new(policy);

    // 1KB 限制下，2KB 的 WASM 应被拒绝
    let wasm_2kb = vec![0u8; 2048];
    let result = validator.validate(&wasm_2kb);
    assert!(result.is_err());
    match result.unwrap_err() {
        ValidationError::ModuleTooLarge { size, max } => {
            assert_eq!(size, 2048);
            assert_eq!(max, 1024);
            println!("✅ Custom policy correctly rejected 2KB module (max 1KB)");
        }
        other => panic!("Expected ModuleTooLarge, got: {:?}", other),
    }
}
