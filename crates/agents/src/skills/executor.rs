//! Skill Executor
//!
//! Executes WASM skills in sandboxed environment using kernel's WASM runtime.
//! Supports single-shot execution, multi-function dispatch, timeout
//! enforcement, and streaming output.

use std::collections::HashMap;
use std::sync::Arc;

use beebotos_kernel::error::KernelError;
use beebotos_kernel::wasm::{EngineConfig, WasmEngine};
use tokio::sync::mpsc;

use crate::skills::loader::LoadedSkill;
use crate::skills::security::{SkillSecurityPolicy, SkillSecurityValidator};

/// Safe offset for input buffer in WASM linear memory (64KB, avoiding
/// stack/data collisions).
const INPUT_BUFFER_OFFSET: usize = 64 * 1024;

/// Chunk size for streaming output (1KB per chunk)
const STREAM_CHUNK_SIZE: usize = 1024;

/// Skill executor using kernel's WASM runtime
#[derive(Clone)]
pub struct SkillExecutor {
    engine: Arc<WasmEngine>,
    security_validator: Arc<SkillSecurityValidator>,
}

/// Execution context for skills
#[derive(Debug, Clone)]
pub struct SkillContext {
    pub input: String,
    pub parameters: HashMap<String, String>,
}

/// Skill execution result
#[derive(Debug, Clone)]
pub struct SkillExecutionResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    /// Structured output parsed as map<string, bytes> when the WASM returns
    /// a valid JSON object with base64-encoded values.
    pub structured_output: Option<HashMap<String, Vec<u8>>>,
    pub execution_time_ms: u64,
}

/// A chunk emitted during streaming execution
#[derive(Debug, Clone)]
pub enum StreamChunk {
    Data(String),
    Error(String),
    Complete,
}

impl SkillExecutor {
    /// Create a new skill executor with default configuration
    pub fn new() -> Result<Self, SkillExecutionError> {
        let config = EngineConfig::default();
        let engine =
            WasmEngine::new(config).map_err(|e| SkillExecutionError::EngineError(e.to_string()))?;

        Ok(Self {
            engine: Arc::new(engine),
            security_validator: Arc::new(SkillSecurityValidator::new(
                SkillSecurityPolicy::default(),
            )),
        })
    }

    /// Create with custom engine configuration
    pub fn with_config(config: EngineConfig) -> Result<Self, SkillExecutionError> {
        let engine =
            WasmEngine::new(config).map_err(|e| SkillExecutionError::EngineError(e.to_string()))?;

        Ok(Self {
            engine: Arc::new(engine),
            security_validator: Arc::new(SkillSecurityValidator::new(
                SkillSecurityPolicy::default(),
            )),
        })
    }

    /// Create with custom engine configuration and security policy
    pub fn with_config_and_policy(
        config: EngineConfig,
        policy: SkillSecurityPolicy,
    ) -> Result<Self, SkillExecutionError> {
        let engine =
            WasmEngine::new(config).map_err(|e| SkillExecutionError::EngineError(e.to_string()))?;

        Ok(Self {
            engine: Arc::new(engine),
            security_validator: Arc::new(SkillSecurityValidator::new(policy)),
        })
    }

    /// Execute a skill using the legacy entry-point convention.
    pub async fn execute(
        &self,
        skill: &LoadedSkill,
        context: SkillContext,
    ) -> Result<SkillExecutionResult, SkillExecutionError> {
        self.execute_function(skill, None, HashMap::new(), None, context)
            .await
    }

    /// Execute a specific function exported by the skill with optional timeout.
    ///
    /// # Arguments
    /// * `skill` — The loaded skill to execute
    /// * `function_name` — Optional exported function name; falls back to
    ///   `manifest.entry_point`
    /// * `parameters` — Binary parameters encoded as JSON with base64 values
    ///   before passing to WASM
    /// * `timeout_ms` — Optional execution timeout in milliseconds. **Note**:
    ///   The timeout is enforced by Tokio's async scheduler. If the WASM guest
    ///   enters an infinite loop without yielding, the timeout may not fire
    ///   promptly because wasmtime execution is synchronous and does not yield
    ///   to the Tokio runtime.
    /// * `context` — Legacy skill context (merged with parameters)
    pub async fn execute_function(
        &self,
        skill: &LoadedSkill,
        function_name: Option<&str>,
        parameters: HashMap<String, Vec<u8>>,
        timeout_ms: Option<u32>,
        context: SkillContext,
    ) -> Result<SkillExecutionResult, SkillExecutionError> {
        let (output, execution_time_ms) = self
            .run_wasm_function(skill, function_name, &parameters, &context, timeout_ms)
            .await?;

        let structured_output = parse_structured_output(&output);

        Ok(SkillExecutionResult {
            task_id: skill.id.clone(),
            success: true,
            output,
            structured_output,
            execution_time_ms,
        })
    }

