//! 消息处理器
//!
//! 集成消息去重、会话管理、多模态处理、Memory 协同和持久化

use std::collections::HashMap;
use std::sync::Arc;

use beebotos_agents::communication::channel::session_manager::{SessionManager, SessionMessage};
use beebotos_agents::communication::channel::ChannelEvent;
use beebotos_agents::communication::{Message, MessageType, PlatformType};
use beebotos_agents::deduplicator::MessageDeduplicator;
use beebotos_agents::media::multimodal::MultimodalProcessor;
use beebotos_agents::ChannelRegistry;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::GatewayError;
use crate::services::agent_resolver::AgentResolver;
use crate::services::llm_service::LlmService;
use crate::services::webchat_service::WebchatService;

/// 消息处理器
pub struct MessageProcessor {
    /// 去重器
    deduplicator: Arc<MessageDeduplicator>,
    /// 会话管理器
    session_manager: Arc<SessionManager>,
    /// 多模态处理器
    multimodal_processor: MultimodalProcessor,
    /// LLM 服务
    llm_service: Arc<LlmService>,
    /// 频道注册表
    channel_registry: Arc<ChannelRegistry>,
    /// Memory 系统
    memory_system: Option<Arc<beebotos_agents::memory::UnifiedMemorySystem>>,
    /// Webchat 持久化服务
    webchat_service: Option<Arc<WebchatService>>,
    /// Skill 注册表
    skill_registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
}

impl MessageProcessor {
    /// 创建新的消息处理器
    pub fn new(
        llm_service: Arc<LlmService>,
        channel_registry: Arc<ChannelRegistry>,
        memory_system: Option<Arc<beebotos_agents::memory::UnifiedMemorySystem>>,
        webchat_service: Option<Arc<WebchatService>>,
        skill_registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
    ) -> Self {
        Self {
            deduplicator: Arc::new(MessageDeduplicator::default()),
            session_manager: SessionManager::default(),
            multimodal_processor: MultimodalProcessor::new(),
            llm_service,
            channel_registry,
            memory_system,
            webchat_service,
            skill_registry,
        }
    }

    /// 处理频道事件
    pub async fn process_event(&self, event: ChannelEvent) -> Result<(), GatewayError> {
        match event {
            ChannelEvent::MessageReceived {
                platform,
                channel_id,
                message,
            } => self.handle_message(platform, &channel_id, message).await,
            _ => {
                debug!("Unhandled channel event: {:?}", event);
                Ok(())
            }
        }
    }

