//! gRPC Skill Registry Service
//!
//! Full implementation of the proto SkillRegistry service with instance-based
//! execution model: skill lifecycle, multi-function dispatch, timeout
//! enforcement, and streaming output.

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::mpsc;
use tonic::{Request, Response, Status};

use super::generated::beebotos::common::SemanticVersion;
use super::generated::beebotos::skills::registry::skill_registry_server::{
    SkillRegistry, SkillRegistryServer,
};
use super::generated::beebotos::skills::registry::*;

/// gRPC service for skill registry and instance operations.
pub struct SkillsGrpcService {
    registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
    instance_manager: Option<Arc<beebotos_agents::skills::InstanceManager>>,
    executor: Option<Arc<beebotos_agents::skills::SkillExecutor>>,
    rating_store: Option<beebotos_agents::skills::SkillRatingStore>,
    skills_base_dir: PathBuf,
}

impl SkillsGrpcService {
    pub fn new(
        registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
        instance_manager: Option<Arc<beebotos_agents::skills::InstanceManager>>,
        executor: Option<Arc<beebotos_agents::skills::SkillExecutor>>,
        skills_base_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            registry,
            instance_manager,
            executor,
            rating_store: None,
            skills_base_dir: skills_base_dir.into(),
        }
    }

    pub fn with_rating_store(mut self, db: sqlx::SqlitePool) -> Self {
        self.rating_store = Some(beebotos_agents::skills::SkillRatingStore::new(db));
        self
    }

    pub fn into_server(self) -> SkillRegistryServer<Self> {
        SkillRegistryServer::new(self)
    }

    /// Helper to create a SkillLoader with the configured base directory.
    fn create_loader(&self) -> beebotos_agents::skills::SkillLoader {
        let mut loader = beebotos_agents::skills::SkillLoader::new();
        loader.add_path(&self.skills_base_dir);
        loader
    }

    /// Resolve instance_id → SkillInstance, verifying the instance exists.
    async fn resolve_instance(
        &self,
        instance_id: &str,
    ) -> Result<beebotos_agents::skills::SkillInstance, Status> {
        let manager = self
            .instance_manager
            .as_ref()
            .ok_or_else(|| Status::internal("Instance manager not available"))?;
        manager
            .get(instance_id)
            .await
            .ok_or_else(|| Status::not_found(format!("Instance {} not found", instance_id)))
    }

    /// Load a skill by ID using a fresh loader.
    async fn load_skill(
        &self,
        skill_id: &str,
    ) -> Result<beebotos_agents::skills::LoadedSkill, Status> {
        let mut loader = self.create_loader();
        loader
            .load_skill(skill_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to load skill: {}", e)))
    }
}

impl Default for SkillsGrpcService {
    fn default() -> Self {
        Self {
            registry: None,
            instance_manager: None,
            executor: None,
            rating_store: None,
            skills_base_dir: PathBuf::from("data/skills"),
        }
    }
}

/// Maximum package size for skill upload (50 MB)
const MAX_PACKAGE_SIZE: usize = 50 * 1024 * 1024;