    /// Shared core: read WASM, validate, compile, instantiate, call, read
    /// output.
    async fn run_wasm_function(
        &self,
        skill: &LoadedSkill,
        function_name: Option<&str>,
        parameters: &HashMap<String, Vec<u8>>,
        context: &SkillContext,
        timeout_ms: Option<u32>,
    ) -> Result<(String, u64), SkillExecutionError> {
        let target_func = match function_name {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => skill.manifest.entry_point.clone(),
        };

        let wasm_bytes = tokio::fs::read(&skill.wasm_path)
            .await
            .map_err(|e| SkillExecutionError::IoError(e.to_string()))?;

        self.security_validator
            .validate(&wasm_bytes)
            .map_err(|e| SkillExecutionError::SecurityValidationFailed(e.to_string()))?;

        let module = self
            .engine
            .compile_cached(&skill.id, &wasm_bytes)
            .map_err(|e| SkillExecutionError::InvalidWasm(e.to_string()))?;

        let mut instance = self
            .engine
            .instantiate_with_host(&module, &skill.id)
            .map_err(|e| SkillExecutionError::InstantiationError(e.to_string()))?;

        if function_name.is_some() && !function_name.unwrap().is_empty() {
            if !instance.has_export(&target_func) {
                return Err(SkillExecutionError::FunctionNotFound(format!(
                    "Function '{}' not exported by skill '{}'",
                    target_func, skill.id
                )));
            }
        }

        let input_json = build_input_json(&context.input, parameters);
        let input_bytes = input_json.as_bytes();

        let mem_bytes = instance.memory_size_bytes();
        let needed = INPUT_BUFFER_OFFSET + input_bytes.len() + 4096;
        if mem_bytes < needed {
            let current_pages = (mem_bytes / 65536) as u32;
            let needed_pages = ((needed + 65535) / 65536) as u32;
            let delta = needed_pages.saturating_sub(current_pages);
            if delta > 0 {
                instance
                    .grow_memory(delta)
                    .map_err(|e| SkillExecutionError::ExecutionFailed(e.to_string()))?;
            }
        }

        instance
            .write_memory(INPUT_BUFFER_OFFSET, input_bytes)
            .map_err(|e| SkillExecutionError::ExecutionFailed(e.to_string()))?;

        let start_time = std::time::Instant::now();

        let output_ptr = if let Some(ms) = timeout_ms {
            tokio::time::timeout(std::time::Duration::from_millis(ms as u64), async {
                instance.call_typed::<(i32, i32), i32>(
                    &target_func,
                    (INPUT_BUFFER_OFFSET as i32, input_bytes.len() as i32),
                )
            })
            .await
            .map_err(|_| SkillExecutionError::ExecutionFailed("Execution timed out".to_string()))?
            .map_err(|e| SkillExecutionError::ExecutionFailed(e.to_string()))?
        } else {
            instance
                .call_typed::<(i32, i32), i32>(
                    &target_func,
                    (INPUT_BUFFER_OFFSET as i32, input_bytes.len() as i32),
                )
                .map_err(|e| SkillExecutionError::ExecutionFailed(e.to_string()))?
        };

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        let output_len_bytes = instance.read_memory(output_ptr as usize, 4).map_err(|e| {
            SkillExecutionError::ExecutionFailed(format!("Failed to read output length: {}", e))
        })?;
        let output_len = i32::from_le_bytes([
            output_len_bytes[0],
            output_len_bytes[1],
            output_len_bytes[2],
            output_len_bytes[3],
        ]) as usize;

        const MAX_OUTPUT_LEN: usize = 1024 * 1024;
        if output_len > MAX_OUTPUT_LEN {
            return Err(SkillExecutionError::ExecutionFailed(format!(
                "Output length {} exceeds maximum {}",
                output_len, MAX_OUTPUT_LEN
            )));
        }

        let output_data = instance
            .read_memory(output_ptr as usize + 4, output_len)
            .map_err(|e| {
                SkillExecutionError::ExecutionFailed(format!("Failed to read output data: {}", e))
            })?;
        let output = String::from_utf8_lossy(&output_data).to_string();

        Ok((output, execution_time_ms))
    }