    /// 处理消息
    async fn handle_message(
        &self,
        platform: PlatformType,
        channel_id: &str,
        message: Message,
    ) -> Result<(), GatewayError> {
        // 1. 消息去重检查
        if let Some(msg_id) = message.metadata.get("message_id") {
            if !self
                .deduplicator
                .should_process_key(&platform.to_string(), msg_id)
                .await
            {
                warn!("🔄 重复消息，跳过处理: {}", msg_id);
                return Ok(());
            }
        }

        // 2. 获取或创建会话
        let user_id = message
            .metadata
            .get("sender_id")
            .or_else(|| message.metadata.get("open_id"))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let session = self
            .session_manager
            .get_or_create_session(platform, channel_id, &user_id)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to create session: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        info!("💬 会话 {} - 用户 {} 发送消息", session.id, user_id);

        // 2.5 统一获取/创建 DB session
        let db_session_id = if let Some(ref svc) = self.webchat_service {
            if platform == PlatformType::WebChat {
                // WebChat: 验证前端提供的 session_id，无效则自动创建
                let provided_sid = message
                    .metadata
                    .get("session_id")
                    .cloned()
                    .unwrap_or_else(|| session.id.clone());
                match svc.validate_session(&provided_sid, &user_id).await {
                    Ok(true) => provided_sid,
                    _ => {
                        match svc
                            .get_or_create_channel_session(
                                &user_id,
                                &platform.to_string(),
                                &user_id,
                            )
                            .await
                        {
                            Ok(sid) => sid,
                            Err(e) => {
                                warn!("Failed to get/create webchat session: {}", e);
                                provided_sid
                            }
                        }
                    }
                }
            } else {
                // 外部渠道：按 user_id + channel 查找或创建
                let sender_id = message
                    .metadata
                    .get("sender_id")
                    .cloned()
                    .unwrap_or_else(|| channel_id.to_string());
                match svc
                    .get_or_create_channel_session(&user_id, &platform.to_string(), &sender_id)
                    .await
                {
                    Ok(sid) => sid,
                    Err(e) => {
                        warn!("Failed to get/create channel session: {}", e);
                        session.id.clone()
                    }
                }
            }
        } else {
            session.id.clone()
        };

        // 3. 处理多模态内容（下载图片等）
        let (content, images) = self.process_multimodal(&message).await?;

        // 4. 添加用户消息到会话历史
        let image_urls: Vec<String> = images
            .iter()
            .map(|img| format!("data:{};base64,{},", img.mime_type, img.data))
            .collect();

        self.session_manager
            .add_message(
                &session.id,
                "user",
                &content,
                !images.is_empty(),
                image_urls,
            )
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to add message to session: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 4.5 持久化用户消息
        if let Some(ref svc) = self.webchat_service {
            let _ = svc
                .save_message(
                    &db_session_id,
                    "user",
                    &content,
                    Some(serde_json::json!({
                        "platform": platform.to_string(),
                        "sender_id": user_id,
                        "has_image": !images.is_empty(),
                        "channel_id": channel_id,
                    })),
                    None,
                )
                .await;
        }

        // 5. 构建 LLM 上下文（包含历史消息）
        let history = self
            .session_manager
            .get_history_for_llm(&session.id, 20)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to get session history: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 5.5 Memory 检索
        let (memory_context, _direct_answer) = self.build_memory_context(&content, &None).await;

        // 6. 调用 LLM（注入记忆上下文）
        let llm_response = self
            .call_llm_with_context(&message, &history, &images, &memory_context)
            .await?;

        // 7. 添加助手回复到会话历史
        self.session_manager
            .add_message(&session.id, "assistant", &llm_response, false, vec![])
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to add assistant message: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 7.5 持久化 AI 回复
        if let Some(ref svc) = self.webchat_service {
            let token_usage = serde_json::json!({
                "model": "kimi-k2.5",
                "prompt_tokens": history.len(),
                "completion_tokens": llm_response.len(),
            });
            let _ = svc
                .save_message(
                    &db_session_id,
                    "assistant",
                    &llm_response,
                    Some(serde_json::json!({
                        "platform": platform.to_string(),
                        "channel_id": channel_id,
                    })),
                    Some(token_usage),
                )
                .await;
        }

        // 8. 发送回复
        self.send_reply(platform, channel_id, &message, &llm_response)
            .await?;

        // 9. Memory 回写
        if let Some(ref memory) = self.memory_system {
            use beebotos_agents::memory::markdown_storage::{MarkdownMemoryEntry, MemoryFileType};

            let user_entry = MarkdownMemoryEntry {
                id: Uuid::new_v4(),
                timestamp: chrono::Utc::now(),
                title: format!("User: {}", content.chars().take(30).collect::<String>()),
                content: content.clone(),
                category: "conversation".to_string(),
                importance: 0.5,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("session_id".to_string(), db_session_id.clone());
                    m.insert("user_id".to_string(), user_id.clone());
                    m.insert("role".to_string(), "user".to_string());
                    m.insert("channel".to_string(), platform.to_string());
                    m
                },
                session_id: Some(db_session_id.clone()),
            };
            let _ = memory.store(MemoryFileType::Core, &user_entry, None).await;

            let assistant_entry = MarkdownMemoryEntry {
                id: Uuid::new_v4(),
                timestamp: chrono::Utc::now(),
                title: format!(
                    "Assistant: {}",
                    llm_response.chars().take(30).collect::<String>()
                ),
                content: llm_response.clone(),
                category: "conversation".to_string(),
                importance: 0.5,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("session_id".to_string(), db_session_id.clone());
                    m.insert("user_id".to_string(), user_id.clone());
                    m.insert("role".to_string(), "assistant".to_string());
                    m.insert("channel".to_string(), platform.to_string());
                    m
                },
                session_id: Some(db_session_id.clone()),
            };
            let _ = memory
                .store(MemoryFileType::Core, &assistant_entry, None)
                .await;
        }

        Ok(())
    }

    /// 处理消息（通过 AgentRuntime）
    pub async fn handle_message_via_agent(
        &self,
        platform: PlatformType,
        channel_id: &str,
        message: Message,
        resolver: Arc<AgentResolver>,
        agent_runtime: Arc<dyn gateway::AgentRuntime>,
    ) -> Result<(), GatewayError> {
        // 1. 消息去重检查
        if let Some(msg_id) = message.metadata.get("message_id") {
            if !self
                .deduplicator
                .should_process_key(&platform.to_string(), msg_id)
                .await
            {
                warn!("🔄 重复消息，跳过处理: {}", msg_id);
                return Ok(());
            }
        }

        // 2. 获取或创建会话
        let user_id = message
            .metadata
            .get("sender_id")
            .or_else(|| message.metadata.get("open_id"))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let session = self
            .session_manager
            .get_or_create_session(platform, channel_id, &user_id)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to create session: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        info!("💬 会话 {} - 用户 {} 发送消息", session.id, user_id);

        // 2.5 统一获取/创建 DB session
        let db_session_id = if let Some(ref svc) = self.webchat_service {
            if platform == PlatformType::WebChat {
                // WebChat: 验证前端提供的 session_id，无效则自动创建
                let provided_sid = message
                    .metadata
                    .get("session_id")
                    .cloned()
                    .unwrap_or_else(|| session.id.clone());
                match svc.validate_session(&provided_sid, &user_id).await {
                    Ok(true) => provided_sid,
                    _ => {
                        match svc
                            .get_or_create_channel_session(
                                &user_id,
                                &platform.to_string(),
                                &user_id,
                            )
                            .await
                        {
                            Ok(sid) => sid,
                            Err(e) => {
                                warn!("Failed to get/create webchat session: {}", e);
                                provided_sid
                            }
                        }
                    }
                }
            } else {
                // 外部渠道：按 user_id + channel 查找或创建
                let sender_id = message
                    .metadata
                    .get("sender_id")
                    .cloned()
                    .unwrap_or_else(|| channel_id.to_string());
                match svc
                    .get_or_create_channel_session(&user_id, &platform.to_string(), &sender_id)
                    .await
                {
                    Ok(sid) => sid,
                    Err(e) => {
                        warn!("Failed to get/create channel session: {}", e);
                        session.id.clone()
                    }
                }
            }
        } else {
            session.id.clone()
        };

        // 3. 处理多模态内容（下载图片等）
        let (content, images) = self.process_multimodal(&message).await?;

        // 4. 添加用户消息到会话历史
        let image_urls: Vec<String> = images
            .iter()
            .map(|img| format!("data:{};base64,{},", img.mime_type, img.data))
            .collect();

        self.session_manager
            .add_message(
                &session.id,
                "user",
                &content,
                !images.is_empty(),
                image_urls,
            )
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to add message to session: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 4.5 持久化用户消息
        if let Some(ref svc) = self.webchat_service {
            let _ = svc
                .save_message(
                    &db_session_id,
                    "user",
                    &content,
                    Some(serde_json::json!({
                        "platform": platform.to_string(),
                        "sender_id": user_id,
                        "has_image": !images.is_empty(),
                        "channel_id": channel_id,
                    })),
                    None,
                )
                .await;
        }

        // 5. 解析 agent_id
        let agent_id = resolver.resolve(platform, channel_id, &user_id).await?;

        // 6. 构建 LLM 上下文（包含历史消息）
        let history = self
            .session_manager
            .get_history_for_llm(&session.id, 20)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to get session history: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 6.5 Memory 检索
        // 🆕 FIX: 先匹配 skill，统一在 build_memory_context 内注入 skill prompt
        // 并控制总预算
        let skill_match = self.try_match_skill(&content).await;
        let (memory_context, direct_answer) =
            self.build_memory_context(&content, &skill_match).await;

        // 🟢 P2 FIX: Memory 精确匹配直接返回，跳过 LLM
        if let Some(answer) = direct_answer {
            info!(
                "🧠 P2 FAST PATH: Memory direct answer, skipping Agent/LLM for '{}'",
                content.chars().take(40).collect::<String>()
            );
            // 更新会话历史
            self.session_manager
                .add_message(&session.id, "assistant", &answer, false, vec![])
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to add assistant message: {}", e),
                    correlation_id: Uuid::new_v4().to_string(),
                })?;
            // 发送回复
            self.send_reply(platform, channel_id, &message, &answer)
                .await?;
            return Ok(());
        }

        // 7. 处理 Skill planning 判断
        let mut has_skill_plan = false;
        if let Some((ref skill_name, _, _)) = skill_match {
            // 复杂 skill 强制触发 agent 端 planning
            // 🆕 FIX: 结合 skill 类型与 query 复杂度综合判断是否启用 planning
            let skill_lower = skill_name.to_lowercase();
            let is_generative_skill = skill_lower.contains("travel")
                || skill_lower.contains("planner")
                || skill_lower.contains("writer")
                || skill_lower.contains("creator")
                || skill_lower.contains("story")
                || skill_lower.contains("email")
                || skill_lower.contains("master")
                || skill_lower.contains("game");
            let is_analytical_skill = skill_lower.contains("developer")
                || skill_lower.contains("analyst")
                || skill_lower.contains("advisor")
                || skill_lower.contains("manager")
                || skill_lower.contains("auditor")
                || skill_lower.contains("researcher");

            let query_complexity = Self::estimate_query_complexity(&content);
            let is_high_complexity = query_complexity == QueryComplexity::High;

            if is_analytical_skill && !is_generative_skill {
                has_skill_plan = true;
                info!(
                    "🎯 Analytical skill matched, will inject plan=true for '{}'",
                    skill_name
                );
            } else if is_generative_skill && is_high_complexity {
                // 🆕 FIX: 高复杂度 generative skill 也启用 planning
                has_skill_plan = true;
                info!(
                    "🎯 Generative skill '{}' matched with high complexity query, forcing \
                     plan=true",
                    skill_name
                );
            } else if is_generative_skill {
                info!(
                    "🎯 Generative skill matched, skipping plan=true for '{}' (single-shot \
                     generation preferred)",
                    skill_name
                );
            }
        }

        // 8. 构造 TaskConfig
        let mut task_input = serde_json::json!({
            "message": content,
            "history": history.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
            "images": images.iter().map(|img| format!("data:{};base64,{},", img.mime_type, img.data)).collect::<Vec<_>>(),
            "platform": platform.to_string(),
            "channel_id": channel_id,
            "user_id": user_id,
            "session_id": session.id,
            "metadata": message.metadata,
            "memory_context": memory_context,
        });
        if let Some((skill_name, skill_desc, skill_prompt)) = skill_match {
            if let Some(obj) = task_input.as_object_mut() {
                obj.insert(
                    "skill_hint".to_string(),
                    serde_json::json!({
                        "name": skill_name,
                        "description": skill_desc,
                        "prompt_template": skill_prompt,
                    }),
                );
                if has_skill_plan {
                    obj.insert("plan".to_string(), serde_json::json!("true"));
                }
            }
        }

        let task = gateway::TaskConfig {
            task_type: "llm_chat".to_string(),
            input: task_input,
            timeout_secs: 180,
            priority: 5,
        };

        // 🟢 P2 FIX: 发送"正在思考..."占位消息，然后后台异步执行 Agent
        let placeholder = "🤖 正在思考，请稍候...";
        self.send_reply(platform, channel_id, &message, placeholder)
            .await?;

        // 克隆需要在后台任务中使用的数据
        let processor = Arc::new(MessageProcessor {
            deduplicator: Arc::clone(&self.deduplicator),
            session_manager: Arc::clone(&self.session_manager),
            multimodal_processor: MultimodalProcessor::new(), // placeholder, not used in bg
            llm_service: Arc::clone(&self.llm_service),
            channel_registry: Arc::clone(&self.channel_registry),
            memory_system: self.memory_system.as_ref().map(Arc::clone),
            webchat_service: self.webchat_service.as_ref().map(Arc::clone),
            skill_registry: self.skill_registry.as_ref().map(Arc::clone),
        });
        let session_id = session.id.clone();
        let db_session_id_bg = db_session_id.clone();
        let user_id_bg = user_id.clone();
        let content_bg = content.clone();
        let channel_id_bg = channel_id.to_string();
        let agent_id_bg = agent_id.clone();
        let message_bg = message.clone();
        let platform_bg = platform;
        let agent_runtime_bg = Arc::clone(&agent_runtime);

        tokio::spawn(async move {
            info!("🤖 [BG] Agent {} 开始后台处理消息", agent_id_bg);
            let start = std::time::Instant::now();

            let result = agent_runtime_bg.execute_task(&agent_id_bg, task).await;
            let llm_response = match result {
                Ok(r) if r.success => r
                    .output
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| {
                        r.output
                            .get("response")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "Agent returned empty response".to_string()),
                Ok(r) => r
                    .error
                    .clone()
                    .unwrap_or_else(|| "Agent processing failed".to_string()),
                Err(e) => {
                    error!("❌ [BG] Agent execution failed: {}", e);
                    format!("处理失败: {}", e)
                }
            };

            info!(
                "🤖 [BG] Agent {} 回复 ({}ms): {}",
                agent_id_bg,
                start.elapsed().as_millis(),
                llm_response.chars().take(100).collect::<String>()
            );

            // 更新会话历史
            let _ = processor
                .session_manager
                .add_message(&session_id, "assistant", &llm_response, false, vec![])
                .await;

            // 持久化 AI 回复
            if let Some(ref svc) = processor.webchat_service {
                let _ = svc
                    .save_message(
                        &db_session_id_bg,
                        "assistant",
                        &llm_response,
                        Some(serde_json::json!({
                            "platform": platform_bg.to_string(),
                            "channel_id": channel_id_bg.clone(),
                        })),
                        None,
                    )
                    .await;
            }

            // 发送最终回复
            let _ = processor
                .send_reply(platform_bg, &channel_id_bg, &message_bg, &llm_response)
                .await;

            // Memory 回写
            if let Some(ref memory) = processor.memory_system {
                use beebotos_agents::memory::markdown_storage::{
                    MarkdownMemoryEntry, MemoryFileType,
                };
                let user_entry = MarkdownMemoryEntry {
                    id: Uuid::new_v4(),
                    timestamp: chrono::Utc::now(),
                    title: format!("User: {}", content_bg.chars().take(30).collect::<String>()),
                    content: content_bg.clone(),
                    category: "conversation".to_string(),
                    importance: 0.5,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("session_id".to_string(), db_session_id_bg.clone());
                        m.insert("user_id".to_string(), user_id_bg.clone());
                        m.insert("role".to_string(), "user".to_string());
                        m.insert("channel".to_string(), platform_bg.to_string());
                        m
                    },
                    session_id: Some(db_session_id_bg.clone()),
                };
                let _ = memory.store(MemoryFileType::Core, &user_entry, None).await;

                let assistant_entry = MarkdownMemoryEntry {
                    id: Uuid::new_v4(),
                    timestamp: chrono::Utc::now(),
                    title: format!(
                        "Assistant: {}",
                        llm_response.chars().take(30).collect::<String>()
                    ),
                    content: llm_response.clone(),
                    category: "conversation".to_string(),
                    importance: 0.5,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("session_id".to_string(), db_session_id_bg.clone());
                        m.insert("user_id".to_string(), user_id_bg.clone());
                        m.insert("role".to_string(), "assistant".to_string());
                        m.insert("channel".to_string(), platform_bg.to_string());
                        m
                    },
                    session_id: Some(db_session_id_bg),
                };
                let _ = memory
                    .store(MemoryFileType::Core, &assistant_entry, None)
                    .await;
            }
        });

        Ok(())
    }

    /// 处理多模态内容
    async fn process_multimodal(
        &self,
        message: &Message,
    ) -> Result<(String, Vec<ProcessedImage>), GatewayError> {
        // 检查是否有图片
        if let Some(image_key) = self.extract_image_key(&message.content) {
            info!("🖼️ 检测到图片: {}", image_key);

            // 获取 channel 以下载图片
            if let Some(channel) = self
                .channel_registry
                .get_channel_by_platform(message.platform)
                .await
            {
                let message_id = message.metadata.get("message_id").map(|s| s.as_str());

                // 下载图片
                match channel
                    .read()
                    .await
                    .download_image(&image_key, message_id)
                    .await
                {
                    Ok(image_data) => {
                        // 处理图片
                        let processed = self.process_image(&image_data)?;
                        let text = self.clean_text_content(&message.content);
                        return Ok((text, vec![processed]));
                    }
                    Err(e) => {
                        warn!("图片下载失败: {}", e);
                    }
                }
            }
        }

        // 纯文本消息
        Ok((message.content.clone(), vec![]))
    }

    /// 提取图片 key
    fn extract_image_key(&self, content: &str) -> Option<String> {
        // 匹配 image_key: xxx 格式
        if let Some(pos) = content.find("image_key:") {
            let start = pos + "image_key:".len();
            let rest = &content[start..];
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ']')
                .unwrap_or(rest.len());
            let key = rest[..end].trim();
            if !key.is_empty() {
                return Some(key.to_string());
            }
        }
        None
    }

    /// 清理文本内容
    fn clean_text_content(&self, content: &str) -> String {
        // 移除 image_key 标记
        let re = regex::Regex::new(r"\[?图片\]?\s*image_key:\s*\S+").unwrap();
        re.replace_all(content, "[图片]").to_string()
    }

    /// 处理图片
    fn process_image(&self, data: &[u8]) -> Result<ProcessedImage, GatewayError> {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;

        // 检测图片格式
        let format = self.detect_image_format(data)?;

        // 编码为 base64
        let base64_data = STANDARD.encode(data);

        Ok(ProcessedImage {
            data: base64_data,
            format: format.clone(),
            mime_type: format.mime_type().to_string(),
        })
    }

    /// 检测图片格式
    fn detect_image_format(&self, data: &[u8]) -> Result<ImageFormat, GatewayError> {
        if data.len() < 8 {
            return Err(GatewayError::Internal {
                message: "Image data too small".to_string(),
                correlation_id: Uuid::new_v4().to_string(),
            });
        }

        // PNG: 89 50 4E 47
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return Ok(ImageFormat::Png);
        }
        // JPEG: FF D8 FF
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Ok(ImageFormat::Jpeg);
        }
        // GIF: GIF87a or GIF89a
        if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            return Ok(ImageFormat::Gif);
        }
        // WebP: RIFF....WEBP
        if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
            return Ok(ImageFormat::Webp);
        }

        Err(GatewayError::Internal {
            message: "Unknown image format".to_string(),
            correlation_id: Uuid::new_v4().to_string(),
        })
    }

    /// 🆕 FIX: 尝试匹配 Skill，返回最佳匹配的 skill hint (name, description,
    /// prompt_template) 支持 domain keyword 映射 + registry 语义搜索 + name
    /// 子串匹配
    async fn try_match_skill(&self, content: &str) -> Option<(String, String, String)> {
        // 查询太短不应触发 skill 匹配
        if content.chars().count() < 4 {
            return None;
        }
        let registry = self.skill_registry.as_ref()?;
        let query_lower = content.to_lowercase();

        // 1. Domain keyword → skill ID 快速映射（中文 + 英文）
        let domain_keywords: &[(&[&str], &str)] = &[
            (
                &[
                    "travel",
                    "tour",
                    "trip",
                    "itinerary",
                    "旅游",
                    "旅行",
                    "行程",
                    "攻略",
                    "景点",
                    "酒店",
                    "规划",
                    "计划",
                ],
                "travel_planner",
            ),
            (
                &[
                    "code", "program", "develop", "debug", "coding", "编程", "代码", "开发",
                    "python",
                ],
                "python_developer",
            ),
            (&["rust", "cargo"], "rust_developer"),
            (
                &["contract", "solidity", "smart contract", "合约", "区块链"],
                "solidity_developer",
            ),
            (&["write", "email", "draft", "邮件", "写信"], "email_writer"),
            (
                &["story", "novel", "fiction", "write", "故事", "小说"],
                "story_writer",
            ),
            (&["game", "gaming", "游戏", "玩家"], "game_master"),
            (
                &["data", "analyze", "analysis", "数据", "分析", "统计"],
                "data_analyst",
            ),
            (
                &["image", "photo", "picture", "图", "照片"],
                "image_analyst",
            ),
            (
                &["calendar", "schedule", "meeting", "日历", "会议", "安排"],
                "calendar_assistant",
            ),
            (&["task", "todo", "plan", "任务", "待办"], "task_manager"),
            (
                &["defi", "yield", "liquidity", "farm", "挖矿", "流动性"],
                "yield_farmer",
            ),
            (&["nft", "mint", "token", "数字藏品"], "nft_minter"),
            (
                &["health", "medical", "doctor", "健康", "医疗", "医生"],
                "health_advisor",
            ),
            (
                &["learn", "study", "tutor", "lesson", "学习", "课程", "辅导"],
                "tutor",
            ),
            (
                &["research", "paper", "survey", "研究", "论文", "调查"],
                "code_researcher",
            ),
            (
                &[
                    "dao",
                    "governance",
                    "proposal",
                    "vote",
                    "治理",
                    "提案",
                    "投票",
                ],
                "governance_analyst",
            ),
            (
                &[
                    "finance",
                    "portfolio",
                    "invest",
                    "理财",
                    "投资",
                    "组合",
                    "黄金",
                    "价格",
                ],
                "portfolio_manager",
            ),
            (
                &["social", "community", "content", "社媒", "社群", "内容"],
                "content_creator",
            ),
            (
                &["security", "audit", "vulnerability", "安全", "审计", "漏洞"],
                "auditor",
            ),
            (
                &["weather", "forecast", "天气", "预报", "降雨", "温度"],
                "weather_assistant",
            ),
        ];

        for (keywords, skill_id) in domain_keywords {
            if keywords.iter().any(|kw| query_lower.contains(kw)) {
                if let Some(skill) = registry.get(skill_id).await {
                    if skill.enabled {
                        info!(
                            "🎯 Skill domain matched: '{}' for query '{}'",
                            skill_id,
                            content.chars().take(40).collect::<String>()
                        );
                        return Some((
                            skill.skill.name.clone(),
                            skill.skill.manifest.description.clone(),
                            skill.skill.manifest.prompt_template.clone(),
                        ));
                    }
                }
            }
        }

        // 2. Registry semantic search fallback
        let results = registry.search(content).await;
        if results.is_empty() {
            return None;
        }
        let best = &results[0];
        let name_lower = best.skill.name.to_lowercase();
        // name 子串强匹配
        let is_strong_match =
            name_lower.contains(&query_lower) || query_lower.contains(&name_lower);
        if is_strong_match {
            info!(
                "🎯 Skill matched: '{}' for query '{}'",
                best.skill.id,
                content.chars().take(40).collect::<String>()
            );
            let hint = (
                best.skill.name.clone(),
                best.skill.manifest.description.clone(),
                best.skill.manifest.prompt_template.clone(),
            );
            Some(hint)
        } else {
            debug!(
                "Skill match too weak: '{}' for query '{}'",
                best.skill.id,
                content.chars().take(40).collect::<String>()
            );
            None
        }
    }

    /// P2 FIX: 提取共享的 Memory 搜索逻辑，消除双重搜索
    ///
    /// 🟢 P2 FIX: 返回 (memory_context, direct_answer)。如果 Memory
    /// 中有高置信度的精确匹配问答对， 直接提取答案返回，跳过 LLM 调用。
    ///
    /// 🆕 FIX (方案B): 固定档案与动态记忆分独立预算，简单查询可跳过冗余档案
    async fn build_memory_context(
        &self,
        content: &str,
        skill_match: &Option<(String, String, String)>,
    ) -> (String, Option<String>) {
        let mut memory_context = String::new();
        let mut direct_answer: Option<String> = None;

        // 🆕 FIX: 根据 query 复杂度动态调整参数
        let char_count = content.chars().count();
        let is_simple = char_count <= 10;
        let is_complex = char_count > 30
            || content.contains("计划")
            || content.contains("规划")
            || content.contains("步骤")
            || content.contains("安排")
            || content.contains("攻略")
            || content.contains("对比")
            || content.contains("分析")
            || content.contains("总结");
        let search_limit = if is_complex {
            6
        } else if char_count > 15 {
            4
        } else {
            2
        };

        // 🆕 FIX (方案B): 独立预算体系
        // 简单查询：固定档案 300 chars + 动态记忆 400 chars
        // 普通查询：固定档案 600 chars + 动态记忆 800 chars
        // 复杂查询：固定档案 1000 chars + 动态记忆 1200 chars
        let (system_budget, dynamic_budget): (usize, usize) = if is_simple {
            (300, 400)
        } else if is_complex {
            (1000, 1200)
        } else {
            (600, 800)
        };
        // 🆕 FIX: 当外部注入了大段 skill prompt 等额外 context 时，相应缩减 dynamic
        // memory budget
        let extra_context_len = skill_match.as_ref().map_or(0, |(name, _, prompt)| {
            let wrapper_len = format!(
                "\n\n[系统提示：你当前正在使用 {} 技能处理此请求。请遵循以下专业指引]\n",
                name
            )
            .chars()
            .count();
            prompt.chars().count() + wrapper_len
        });
        let adjusted_dynamic_budget = dynamic_budget.saturating_sub(extra_context_len).max(150);

        // 🆕 FIX: 前缀文本长度预扣，确保各段总长度（含前缀）不超预算
        let system_prefix =
            "\n\n[系统提示：以下是该用户的固定档案和AI人格设定，回答时必须始终遵守]\n";
        let dynamic_prefix = "\n\n[系统提示：以下是该用户的历史记忆，回答时必须结合这些信息]\n";
        let system_context_budget = system_budget.saturating_sub(system_prefix.chars().count());
        let dynamic_context_budget =
            adjusted_dynamic_budget.saturating_sub(dynamic_prefix.chars().count());

        // 🆕 FIX: 预加载 USER.md 和 SOUL.md 作为固定系统上下文
        if let Some(ref memory) = self.memory_system {
            let storage = memory.storage();
            let mut system_context = String::new();

            if is_simple {
                // 🆕 FIX: 极简模式也加载核心用户档案（名字、语言偏好等关键字段）
                // 先加载 USER.md 的前 2 条有效关键信息
                if let Ok(entries) = storage
                    .read_entries(beebotos_agents::memory::MemoryFileType::User, None)
                    .await
                {
                    let mut user_parts = Vec::new();
                    for entry in entries {
                        let trimmed = entry.content.trim();
                        let is_placeholder = trimmed.contains("*To be filled")
                            || trimmed.starts_with("- Name:") && trimmed.len() < 12
                            || trimmed.starts_with("- Preferred language:") && trimmed.len() < 25
                            || trimmed.starts_with("- Timezone:") && trimmed.len() < 15
                            || trimmed.starts_with("- Communication style:") && trimmed.len() < 26
                            || trimmed.starts_with("- Notification preferences:")
                                && trimmed.len() < 31
                            || trimmed.starts_with("- Professional background:")
                                && trimmed.len() < 30
                            || trimmed.starts_with("- Technical skills:") && trimmed.len() < 23
                            || trimmed.starts_with("- Hobbies:") && trimmed.len() < 14;
                        if !trimmed.is_empty() && !is_placeholder {
                            user_parts.push(trimmed.to_string());
                            if user_parts.len() >= 1 {
                                break;
                            } // 🆕 FIX: 简单模式只取1条最关键档案，给SOUL.
                              // md留空间
                        }
                    }
                    if !user_parts.is_empty() {
                        system_context.push_str("## 用户档案\n");
                        for part in &user_parts {
                            system_context.push_str(&part);
                            system_context.push('\n');
                        }
                        info!(
                            "📄 Simple query mode: loaded USER.md core profile ({} entries)",
                            user_parts.len()
                        );
                    }
                }

                // 再加载 SOUL.md 的第一句核心人格描述
                if let Ok(entries) = storage
                    .read_entries(beebotos_agents::memory::MemoryFileType::Soul, None)
                    .await
                {
                    for entry in entries {
                        let trimmed = entry.content.trim();
                        if !trimmed.is_empty()
                            && !trimmed.starts_with('#')
                            && !trimmed.starts_with("---")
                        {
                            let first_line = trimmed.lines().next().unwrap_or(trimmed);
                            if first_line.len() > 10 {
                                if !system_context.is_empty() {
                                    system_context.push('\n');
                                }
                                system_context.push_str("## AI 人格设定\n");
                                system_context.push_str(first_line);
                                system_context.push('\n');
                                break;
                            }
                        }
                    }
                }
                if system_context.is_empty() {
                    system_context =
                        "你是 BeeBotOS 的个人 AI 助手，用中文友好地回答用户。\n".to_string();
                }
                info!(
                    "📄 Simple query mode: loaded minimal persona ({} chars)",
                    system_context.chars().count()
                );
            } else {
                // 标准模式：加载 USER.md + SOUL.md
                // Read USER.md
                match storage
                    .read_entries(beebotos_agents::memory::MemoryFileType::User, None)
                    .await
                {
                    Ok(entries) => {
                        let mut user_parts = Vec::new();
                        for entry in entries {
                            let trimmed = entry.content.trim();
                            let is_placeholder = trimmed.contains("*To be filled")
                                || trimmed.starts_with("- Name:") && trimmed.len() < 12
                                || trimmed.starts_with("- Preferred language:")
                                    && trimmed.len() < 25
                                || trimmed.starts_with("- Timezone:") && trimmed.len() < 15
                                || trimmed.starts_with("- Communication style:")
                                    && trimmed.len() < 26
                                || trimmed.starts_with("- Notification preferences:")
                                    && trimmed.len() < 31
                                || trimmed.starts_with("- Professional background:")
                                    && trimmed.len() < 30
                                || trimmed.starts_with("- Technical skills:") && trimmed.len() < 23
                                || trimmed.starts_with("- Hobbies:") && trimmed.len() < 14;
                            if !trimmed.is_empty() && !is_placeholder {
                                user_parts.push(trimmed.to_string());
                            }
                        }
                        if !user_parts.is_empty() {
                            system_context.push_str("## 用户档案\n");
                            for part in &user_parts {
                                system_context.push_str(&part);
                                system_context.push('\n');
                            }
                            system_context.push('\n');
                            info!("📄 Loaded USER.md profile ({} entries)", user_parts.len());
                        } else {
                            info!("📄 USER.md loaded but no valid entries after filtering");
                        }
                    }
                    Err(e) => {
                        warn!("📄 Failed to load USER.md: {}", e);
                    }
                }

                // Read SOUL.md
                match storage
                    .read_entries(beebotos_agents::memory::MemoryFileType::Soul, None)
                    .await
                {
                    Ok(entries) => {
                        let mut soul_parts = Vec::new();
                        for entry in entries {
                            let trimmed = entry.content.trim();
                            let is_placeholder = trimmed.contains("Helpful and friendly")
                                && trimmed.len() < 30
                                || trimmed.starts_with("- Professional but approachable")
                                    && trimmed.len() < 35
                                || trimmed.starts_with("- Detail-oriented") && trimmed.len() < 20
                                || trimmed.starts_with("- Clear and concise") && trimmed.len() < 22
                                || trimmed.starts_with("- Use examples when helpful")
                                    && trimmed.len() < 30
                                || trimmed.starts_with("- Ask clarifying questions when needed")
                                    && trimmed.len() < 42
                                || trimmed.starts_with("- Respect user privacy")
                                    && trimmed.len() < 25
                                || trimmed.starts_with("- Decline harmful requests")
                                    && trimmed.len() < 30
                                || trimmed.starts_with("- Be honest about limitations")
                                    && trimmed.len() < 32;
                            if !trimmed.is_empty() && !is_placeholder {
                                soul_parts.push(trimmed.to_string());
                            }
                        }
                        if !soul_parts.is_empty() {
                            system_context.push_str("## AI 人格设定\n");
                            for part in &soul_parts {
                                system_context.push_str(&part);
                                system_context.push('\n');
                            }
                            system_context.push('\n');
                            info!("📄 Loaded SOUL.md profile ({} entries)", soul_parts.len());
                        } else {
                            info!("📄 SOUL.md loaded but no valid entries after filtering");
                        }
                    }
                    Err(e) => {
                        warn!("📄 Failed to load SOUL.md: {}", e);
                    }
                }
            }

            // 🆕 FIX (方案B): 对固定档案做硬截断（统一字符计数，已预扣前缀长度）
            if !system_context.is_empty() {
                let system_chars = system_context.chars().count();
                if system_chars > system_context_budget {
                    let suffix = "\n...（档案已精简）\n";
                    let suffix_len = suffix.chars().count();
                    let truncate_limit = system_context_budget.saturating_sub(suffix_len);

                    let mut truncated = String::new();
                    let mut char_count = 0;
                    for ch in system_context.chars() {
                        if char_count >= truncate_limit {
                            break;
                        }
                        truncated.push(ch);
                        char_count += 1;
                    }
                    truncated.push_str(suffix);
                    system_context = truncated;

                    debug_assert!(
                        system_context.chars().count() <= system_context_budget,
                        "System context truncation failed: {} > {}",
                        system_context.chars().count(),
                        system_context_budget
                    );
                    info!(
                        "📄 System context truncated to {} chars (budget={})",
                        system_context.chars().count(),
                        system_budget
                    );
                }
                memory_context.push_str(system_prefix);
                memory_context.push_str(&system_context);
            }

            match memory.search(content, search_limit).await {
                Ok(results) if !results.is_empty() => {
                    info!(
                        "Memory search returned {} results (limit={}) for query '{}'",
                        results.len(),
                        search_limit,
                        content.chars().take(40).collect::<String>()
                    );
                    let content_lower = content.to_lowercase().trim().to_string();

                    // 🟢 P2 FIX: 检查是否有精确问答对可直接返回
                    for r in &results {
                        let mem_lower = r.entry.content.to_lowercase();
                        if mem_lower.contains(&content_lower) {
                            for marker in &["assistant:", "答：", "a:", "回答：", "助手："]
                            {
                                if let Some(pos) = mem_lower.find(marker) {
                                    let answer =
                                        r.entry.content[pos + marker.len()..].trim().to_string();
                                    if answer.chars().count() > 5 && answer.chars().count() < 500 {
                                        info!(
                                            "🧠 P2 MEMORY DIRECT HIT: 精确匹配，直接返回答案 ({} \
                                             chars)",
                                            answer.chars().count()
                                        );
                                        direct_answer = Some(answer);
                                        break;
                                    }
                                }
                            }
                            if direct_answer.is_some() {
                                break;
                            }
                        }
                    }

                    let filtered: Vec<_> = results
                        .iter()
                        .filter(|r| !r.entry.content.to_lowercase().contains(&content_lower))
                        .take(search_limit)
                        .collect();
                    if !filtered.is_empty() {
                        memory_context.push_str(dynamic_prefix);
                        // 🆕 FIX (方案B): 动态记忆独立预算，从 0 开始计算（已预扣前缀长度）
                        // 🆕 FIX: 单条记忆最多 200 chars，避免一条超长记忆占满 budget
                        const MAX_ENTRY_LEN: usize = 200;
                        let mut total_chars = 0;
                        for r in filtered {
                            let mut entry_text = r.entry.content.clone();
                            let entry_text_chars = entry_text.chars().count();
                            if entry_text_chars > MAX_ENTRY_LEN {
                                let mut truncated = String::new();
                                let mut char_count = 0;
                                for ch in entry_text.chars() {
                                    if char_count >= MAX_ENTRY_LEN - 3 {
                                        // 留 3 字符给 "..."
                                        break;
                                    }
                                    truncated.push(ch);
                                    char_count += 1;
                                }
                                truncated.push_str("...");
                                entry_text = truncated;
                            }
                            let entry = format!("- {}\n", entry_text);
                            let entry_chars = entry.chars().count();
                            if total_chars + entry_chars > dynamic_context_budget {
                                memory_context.push_str("- ...（更多记忆已省略）\n");
                                break;
                            }
                            memory_context.push_str(&entry);
                            total_chars += entry_chars;
                        }
                        info!(
                            "Injecting memory context ({} chars, system_budget={}, \
                             dynamic_budget={}) into LLM prompt",
                            memory_context.chars().count(),
                            system_budget,
                            adjusted_dynamic_budget
                        );
                    } else {
                        info!("All memory results were self-referential, skipping injection");
                    }
                }
                Ok(_) => {
                    info!(
                        "Memory search returned no results for query '{}'",
                        content.chars().take(40).collect::<String>()
                    );
                }
                Err(e) => {
                    warn!("Memory search failed: {}", e);
                }
            }
        }

        // 🆕 FIX: 统一注入 skill prompt（无论 memory_system 是否存在）
        if let Some((ref skill_name, _, ref skill_prompt)) = skill_match {
            if !skill_prompt.is_empty() {
                let injection = format!(
                    "\n\n[系统提示：你当前正在使用 {} 技能处理此请求。请遵循以下专业指引]\n{}",
                    skill_name, skill_prompt
                );
                memory_context.push_str(&injection);
                info!(
                    "🎯 Skill prompt injected ({} chars) for '{}'",
                    skill_prompt.chars().count(),
                    skill_name
                );
            }
        }

        // 🆕 FIX: 总预算防御性截断
        let total_budget = system_budget + dynamic_budget;
        let current_chars = memory_context.chars().count();
        if current_chars > total_budget {
            let suffix = "\n...[上下文已精简]\n";
            let keep_chars = total_budget.saturating_sub(suffix.chars().count());
            memory_context = Self::truncate_to_chars(&memory_context, keep_chars);
            memory_context.push_str(suffix);
            warn!(
                "🎯 Total memory context truncated from {} to {} chars (total_budget={})",
                current_chars,
                memory_context.chars().count(),
                total_budget
            );
        }

        (memory_context, direct_answer)
    }

    /// 调用 LLM 并传入上下文
    async fn call_llm_with_context(
        &self,
        message: &Message,
        history: &[SessionMessage],
        images: &[ProcessedImage],
        memory_context: &str,
    ) -> Result<String, GatewayError> {
        // 构建包含历史和记忆的提示
        let mut context = String::new();

        if !memory_context.is_empty() {
            context.push_str("以下是与当前对话相关的历史记忆，供你参考：\n");
            context.push_str(memory_context);
            context.push_str("\n\n");
        }

        for msg in history.iter().take(history.len().saturating_sub(1)) {
            let role = match msg.role.as_str() {
                "user" => "用户",
                "assistant" => "助手",
                _ => &msg.role,
            };
            context.push_str(&format!("{}: {}\n", role, msg.content));
        }

        // 当前消息
        context.push_str(&format!("用户: {}\n", message.content));

        info!("🤖 调用 LLM，上下文长度: {} 字符", context.len());

        // P1 FIX: 实际使用构建的 context，而非忽略它
        let mut contextual_message = message.clone();
        contextual_message.content = context;
        self.llm_service.process_message(&contextual_message).await
    }

    /// 发送回复
    async fn send_reply(
        &self,
        platform: PlatformType,
        channel_id: &str,
        original: &Message,
        response: &str,
    ) -> Result<(), GatewayError> {
        // 检查回复中是否包含图片标记
        if response.contains("![") && response.contains("](") {
            // 需要发送图文混合消息
            self.send_mixed_message(platform, channel_id, original, response)
                .await
        } else {
            // 纯文本回复
            let reply = Message {
                id: Uuid::new_v4(),
                thread_id: original.thread_id,
                platform,
                message_type: MessageType::Text,
                content: response.to_string(),
                metadata: HashMap::new(),
                timestamp: chrono::Utc::now(),
            };

            if let Some(channel) = self
                .channel_registry
                .get_channel_by_platform(platform)
                .await
            {
                channel
                    .read()
                    .await
                    .send(channel_id, &reply)
                    .await
                    .map_err(|e| GatewayError::Internal {
                        message: format!("Failed to send reply: {}", e),
                        correlation_id: Uuid::new_v4().to_string(),
                    })?;

                info!("✅ 回复已发送到 {:?} 频道 {}", platform, channel_id);
            }

            Ok(())
        }
    }

    /// 发送图文混合消息
    async fn send_mixed_message(
        &self,
        platform: PlatformType,
        channel_id: &str,
        original: &Message,
        response: &str,
    ) -> Result<(), GatewayError> {
        // 提取文本和图片
        let parts = self.parse_mixed_content(response);

        for part in parts {
            match part {
                MessagePart::Text(text) => {
                    let reply = Message {
                        id: Uuid::new_v4(),
                        thread_id: original.thread_id,
                        platform,
                        message_type: MessageType::Text,
                        content: text,
                        metadata: HashMap::new(),
                        timestamp: chrono::Utc::now(),
                    };

                    if let Some(channel) = self
                        .channel_registry
                        .get_channel_by_platform(platform)
                        .await
                    {
                        if let Err(e) = channel.read().await.send(channel_id, &reply).await {
                            error!("发送文本消息失败: {}", e);
                        }
                    }
                }
                MessagePart::Image { data, mime_type } => {
                    // 发送图片
                    self.send_image(platform, channel_id, original, &data, &mime_type)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// 解析混合内容
    fn parse_mixed_content(&self, content: &str) -> Vec<MessagePart> {
        let mut parts = Vec::new();
        let mut last_end = 0;

        // 匹配 markdown 图片 ![alt](url)
        // 使用lazy_static避免重复编译正则表达式
        static IMAGE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let re = IMAGE_RE.get_or_init(|| {
            regex::Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").expect("Invalid regex pattern")
        });

        for cap in re.captures_iter(content) {
            let full_match = match cap.get(0) {
                Some(m) => m,
                None => continue,
            };
            let start = full_match.start();
            let end = full_match.end();

            // 添加前面的文本
            if start > last_end {
                let text = content[last_end..start].trim();
                if !text.is_empty() {
                    parts.push(MessagePart::Text(text.to_string()));
                }
            }

            // 添加图片
            let url = &cap[2];
            if url.starts_with("data:image") {
                // base64 编码的图片
                if let Some((mime_type, data)) = self.parse_data_url(url) {
                    parts.push(MessagePart::Image { data, mime_type });
                }
            }

            last_end = end;
        }

        // 添加剩余文本
        if last_end < content.len() {
            let text = content[last_end..].trim();
            if !text.is_empty() {
                parts.push(MessagePart::Text(text.to_string()));
            }
        }

        parts
    }

    /// 解析 data URL
    fn parse_data_url(&self, url: &str) -> Option<(String, String)> {
        // data:image/png;base64,xxxx
        let prefix = "data:image/";
        if !url.starts_with(prefix) {
            return None;
        }

        let rest = &url[prefix.len()..];
        let semi_pos = rest.find(';')?;
        let comma_pos = rest.find(',')?;

        let format = &rest[..semi_pos];
        let data = &rest[comma_pos + 1..];

        Some((format!("image/{}", format), data.to_string()))
    }

    /// 发送图片
    async fn send_image(
        &self,
        platform: PlatformType,
        channel_id: &str,
        original: &Message,
        image_data: &str,
        mime_type: &str,
    ) -> Result<(), GatewayError> {
        // 解码 base64
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;

        let data = STANDARD
            .decode(image_data)
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to decode image: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 创建图片消息
        let mut metadata = HashMap::new();
        metadata.insert("image_data".to_string(), image_data.to_string());
        metadata.insert("mime_type".to_string(), mime_type.to_string());

        let reply = Message {
            id: Uuid::new_v4(),
            thread_id: original.thread_id,
            platform,
            message_type: MessageType::Image,
            content: format!("[图片] {} bytes", data.len()),
            metadata,
            timestamp: chrono::Utc::now(),
        };

        if let Some(channel) = self
            .channel_registry
            .get_channel_by_platform(platform)
            .await
        {
            channel
                .read()
                .await
                .send(channel_id, &reply)
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to send image: {}", e),
                    correlation_id: Uuid::new_v4().to_string(),
                })?;

            info!("✅ 图片已发送到 {:?} 频道 {}", platform, channel_id);
        }

        Ok(())
    }

    /// 🆕 FIX: 按字符截断字符串
    fn truncate_to_chars(s: &str, limit: usize) -> String {
        let mut result = String::new();
        let mut count = 0;
        for ch in s.chars() {
            if count >= limit {
                break;
            }
            result.push(ch);
            count += 1;
        }
        result
    }

    /// 🆕 FIX: 评估查询复杂度
    fn estimate_query_complexity(query: &str) -> QueryComplexity {
        let len = query.chars().count();
        let complex_keywords = [
            "计划", "规划", "分析", "对比", "步骤", "方案", "周", "预算", "攻略", "安排", "行程",
        ];
        let keyword_score = complex_keywords
            .iter()
            .filter(|k| query.contains(**k))
            .count();

        if len > 15 || keyword_score >= 2 {
            QueryComplexity::High
        } else if len > 8 || keyword_score >= 1 {
            QueryComplexity::Medium
        } else {
            QueryComplexity::Low
        }
    }
}

/// 查询复杂度等级，用于判断 Skill Planning 是否需要启用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryComplexity {
    /// 简单短查询，如 "hi"、"你好"
    Low,
    /// 中等查询，含一个复杂关键词或长度稍长
    Medium,
    /// 复杂查询，含多个关键词或长句，需要多步规划
    High,
}

/// 处理后的图片
#[derive(Debug, Clone)]
pub struct ProcessedImage {
    pub data: String,
    pub format: ImageFormat,
    pub mime_type: String,
}

/// 图片格式
#[derive(Debug, Clone)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl ImageFormat {
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Webp => "image/webp",
        }
    }
}

/// 消息部分
enum MessagePart {
    Text(String),
    Image { data: String, mime_type: String },
}
