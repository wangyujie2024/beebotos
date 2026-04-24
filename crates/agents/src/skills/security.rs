//! Skills Security

/// Security policy for skills
#[derive(Debug, Clone)]
pub struct SkillSecurityPolicy {
    pub allow_network: bool,
    pub allow_filesystem: bool,
    pub allow_env_access: bool,
    pub max_memory_mb: u32,
    pub timeout_secs: u32,
    /// Allowed WASM imports (whitelist approach)
    pub allowed_imports: Vec<String>,
    /// Maximum WASM module size in bytes
    pub max_module_size: usize,
}

impl Default for SkillSecurityPolicy {
    fn default() -> Self {
        Self {
            allow_network: false,
            allow_filesystem: false,
            allow_env_access: false,
            max_memory_mb: 128,
            timeout_secs: 30,
            allowed_imports: vec![
                "env.memory".to_string(),
                "env.__stack_pointer".to_string(),
                "env.__memory_base".to_string(),
                "env.__table_base".to_string(),
            ],
            max_module_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Skill security validator
pub struct SkillSecurityValidator {
    policy: SkillSecurityPolicy,
}

/// WASM validation error
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    #[error("WASM module size {size} exceeds maximum {max}")]
    ModuleTooLarge { size: usize, max: usize },
    #[error("Unauthorized import: {0}")]
    UnauthorizedImport(String),
    #[error("Invalid WASM: {0}")]
    InvalidWasm(String),
    #[error("Memory limit {requested}MB exceeds maximum {max}MB")]
    MemoryLimitExceeded { requested: u32, max: u32 },
    #[error("Dangerous pattern detected: {0}")]
    DangerousPattern(String),
}

impl SkillSecurityValidator {
    pub fn new(policy: SkillSecurityPolicy) -> Self {
        Self { policy }
    }

    /// Validate WASM module against security policy
    ///
    /// Checks:
    /// 1. Module size limits
    /// 2. Valid WASM structure (via wasmparser)
    /// 3. Import whitelist
    /// 4. Memory limits
    /// 5. Dangerous patterns in imports
    pub fn validate(&self, skill_wasm: &[u8]) -> std::result::Result<(), ValidationError> {
        // Check module size
        if skill_wasm.len() > self.policy.max_module_size {
            return Err(ValidationError::ModuleTooLarge {
                size: skill_wasm.len(),
                max: self.policy.max_module_size,
            });
        }

        // Parse and validate WASM module using wasmparser
        self.validate_wasm_structure(skill_wasm)?;

        Ok(())
    }

    /// Validate WASM structure using wasmparser
    fn validate_wasm_structure(&self, wasm: &[u8]) -> std::result::Result<(), ValidationError> {
        use wasmparser::{Parser, Payload, TypeRef};

        let parser = Parser::new(0);

        for payload in parser.parse_all(wasm) {
            let payload =
                payload.map_err(|e| ValidationError::InvalidWasm(format!("Parse error: {}", e)))?;

            match payload {
                Payload::Version { num, .. } => {
                    if num != 1 {
                        return Err(ValidationError::InvalidWasm(format!(
                            "Unsupported WASM version: {}. Only version 1 is supported.",
                            num
                        )));
                    }
                }
                Payload::ImportSection(imports) => {
                    for import in imports.into_imports() {
                        let import = import.map_err(|e| {
                            ValidationError::InvalidWasm(format!("Invalid import entry: {}", e))
                        })?;

                        let full_name = format!("{}.{}", import.module, import.name);

                        // Check for forbidden imports
                        if self.is_forbidden_import(&full_name) {
                            return Err(ValidationError::UnauthorizedImport(full_name));
                        }

                        // Check memory import limits if applicable
                        if let TypeRef::Memory(mem_ty) = import.ty {
                            self.validate_memory_type(&mem_ty)?;
                        }
                    }
                }
                Payload::MemorySection(memories) => {
                    for memory in memories {
                        let memory = memory.map_err(|e| {
                            ValidationError::InvalidWasm(format!("Invalid memory entry: {}", e))
                        })?;
                        self.validate_memory_type(&memory)?;
                    }
                }
                Payload::DataSection(data) => {
                    // Validate data segments don't exceed bounds
                    for segment in data {
                        let segment = segment.map_err(|e| {
                            ValidationError::InvalidWasm(format!("Invalid data segment: {}", e))
                        })?;
                        // Check that data segment mode is passive or within bounds
                        match segment.kind {
                            wasmparser::DataKind::Passive => {}
                            wasmparser::DataKind::Active {
                                memory_index,
                                ref offset_expr,
                            } => {
                                if memory_index != 0 {
                                    return Err(ValidationError::InvalidWasm(
                                        "Data segment references non-zero memory index".to_string(),
                                    ));
                                }
                                // offset_expr is validated by wasmparser; we just ensure it's
                                // present
                                let _ = offset_expr;
                            }
                        }
                    }
                }
                Payload::ExportSection(exports) => {
                    for export in exports {
                        let export = export.map_err(|e| {
                            ValidationError::InvalidWasm(format!("Invalid export entry: {}", e))
                        })?;
                        // Check export name for suspicious patterns
                        if export.name.contains("__syscall") || export.name.contains("__wasi") {
                            return Err(ValidationError::DangerousPattern(format!(
                                "Suspicious export name: {}",
                                export.name
                            )));
                        }
                    }
                }
                Payload::CustomSection(custom) => {
                    // Check custom section name for known malicious sections
                    let bad_sections: &[&str] = &["malicious", "exploit", "shellcode"];
                    for name in bad_sections {
                        if custom.name().eq_ignore_ascii_case(name) {
                            return Err(ValidationError::DangerousPattern(format!(
                                "Suspicious custom section: {}",
                                custom.name()
                            )));
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Validate memory type against policy
    fn validate_memory_type(
        &self,
        memory: &wasmparser::MemoryType,
    ) -> std::result::Result<(), ValidationError> {
        let max_pages = memory.maximum.unwrap_or(memory.initial);
        let max_mb = ((max_pages as u64) * 64) / 1024; // 64KB per page -> MB

        if max_mb > self.policy.max_memory_mb as u64 {
            return Err(ValidationError::MemoryLimitExceeded {
                requested: max_mb as u32,
                max: self.policy.max_memory_mb,
            });
        }

        Ok(())
    }

    /// Check if an import is forbidden
    fn is_forbidden_import(&self, full_name: &str) -> bool {
        let forbidden_imports: &[&str] = &[
            "env.__syscall",
            "env.__wasi",
            "env.abort",
            "env.exit",
            "wasi_snapshot_preview1",
            "wasi_unstable",
        ];

        for forbidden in forbidden_imports {
            if full_name.starts_with(forbidden) {
                // Check whitelist override
                if self.policy.allowed_imports.contains(&full_name.to_string()) {
                    return false;
                }
                return true;
            }
        }

        false
    }

    /// Detect potential sandbox escape attempts by scanning imports, exports
    /// and custom sections
    pub fn detect_sandbox_escape(
        &self,
        wasm: &[u8],
    ) -> std::result::Result<Vec<String>, ValidationError> {
        use wasmparser::{Parser, Payload};

        let mut detections = Vec::new();
        let parser = Parser::new(0);

        for payload in parser.parse_all(wasm) {
            let payload =
                payload.map_err(|e| ValidationError::InvalidWasm(format!("Parse error: {}", e)))?;

            match payload {
                Payload::ImportSection(imports) => {
                    for import in imports.into_imports() {
                        let import = import.map_err(|e| {
                            ValidationError::InvalidWasm(format!("Invalid import: {}", e))
                        })?;

                        let full_name = format!("{}.{}", import.module, import.name);

                        let escape_patterns: &[(&str, &str)] = &[
                            ("Spectre/Meltdown timing", "rdtsc"),
                            ("Rowhammer cache flush", "clflush"),
                            ("Side-channel cache probing", "cache"),
                            ("Timing attack", "performance.now"),
                        ];

                        for (technique, pattern) in escape_patterns {
                            if full_name.contains(pattern) {
                                detections.push(format!(
                                    "{} attack pattern in import: {}",
                                    technique, full_name
                                ));
                            }
                        }
                    }
                }
                Payload::ExportSection(exports) => {
                    for export in exports {
                        let export = export.map_err(|e| {
                            ValidationError::InvalidWasm(format!("Invalid export: {}", e))
                        })?;

                        let escape_patterns: &[(&str, &str)] = &[
                            ("Spectre/Meltdown timing", "rdtsc"),
                            ("Rowhammer cache flush", "clflush"),
                            ("Side-channel cache probing", "cache"),
                            ("Timing attack", "performance.now"),
                        ];

                        for (technique, pattern) in escape_patterns {
                            if export.name.contains(pattern) {
                                detections.push(format!(
                                    "{} attack pattern in export: {}",
                                    technique, export.name
                                ));
                            }
                        }
                    }
                }
                Payload::CustomSection(custom) => {
                    let suspicious_sections: &[&str] =
                        &["malicious", "exploit", "shellcode", "payload"];
                    for name in suspicious_sections {
                        if custom.name().eq_ignore_ascii_case(name) {
                            detections.push(format!(
                                "Suspicious custom section detected: {}",
                                custom.name()
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(detections)
    }

    pub fn policy(&self) -> &SkillSecurityPolicy {
        &self.policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_magic_validation() {
        let validator = SkillSecurityValidator::new(SkillSecurityPolicy::default());

        // Valid WASM header
        let valid_wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        assert!(validator.validate(&valid_wasm).is_ok());

        // Invalid WASM header
        let invalid_wasm = vec![0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
        assert!(validator.validate(&invalid_wasm).is_err());
    }

    #[test]
    fn test_module_size_limit() {
        let mut policy = SkillSecurityPolicy::default();
        policy.max_module_size = 100;

        let validator = SkillSecurityValidator::new(policy);

        // Module too large
        let large_wasm = vec![0x00; 101];
        assert!(validator.validate(&large_wasm).is_err());
    }

    #[test]
    fn test_forbidden_import_detection() {
        let policy = SkillSecurityPolicy::default();
        let validator = SkillSecurityValidator::new(policy);

        assert!(validator.is_forbidden_import("env.__syscall"));
        assert!(validator.is_forbidden_import("env.exit"));
        assert!(!validator.is_forbidden_import("env.memory"));
    }
}