/// Validate that a skill_id does not contain path traversal characters.
fn validate_skill_id(skill_id: &str) -> Result<(), Status> {
    if skill_id.is_empty() {
        return Err(Status::invalid_argument("skill_id cannot be empty"));
    }
    if skill_id.starts_with('/') {
        return Err(Status::invalid_argument(
            "skill_id cannot be an absolute path",
        ));
    }
    if skill_id.contains("..") || skill_id.contains('/') || skill_id.contains('\\') {
        return Err(Status::invalid_argument(
            "skill_id contains invalid characters",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn convert_version(v: &beebotos_agents::skills::Version) -> Option<SemanticVersion> {
    Some(SemanticVersion {
        major: v.major,
        minor: v.minor,
        patch: v.patch,
        prerelease: "".to_string(),
        build: "".to_string(),
    })
}

fn convert_registered_skill(r: &beebotos_agents::skills::RegisteredSkill) -> Skill {
    Skill {
        id: r.skill.id.clone(),
        name: r.skill.name.clone(),
        description: r.skill.manifest.description.clone(),
        version: convert_version(&r.skill.version),
        author: r.skill.manifest.author.clone(),
        categories: vec![r.category.clone()],
        functions: r
            .skill
            .manifest
            .functions
            .iter()
            .map(|f| Function {
                name: f.name.clone(),
                description: f.description.clone(),
                inputs: f
                    .inputs
                    .iter()
                    .map(|p| Parameter {
                        name: p.name.clone(),
                        r#type: p.param_type.clone(),
                        required: p.required,
                        description: p.description.clone(),
                        default_value: p.default_value.clone(),
                    })
                    .collect(),
                outputs: f
                    .outputs
                    .iter()
                    .map(|p| Parameter {
                        name: p.name.clone(),
                        r#type: p.param_type.clone(),
                        required: p.required,
                        description: p.description.clone(),
                        default_value: p.default_value.clone(),
                    })
                    .collect(),
                example: f.example.clone(),
            })
            .collect(),
        metadata: Some(SkillMetadata {
            icon: "".to_string(),
            readme: "".to_string(),
            changelog: "".to_string(),
            license: r.skill.manifest.license.clone(),
            keywords: r.tags.clone(),
            labels: std::collections::HashMap::new(),
        }),
        pricing: Some(PricingInfo {
            model: PricingModel::PricingFree as i32,
            price_per_use: "0".to_string(),
            price_per_month: "0".to_string(),
            revenue_share_percent: "0".to_string(),
            token_address: "".to_string(),
        }),
        dependencies: vec![],
        repository_url: "".to_string(),
        documentation_url: "".to_string(),
        created_at: r.installed_at as i64,
        updated_at: r.installed_at as i64,
    }
}

fn convert_instance_status(s: beebotos_agents::skills::InstanceStatus) -> i32 {
    use beebotos_agents::skills::InstanceStatus::*;
    match s {
        Pending => InstanceStatus::InstancePending as i32,
        Running => InstanceStatus::InstanceRunning as i32,
        Paused => InstanceStatus::InstancePaused as i32,
        Stopped => InstanceStatus::InstanceStopped as i32,
        Error => InstanceStatus::InstanceError as i32,
    }
}

fn proto_status_to_internal(status: i32) -> Option<beebotos_agents::skills::InstanceStatus> {
    use beebotos_agents::skills::InstanceStatus::*;
    match status {
        1 => Some(Pending),
        2 => Some(Running),
        3 => Some(Paused),
        4 => Some(Stopped),
        5 => Some(Error),
        _ => None,
    }
}

fn convert_instance(i: &beebotos_agents::skills::SkillInstance) -> SkillInstance {
    SkillInstance {
        instance_id: i.instance_id.clone(),
        skill_id: i.skill_id.clone(),
        agent_id: i.agent_id.clone(),
        status: convert_instance_status(i.status),
        config: i.config.clone(),
        started_at: i.started_at as i64,
        last_active: i.last_active as i64,
        usage: Some(UsageStats {
            total_calls: i.usage.total_calls,
            successful_calls: i.usage.successful_calls,
            failed_calls: i.usage.failed_calls,
            avg_latency_ms: i.usage.avg_latency_ms,
            total_cost: "0".to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Trait implementation
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl SkillRegistry for SkillsGrpcService {
    async fn register_skill(
        &self,
        request: Request<RegisterSkillRequest>,
    ) -> Result<Response<RegisterSkillResponse>, Status> {
        let req = request.into_inner();
        let registry = self
            .registry
            .as_ref()
            .ok_or_else(|| Status::internal("Skill registry not available"))?;

        let skill_proto = req
            .skill
            .ok_or_else(|| Status::invalid_argument("Skill metadata required"))?;

        validate_skill_id(&skill_proto.id)?;

        // Reject oversized packages
        if req.package_data.len() > MAX_PACKAGE_SIZE {
            return Err(Status::invalid_argument(format!(
                "Package size {} exceeds maximum {}",
                req.package_data.len(),
                MAX_PACKAGE_SIZE
            )));
        }

        // Write package data to disk
        let skill_dir = self.skills_base_dir.join(&skill_proto.id);
        tokio::fs::create_dir_all(&skill_dir)
            .await
            .map_err(|e| Status::internal(format!("Failed to create skill dir: {}", e)))?;

        if !req.package_data.is_empty() {
            let package_path = skill_dir.join("package.zip");
            tokio::fs::write(&package_path, &req.package_data)
                .await
                .map_err(|e| Status::internal(format!("Failed to write package: {}", e)))?;

            // Extract ZIP in blocking task
            let skill_dir_clone = skill_dir.clone();
            tokio::task::spawn_blocking(move || {
                let file = std::fs::File::open(&package_path)?;
                let mut archive = zip::ZipArchive::new(file)?;
                archive.extract(&skill_dir_clone)?;
                Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
            })
            .await
            .map_err(|e| Status::internal(format!("ZIP extraction task failed: {}", e)))?
            .map_err(|e| Status::internal(format!("ZIP extraction failed: {}", e)))?;
        }

        // Load skill into registry
        let mut loader = self.create_loader();
        let loaded = loader
            .load_skill(&skill_proto.id)
            .await
            .map_err(|e| Status::internal(format!("Failed to load skill: {}", e)))?;

        let category = skill_proto.categories.first().cloned().unwrap_or_default();
        let keywords = skill_proto
            .metadata
            .as_ref()
            .map(|m| m.keywords.clone())
            .unwrap_or_default();
        registry.register(loaded, category, keywords).await;

        Ok(Response::new(RegisterSkillResponse {
            success: true,
            skill_id: skill_proto.id,
            error_message: "".to_string(),
        }))
    }

    async fn get_skill(
        &self,
        request: Request<GetSkillRequest>,
    ) -> Result<Response<Skill>, Status> {
        let skill_id = request.into_inner().skill_id;
        match &self.registry {
            Some(registry) => match registry.get(&skill_id).await {
                Some(r) => Ok(Response::new(convert_registered_skill(&r))),
                None => Err(Status::not_found(format!("Skill {} not found", skill_id))),
            },
            None => Err(Status::unimplemented("SkillRegistry not available")),
        }
    }

    async fn list_skills(
        &self,
        request: Request<ListSkillsRequest>,
    ) -> Result<Response<ListSkillsResponse>, Status> {
        let req = request.into_inner();
        match &self.registry {
            Some(registry) => {
                let mut skills = registry.list_all().await;
                if !req.category.is_empty() {
                    skills = registry.by_category(&req.category).await;
                }
                let total_count = skills.len() as u32;
                let proto_skills: Vec<Skill> =
                    skills.iter().map(|r| convert_registered_skill(r)).collect();
                Ok(Response::new(ListSkillsResponse {
                    skills: proto_skills,
                    next_page_token: "".to_string(),
                    total_count,
                }))
            }
            None => Err(Status::unimplemented("SkillRegistry not available")),
        }
    }

    async fn search_skills(
        &self,
        request: Request<SearchSkillsRequest>,
    ) -> Result<Response<ListSkillsResponse>, Status> {
        let req = request.into_inner();
        match &self.registry {
            Some(registry) => {
                let skills = registry.search(&req.query).await;
                let total_count = skills.len() as u32;
                let proto_skills: Vec<Skill> =
                    skills.iter().map(|r| convert_registered_skill(r)).collect();
                Ok(Response::new(ListSkillsResponse {
                    skills: proto_skills,
                    next_page_token: "".to_string(),
                    total_count,
                }))
            }
            None => Err(Status::unimplemented("SkillRegistry not available")),
        }
    }

    async fn update_skill(
        &self,
        request: Request<UpdateSkillRequest>,
    ) -> Result<Response<Skill>, Status> {
        let req = request.into_inner();
        let registry = self
            .registry
            .as_ref()
            .ok_or_else(|| Status::internal("Skill registry not available"))?;

        let skill = registry
            .get(&req.skill_id)
            .await
            .ok_or_else(|| Status::not_found(format!("Skill {} not found", req.skill_id)))?;

        // For now, only enable/disable is supported as an update
        if let Some(updated) = req.updated_skill {
            // Re-register with potentially new metadata
            let mut loader = self.create_loader();
            let loaded = loader
                .load_skill(&req.skill_id)
                .await
                .map_err(|e| Status::internal(format!("Failed to reload skill: {}", e)))?;

            let category = updated
                .categories
                .first()
                .cloned()
                .unwrap_or_else(|| skill.category.clone());
            let keywords = updated
                .metadata
                .as_ref()
                .map(|m| m.keywords.clone())
                .unwrap_or_default();
            registry.register(loaded, category, keywords).await;
        }

        let updated = registry.get(&req.skill_id).await.ok_or_else(|| {
            Status::not_found(format!("Skill {} not found after update", req.skill_id))
        })?;

        Ok(Response::new(convert_registered_skill(&updated)))
    }

    async fn delete_skill(
        &self,
        request: Request<DeleteSkillRequest>,
    ) -> Result<Response<DeleteSkillResponse>, Status> {
        let req = request.into_inner();
        validate_skill_id(&req.skill_id)?;

        let registry = self
            .registry
            .as_ref()
            .ok_or_else(|| Status::internal("Skill registry not available"))?;

        let removed = registry.unregister(&req.skill_id).await;
        if removed.is_none() {
            return Err(Status::not_found(format!(
                "Skill {} not found",
                req.skill_id
            )));
        }

        // Remove from disk
        let skill_dir = self.skills_base_dir.join(&req.skill_id);
        let _ = tokio::fs::remove_dir_all(&skill_dir).await;

        Ok(Response::new(DeleteSkillResponse { success: true }))
    }

    // -----------------------------------------------------------------------
    // Instance lifecycle
    // -----------------------------------------------------------------------

    async fn create_instance(
        &self,
        request: Request<CreateInstanceRequest>,
    ) -> Result<Response<SkillInstance>, Status> {
        let req = request.into_inner();
        let manager = self
            .instance_manager
            .as_ref()
            .ok_or_else(|| Status::internal("Instance manager not available"))?;

        // Verify skill exists
        let registry = self
            .registry
            .as_ref()
            .ok_or_else(|| Status::internal("Skill registry not available"))?;
        if registry.get(&req.skill_id).await.is_none() {
            return Err(Status::not_found(format!(
                "Skill {} not found",
                req.skill_id
            )));
        }

        let instance_id = manager
            .create(req.skill_id, req.agent_id, req.config)
            .await
            .map_err(|e| Status::resource_exhausted(format!("Instance limit reached: {}", e)))?;

        // Auto-transition to Running
        manager
            .update_status(
                &instance_id,
                beebotos_agents::skills::InstanceStatus::Running,
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to start instance: {}", e)))?;

        let instance = manager
            .get(&instance_id)
            .await
            .ok_or_else(|| Status::internal("Instance disappeared after creation"))?;

        Ok(Response::new(convert_instance(&instance)))
    }

    async fn get_instance(
        &self,
        request: Request<GetInstanceRequest>,
    ) -> Result<Response<SkillInstance>, Status> {
        let instance = self
            .resolve_instance(&request.into_inner().instance_id)
            .await?;
        Ok(Response::new(convert_instance(&instance)))
    }

    async fn update_instance(
        &self,
        request: Request<UpdateInstanceRequest>,
    ) -> Result<Response<SkillInstance>, Status> {
        let req = request.into_inner();
        let manager = self
            .instance_manager
            .as_ref()
            .ok_or_else(|| Status::internal("Instance manager not available"))?;

        // Update config if provided
        if !req.config_updates.is_empty() {
            manager
                .update_config(&req.instance_id, req.config_updates)
                .await
                .map_err(|e| Status::internal(format!("Failed to update config: {}", e)))?;
        }

        // Update status if provided
        if let Some(status) = proto_status_to_internal(req.new_status) {
            manager
                .update_status(&req.instance_id, status)
                .await
                .map_err(|e| Status::internal(format!("Failed to update status: {}", e)))?;
        }

        let instance = manager
            .get(&req.instance_id)
            .await
            .ok_or_else(|| Status::not_found(format!("Instance {} not found", req.instance_id)))?;

        Ok(Response::new(convert_instance(&instance)))
    }

    async fn delete_instance(
        &self,
        request: Request<DeleteInstanceRequest>,
    ) -> Result<Response<DeleteInstanceResponse>, Status> {
        let req = request.into_inner();
        let manager = self
            .instance_manager
            .as_ref()
            .ok_or_else(|| Status::internal("Instance manager not available"))?;

        manager
            .delete(&req.instance_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to delete instance: {}", e)))?;

        Ok(Response::new(DeleteInstanceResponse { success: true }))
    }

    async fn list_instances(
        &self,
        request: Request<ListInstancesRequest>,
    ) -> Result<Response<ListInstancesResponse>, Status> {
        let req = request.into_inner();
        let manager = self
            .instance_manager
            .as_ref()
            .ok_or_else(|| Status::internal("Instance manager not available"))?;

        let filter = beebotos_agents::skills::InstanceFilter {
            agent_id: if req.agent_id.is_empty() {
                None
            } else {
                Some(req.agent_id)
            },
            skill_id: if req.skill_id.is_empty() {
                None
            } else {
                Some(req.skill_id)
            },
            status: proto_status_to_internal(req.status),
            page: 0,
            page_size: 0,
        };

        let instances = manager.list(&filter).await;
        Ok(Response::new(ListInstancesResponse {
            instances: instances.iter().map(convert_instance).collect(),
        }))
    }

    // -----------------------------------------------------------------------
    // Execution
    // -----------------------------------------------------------------------

    async fn execute_function(
        &self,
        request: Request<ExecuteFunctionRequest>,
    ) -> Result<Response<ExecuteFunctionResponse>, Status> {
        let req = request.into_inner();

        // Resolve instance
        let instance = self.resolve_instance(&req.instance_id).await?;
        if instance.status != beebotos_agents::skills::InstanceStatus::Running {
            return Err(Status::failed_precondition(format!(
                "Instance {} is not running (status: {:?})",
                req.instance_id, instance.status
            )));
        }

        // Load skill
        let skill = self.load_skill(&instance.skill_id).await?;

        // Convert parameters
        let parameters: HashMap<String, Vec<u8>> = req.parameters.into_iter().collect();

        // Build context from instance config (if any)
        let context = beebotos_agents::skills::SkillContext {
            input: String::new(),
            parameters: instance.config.clone(),
        };

        let executor = self
            .executor
            .as_ref()
            .ok_or_else(|| Status::internal("Skill executor not available"))?;

        let result = executor
            .execute_function(
                &skill,
                Some(&req.function_name),
                parameters,
                if req.timeout_ms > 0 {
                    Some(req.timeout_ms)
                } else {
                    None
                },
                context,
            )
            .await
            .map_err(|e| Status::internal(format!("Execution failed: {}", e)))?;

        // Record usage
        if let Some(manager) = &self.instance_manager {
            let _ = manager
                .record_execution(
                    &req.instance_id,
                    result.success,
                    result.execution_time_ms as f64,
                )
                .await;
        }

        // Convert structured output to proto map<string, bytes>
        let results = result.structured_output.unwrap_or_else(|| {
            let mut map = HashMap::new();
            map.insert("output".to_string(), result.output.into_bytes());
            map
        });

        Ok(Response::new(ExecuteFunctionResponse {
            success: result.success,
            results,
            error_message: "".to_string(),
            execution_time_ms: result.execution_time_ms as u32,
        }))
    }

    type StreamFunctionStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<FunctionOutput, Status>> + Send>>;

    async fn stream_function(
        &self,
        request: Request<StreamFunctionRequest>,
    ) -> Result<Response<Self::StreamFunctionStream>, Status> {
        let req = request.into_inner();

        // Resolve instance
        let instance = self.resolve_instance(&req.instance_id).await?;
        if instance.status != beebotos_agents::skills::InstanceStatus::Running {
            return Err(Status::failed_precondition(format!(
                "Instance {} is not running (status: {:?})",
                req.instance_id, instance.status
            )));
        }

        // Load skill
        let skill = self.load_skill(&instance.skill_id).await?;

        // Convert parameters
        let parameters: HashMap<String, Vec<u8>> = req.parameters.into_iter().collect();

        let context = beebotos_agents::skills::SkillContext {
            input: String::new(),
            parameters: instance.config.clone(),
        };

        let executor = self
            .executor
            .as_ref()
            .ok_or_else(|| Status::internal("Skill executor not available"))?;

        let mut rx = executor
            .execute_stream(&skill, Some(&req.function_name), parameters, context)
            .await
            .map_err(|e| Status::internal(format!("Stream execution failed: {}", e)))?;

        // Bridge mpsc::Receiver<StreamChunk> → tonic Stream<FunctionOutput>
        let (tx, out_rx) = mpsc::channel::<Result<FunctionOutput, Status>>(16);
        let instance_id = req.instance_id.clone();
        let manager = self.instance_manager.clone();
        tokio::spawn(async move {
            let start_time = std::time::Instant::now();
            let mut success = true;

            while let Some(chunk) = rx.recv().await {
                let msg = match &chunk {
                    beebotos_agents::skills::StreamChunk::Data(data) => Ok(FunctionOutput {
                        output: Some(function_output::Output::Data(data.clone().into_bytes())),
                    }),
                    beebotos_agents::skills::StreamChunk::Error(err) => {
                        success = false;
                        Ok(FunctionOutput {
                            output: Some(function_output::Output::Error(err.clone())),
                        })
                    }
                    beebotos_agents::skills::StreamChunk::Complete => Ok(FunctionOutput {
                        output: Some(function_output::Output::Complete(true)),
                    }),
                };
                if tx.send(msg).await.is_err() {
                    break;
                }
                if matches!(chunk, beebotos_agents::skills::StreamChunk::Complete) {
                    break;
                }
            }

            // Record usage after stream actually completes
            let latency_ms = start_time.elapsed().as_millis() as f64;
            if let Some(m) = manager {
                let _ = m.record_execution(&instance_id, success, latency_ms).await;
            }
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(out_rx);
        Ok(Response::new(Box::pin(stream) as Self::StreamFunctionStream))
    }

    async fn rate_skill(
        &self,
        request: Request<RateSkillRequest>,
    ) -> Result<Response<RateSkillResponse>, Status> {
        let req = request.into_inner();
        let store = self
            .rating_store
            .as_ref()
            .ok_or_else(|| Status::unimplemented("Rating store not available"))?;

        store
            .rate(&req.skill_id, &req.user_id, req.rating, Some(&req.review))
            .await
            .map_err(|e| Status::internal(format!("Failed to save rating: {}", e)))?;

        let summary = store
            .get_summary(&req.skill_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get summary: {}", e)))?;

        Ok(Response::new(RateSkillResponse {
            success: true,
            new_average_rating: summary.average_rating,
        }))
    }

    async fn get_skill_ratings(
        &self,
        request: Request<GetSkillRatingsRequest>,
    ) -> Result<Response<GetSkillRatingsResponse>, Status> {
        let req = request.into_inner();
        let store = self
            .rating_store
            .as_ref()
            .ok_or_else(|| Status::unimplemented("Rating store not available"))?;

        let summary = store
            .get_summary(&req.skill_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get summary: {}", e)))?;

        let limit = req.page_size as i64;
        let offset = 0i64;
        let ratings = store
            .list_ratings(&req.skill_id, limit, offset)
            .await
            .map_err(|e| Status::internal(format!("Failed to list ratings: {}", e)))?;

        let proto_ratings: Vec<SkillRating> = ratings
            .into_iter()
            .map(|r| SkillRating {
                skill_id: r.skill_id,
                user_id: r.user_id,
                rating: r.rating as u32,
                review: r.review.unwrap_or_default(),
                created_at: r.created_at,
            })
            .collect();

        Ok(Response::new(GetSkillRatingsResponse {
            ratings: proto_ratings,
            average_rating: summary.average_rating,
            total_ratings: summary.total_ratings,
            next_page_token: "".to_string(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn create_test_service() -> (SkillsGrpcService, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        // Create a dummy skill directory so load_skill succeeds
        let skill_dir = tmp.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("skill.yaml"),
            r#"id: test-skill
name: Test Skill
version: 1.0.0
description: Test
author: test
license: MIT
capabilities: []
permissions: []
entry_point: handle
"#,
        )
        .unwrap();
        // Write a minimal valid WASM header
        std::fs::write(
            skill_dir.join("skill.wasm"),
            &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
        )
        .unwrap();

        let svc = SkillsGrpcService::new(
            Some(Arc::new(beebotos_agents::skills::SkillRegistry::new())),
            Some(Arc::new(beebotos_agents::skills::InstanceManager::new())),
            None,
            tmp.path(),
        );
        (svc, tmp)
    }

    #[tokio::test]
    async fn test_instance_lifecycle_grpc() {
        let (service, _tmp) = create_test_service();

        // Register a skill first (package_data empty skips extraction)
        let _ = service
            .register_skill(Request::new(RegisterSkillRequest {
                skill: Some(Skill {
                    id: "test-skill".to_string(),
                    name: "Test Skill".to_string(),
                    ..Default::default()
                }),
                package_data: vec![],
            }))
            .await
            .unwrap();

        // Create instance
        let resp = service
            .create_instance(Request::new(CreateInstanceRequest {
                skill_id: "test-skill".to_string(),
                agent_id: "agent-1".to_string(),
                config: std::collections::HashMap::new(),
            }))
            .await
            .unwrap();
        let instance = resp.into_inner();
        assert_eq!(instance.skill_id, "test-skill");
        assert_eq!(instance.agent_id, "agent-1");
        assert_eq!(instance.status, InstanceStatus::InstanceRunning as i32);

        let instance_id = instance.instance_id.clone();

        // Get instance
        let resp = service
            .get_instance(Request::new(GetInstanceRequest {
                instance_id: instance_id.clone(),
            }))
            .await
            .unwrap();
        assert_eq!(resp.into_inner().instance_id, instance_id);

        // List instances
        let resp = service
            .list_instances(Request::new(ListInstancesRequest {
                agent_id: "agent-1".to_string(),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(resp.into_inner().instances.len(), 1);

        // Delete instance
        let resp = service
            .delete_instance(Request::new(DeleteInstanceRequest {
                instance_id: instance_id.clone(),
            }))
            .await
            .unwrap();
        assert!(resp.into_inner().success);

        // Verify deleted
        assert!(service
            .get_instance(Request::new(GetInstanceRequest {
                instance_id: instance_id.clone(),
            }))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_delete_skill_not_found() {
        let (service, _tmp) = create_test_service();
        let req = Request::new(DeleteSkillRequest {
            skill_id: "nonexistent".to_string(),
            reason: "".to_string(),
        });
        assert!(service.delete_skill(req).await.is_err());
    }
}