    /// Execute a skill in streaming mode.
    ///
    /// WASM runs in a blocking task; output is split into chunks and pushed
    /// through an async channel. The receiver can be consumed by a gRPC
    /// streaming response or SSE endpoint.
    /// Execute a skill in streaming mode.
    ///
    /// WASM runs in a background task; output is split into chunks and pushed
    /// through an async channel. The receiver can be consumed by a gRPC
    /// streaming response or SSE endpoint.
    pub async fn execute_stream(
        &self,
        skill: &LoadedSkill,
        function_name: Option<&str>,
        parameters: HashMap<String, Vec<u8>>,
        context: SkillContext,
    ) -> Result<mpsc::Receiver<StreamChunk>, SkillExecutionError> {
        let (tx, rx) = mpsc::channel::<StreamChunk>(16);

        let skill_clone = skill.clone();
        let function_name = function_name.map(|s| s.to_string());
        let parameters_clone = parameters.clone();
        let context_clone = context.clone();
        let engine = self.engine.clone();
        let validator = self.security_validator.clone();

        tokio::spawn(async move {
            let result = async {
                let target_func = match function_name.as_deref() {
                    Some(name) if !name.is_empty() => name.to_string(),
                    _ => skill_clone.manifest.entry_point.clone(),
                };

                let wasm_bytes = tokio::fs::read(&skill_clone.wasm_path)
                    .await
                    .map_err(|e| SkillExecutionError::IoError(e.to_string()))?;

                validator
                    .validate(&wasm_bytes)
                    .map_err(|e| SkillExecutionError::SecurityValidationFailed(e.to_string()))?;

                let module = engine
                    .compile_cached(&skill_clone.id, &wasm_bytes)
                    .map_err(|e| SkillExecutionError::InvalidWasm(e.to_string()))?;

                let mut instance = engine
                    .instantiate_with_host(&module, &skill_clone.id)
                    .map_err(|e| SkillExecutionError::InstantiationError(e.to_string()))?;

                if function_name.as_deref().is_some()
                    && !function_name.as_deref().unwrap().is_empty()
                    && !instance.has_export(&target_func)
                {
                    return Err(SkillExecutionError::FunctionNotFound(format!(
                        "Function '{}' not exported",
                        target_func
                    )));
                }

                let input_json = build_input_json(&context_clone.input, &parameters_clone);
                let input_bytes = input_json.as_bytes();

                let mem_bytes = instance.memory_size_bytes();
                let needed = INPUT_BUFFER_OFFSET + input_bytes.len() + 4096;
                if mem_bytes < needed {
                    let current_pages = (mem_bytes / 65536) as u32;
                    let needed_pages = ((needed + 65535) / 65536) as u32;
                    let delta = needed_pages.saturating_sub(current_pages);
                    if delta > 0 {
                        instance
                            .grow_memory(delta)
                            .map_err(|e| SkillExecutionError::ExecutionFailed(e.to_string()))?;
                    }
                }

                instance
                    .write_memory(INPUT_BUFFER_OFFSET, input_bytes)
                    .map_err(|e| SkillExecutionError::ExecutionFailed(e.to_string()))?;

                let start_time = std::time::Instant::now();
                let output_ptr = instance
                    .call_typed::<(i32, i32), i32>(
                        &target_func,
                        (INPUT_BUFFER_OFFSET as i32, input_bytes.len() as i32),
                    )
                    .map_err(|e| SkillExecutionError::ExecutionFailed(e.to_string()))?;
                let execution_time_ms = start_time.elapsed().as_millis() as u64;

                let output_len_bytes =
                    instance.read_memory(output_ptr as usize, 4).map_err(|e| {
                        SkillExecutionError::ExecutionFailed(format!(
                            "Failed to read output length: {}",
                            e
                        ))
                    })?;
                let output_len = i32::from_le_bytes([
                    output_len_bytes[0],
                    output_len_bytes[1],
                    output_len_bytes[2],
                    output_len_bytes[3],
                ]) as usize;

                const MAX_OUTPUT_LEN: usize = 1024 * 1024;
                if output_len > MAX_OUTPUT_LEN {
                    return Err(SkillExecutionError::ExecutionFailed(format!(
                        "Output length {} exceeds maximum {}",
                        output_len, MAX_OUTPUT_LEN
                    )));
                }

                let output_data = instance
                    .read_memory(output_ptr as usize + 4, output_len)
                    .map_err(|e| {
                        SkillExecutionError::ExecutionFailed(format!(
                            "Failed to read output data: {}",
                            e
                        ))
                    })?;

                let output = String::from_utf8_lossy(&output_data).to_string();
                Ok::<(String, u64), SkillExecutionError>((output, execution_time_ms))
            }
            .await;

            match result {
                Ok((output, _elapsed)) => {
                    for chunk in output.as_bytes().chunks(STREAM_CHUNK_SIZE) {
                        let chunk_str = String::from_utf8_lossy(chunk).to_string();
                        if tx.send(StreamChunk::Data(chunk_str)).await.is_err() {
                            return; // Receiver dropped
                        }
                    }
                    let _ = tx.send(StreamChunk::Complete).await;
                }
                Err(e) => {
                    let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                }
            }
        });

        Ok(rx)
    }

    /// Precompile skill for faster loading
    pub async fn precompile(&self, skill: &LoadedSkill) -> Result<Vec<u8>, SkillExecutionError> {
        let wasm_bytes = tokio::fs::read(&skill.wasm_path)
            .await
            .map_err(|e| SkillExecutionError::IoError(e.to_string()))?;

        self.engine
            .precompile(&wasm_bytes)
            .map_err(|e| SkillExecutionError::CompilationError(e.to_string()))
    }

    /// Get engine cache statistics
    pub fn cache_stats(&self) -> beebotos_kernel::wasm::CacheStats {
        self.engine.cache_stats()
    }

    /// Get reference to the security validator
    pub fn security_validator(&self) -> &SkillSecurityValidator {
        &self.security_validator
    }
}

/// Build JSON input merging legacy string input with structured binary
/// parameters.
///
/// Binary values are base64-encoded so they survive JSON serialization.
fn build_input_json(legacy_input: &str, parameters: &HashMap<String, Vec<u8>>) -> String {
    if parameters.is_empty() {
        return legacy_input.to_string();
    }

    let mut map = serde_json::Map::new();

    // Insert legacy input under a reserved key when parameters are present
    if !legacy_input.is_empty() {
        map.insert(
            "__input".to_string(),
            serde_json::Value::String(legacy_input.to_string()),
        );
    }

    for (k, v) in parameters {
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, v);
        map.insert(k.clone(), serde_json::Value::String(b64));
    }

    serde_json::Value::Object(map).to_string()
}

/// Attempt to parse WASM output as structured map<string, bytes>.
///
/// Expects a JSON object where string values are base64-encoded bytes.
/// Returns `None` if parsing fails or the output is not a JSON object.
fn parse_structured_output(output: &str) -> Option<HashMap<String, Vec<u8>>> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    let obj = value.as_object()?;

    let mut result = HashMap::with_capacity(obj.len());
    for (k, v) in obj {
        if k == "__input" {
            continue; // skip internal legacy input key
        }
        let s = v.as_str()?;
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s).ok()?;
        result.insert(k.clone(), bytes);
    }

    Some(result)
}

/// Skill execution errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum SkillExecutionError {
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Engine error: {0}")]
    EngineError(String),
    #[error("Invalid WASM: {0}")]
    InvalidWasm(String),
    #[error("Instantiation error: {0}")]
    InstantiationError(String),
    #[error("Entry point not found: {0}")]
    EntryPointNotFound(String),
    #[error("Function not found: {0}")]
    FunctionNotFound(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Compilation error: {0}")]
    CompilationError(String),
    #[error("Security validation failed: {0}")]
    SecurityValidationFailed(String),
}

impl From<KernelError> for SkillExecutionError {
    fn from(e: KernelError) -> Self {
        SkillExecutionError::EngineError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_executor_creation() {
        let executor = SkillExecutor::new();
        assert!(executor.is_ok());
    }

    #[test]
    fn test_skill_executor_with_config() {
        let config = EngineConfig::default();
        let executor = SkillExecutor::with_config(config);
        assert!(executor.is_ok());
    }

    #[test]
    fn test_build_input_json() {
        let mut params = HashMap::new();
        params.insert("image".to_string(), vec![0x89, 0x50, 0x4e, 0x47]);

        let json = build_input_json("hello", &params);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["__input"].as_str(), Some("hello"));
        assert!(parsed["image"].as_str().is_some());
    }

    #[test]
    fn test_build_input_json_no_params() {
        let params = HashMap::new();
        let json = build_input_json("plain text", &params);
        assert_eq!(json, "plain text");
    }

    #[test]
    fn test_parse_structured_output_roundtrip() {
        let mut original = HashMap::new();
        original.insert("key1".to_string(), vec![1, 2, 3]);
        original.insert("key2".to_string(), b"hello".to_vec());

        let json = build_input_json("", &original);
        let parsed = parse_structured_output(&json).unwrap();
        assert_eq!(parsed.get("key1"), Some(&vec![1, 2, 3]));
        assert_eq!(parsed.get("key2"), Some(&b"hello".to_vec()));
    }

    #[test]
    fn test_parse_structured_output_invalid() {
        assert!(parse_structured_output("not json").is_none());
        assert!(parse_structured_output("42").is_none());
    }
}
