//! 消息处理器
//!
//! 集成消息去重、会话管理、多模态处理、Memory 协同和持久化

use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use beebotos_agents::{
    ChannelRegistry,
    deduplicator::MessageDeduplicator,
    communication::{Message, MessageType, PlatformType, channel::ChannelEvent},
    communication::channel::session_manager::{SessionManager, SessionMessage},
    media::multimodal::MultimodalProcessor,
};

use crate::services::llm_service::LlmService;
use crate::services::agent_resolver::AgentResolver;
use crate::services::webchat_service::WebchatService;
use crate::error::GatewayError;

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
}

impl MessageProcessor {
    /// 创建新的消息处理器
    pub fn new(
        llm_service: Arc<LlmService>,
        channel_registry: Arc<ChannelRegistry>,
        memory_system: Option<Arc<beebotos_agents::memory::UnifiedMemorySystem>>,
        webchat_service: Option<Arc<WebchatService>>,
    ) -> Self {
        Self {
            deduplicator: Arc::new(MessageDeduplicator::default()),
            session_manager: SessionManager::default(),
            multimodal_processor: MultimodalProcessor::new(),
            llm_service,
            channel_registry,
            memory_system,
            webchat_service,
        }
    }

    /// 处理频道事件
    pub async fn process_event(&self, event: ChannelEvent) -> Result<(), GatewayError> {
        match event {
            ChannelEvent::MessageReceived { platform, channel_id, message } => {
                self.handle_message(platform, &channel_id, message).await
            }
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
            if !self.deduplicator.should_process_key(&platform.to_string(), msg_id).await {
                warn!("🔄 重复消息，跳过处理: {}", msg_id);
                return Ok(());
            }
        }

        // 2. 获取或创建会话
        let user_id = message.metadata.get("sender_id")
            .or_else(|| message.metadata.get("open_id"))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let session = self.session_manager
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
                let provided_sid = message.metadata.get("session_id")
                    .cloned()
                    .unwrap_or_else(|| session.id.clone());
                match svc.validate_session(&provided_sid, &user_id).await {
                    Ok(true) => provided_sid,
                    _ => {
                        match svc.get_or_create_channel_session(&user_id, &platform.to_string(), &user_id).await {
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
                let sender_id = message.metadata.get("sender_id").cloned().unwrap_or_else(|| channel_id.to_string());
                match svc.get_or_create_channel_session(&user_id, &platform.to_string(), &sender_id).await {
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
        let image_urls: Vec<String> = images.iter()
            .map(|img| format!("data:{};base64,{},", img.mime_type, img.data))
            .collect();

        self.session_manager
            .add_message(&session.id, "user", &content, !images.is_empty(), image_urls)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to add message to session: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 4.5 持久化用户消息
        if let Some(ref svc) = self.webchat_service {
            let _ = svc.save_message(
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
            ).await;
        }

        // 5. 构建 LLM 上下文（包含历史消息）
        let history = self.session_manager
            .get_history_for_llm(&session.id, 20)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to get session history: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 5.5 Memory 检索
        let mut memory_context = String::new();
        if let Some(ref memory) = self.memory_system {
            match memory.search(&content, 5).await {
                Ok(results) if !results.is_empty() => {
                    info!("Memory search returned {} results for query '{}'", results.len(), content.chars().take(40).collect::<String>());
                    let content_lower = content.to_lowercase();
                    let filtered: Vec<_> = results.iter()
                        .filter(|r| !r.entry.content.to_lowercase().contains(&content_lower))
                        .take(5)
                        .collect();
                    if !filtered.is_empty() {
                        memory_context.push_str("\n\n[系统提示：以下是该用户的历史记忆，回答时必须结合这些信息]\n");
                        for r in filtered {
                            memory_context.push_str(&format!("- {}\n", r.entry.content));
                        }
                        info!("Injecting memory context ({} chars) into LLM prompt", memory_context.len());
                    } else {
                        info!("All memory results were self-referential, skipping injection");
                    }
                }
                Ok(_) => {
                    info!("Memory search returned no results for query '{}'", content.chars().take(40).collect::<String>());
                }
                Err(e) => {
                    warn!("Memory search failed: {}", e);
                }
            }
        }

        // 6. 调用 LLM（注入记忆上下文）
        let llm_response = self.call_llm_with_context(&message, &history, &images, &memory_context).await?;

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
            let _ = svc.save_message(
                &db_session_id,
                "assistant",
                &llm_response,
                Some(serde_json::json!({
                    "platform": platform.to_string(),
                    "channel_id": channel_id,
                })),
                Some(token_usage),
            ).await;
        }

        // 8. 发送回复
        self.send_reply(platform, channel_id, &message, &llm_response).await?;

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
                title: format!("Assistant: {}", llm_response.chars().take(30).collect::<String>()),
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
            let _ = memory.store(MemoryFileType::Core, &assistant_entry, None).await;
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
            if !self.deduplicator.should_process_key(&platform.to_string(), msg_id).await {
                warn!("🔄 重复消息，跳过处理: {}", msg_id);
                return Ok(());
            }
        }

        // 2. 获取或创建会话
        let user_id = message.metadata.get("sender_id")
            .or_else(|| message.metadata.get("open_id"))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let session = self.session_manager
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
                let provided_sid = message.metadata.get("session_id")
                    .cloned()
                    .unwrap_or_else(|| session.id.clone());
                match svc.validate_session(&provided_sid, &user_id).await {
                    Ok(true) => provided_sid,
                    _ => {
                        match svc.get_or_create_channel_session(&user_id, &platform.to_string(), &user_id).await {
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
                let sender_id = message.metadata.get("sender_id").cloned().unwrap_or_else(|| channel_id.to_string());
                match svc.get_or_create_channel_session(&user_id, &platform.to_string(), &sender_id).await {
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
        let image_urls: Vec<String> = images.iter()
            .map(|img| format!("data:{};base64,{},", img.mime_type, img.data))
            .collect();

        self.session_manager
            .add_message(&session.id, "user", &content, !images.is_empty(), image_urls)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to add message to session: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 4.5 持久化用户消息
        if let Some(ref svc) = self.webchat_service {
            let _ = svc.save_message(
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
            ).await;
        }

        // 5. 解析 agent_id
        let agent_id = resolver.resolve(platform, channel_id, &user_id).await?;

        // 6. 构建 LLM 上下文（包含历史消息）
        let history = self.session_manager
            .get_history_for_llm(&session.id, 20)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to get session history: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 6.5 Memory 检索
        let mut memory_context = String::new();
        if let Some(ref memory) = self.memory_system {
            match memory.search(&content, 5).await {
                Ok(results) if !results.is_empty() => {
                    info!("Memory search returned {} results for query '{}'", results.len(), content.chars().take(40).collect::<String>());
                    let content_lower = content.to_lowercase();
                    let filtered: Vec<_> = results.iter()
                        .filter(|r| !r.entry.content.to_lowercase().contains(&content_lower))
                        .take(5)
                        .collect();
                    if !filtered.is_empty() {
                        memory_context.push_str("\n\n[系统提示：以下是该用户的历史记忆，回答时必须结合这些信息]\n");
                        for r in filtered {
                            memory_context.push_str(&format!("- {}\n", r.entry.content));
                        }
                        info!("Injecting memory context ({} chars) into agent LLM prompt", memory_context.len());
                    } else {
                        info!("All memory results were self-referential, skipping injection");
                    }
                }
                Ok(_) => {
                    info!("Memory search returned no results for query '{}'", content.chars().take(40).collect::<String>());
                }
                Err(e) => {
                    warn!("Memory search failed: {}", e);
                }
            }
        }

        // 7. 构造 TaskConfig 并调用 AgentRuntime
        let task_input = serde_json::json!({
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

        let task = gateway::TaskConfig {
            task_type: "llm_chat".to_string(),
            input: task_input,
            timeout_secs: 60,
            priority: 5,
        };

        info!("🤖 调用 Agent {} 处理消息", agent_id);
        let result = agent_runtime.execute_task(&agent_id, task).await
            .map_err(|e| GatewayError::Internal {
                message: format!("Agent execution failed: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        let llm_response = if !result.success {
            result.error.clone().unwrap_or_else(|| "Agent processing failed".to_string())
        } else {
            result.output.as_str()
                .map(|s| s.to_string())
                .or_else(|| result.output.get("response").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .unwrap_or_else(|| "Agent returned empty response".to_string())
        };

        info!("🤖 Agent {} 回复: {}", agent_id, llm_response);

        // 8. 添加助手回复到会话历史
        self.session_manager
            .add_message(&session.id, "assistant", &llm_response, false, vec![])
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to add assistant message: {}", e),
                correlation_id: Uuid::new_v4().to_string(),
            })?;

        // 8.5 持久化 AI 回复
        if let Some(ref svc) = self.webchat_service {
            let token_usage = serde_json::json!({
                "model": "kimi-k2.5",
                "prompt_tokens": history.len(),
                "completion_tokens": llm_response.len(),
            });
            let _ = svc.save_message(
                &db_session_id,
                "assistant",
                &llm_response,
                Some(serde_json::json!({
                    "platform": platform.to_string(),
                    "channel_id": channel_id,
                })),
                Some(token_usage),
            ).await;
        }

        // 9. 发送回复
        self.send_reply(platform, channel_id, &message, &llm_response).await?;

        // 10. Memory 回写
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
                title: format!("Assistant: {}", llm_response.chars().take(30).collect::<String>()),
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
            let _ = memory.store(MemoryFileType::Core, &assistant_entry, None).await;
        }

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
            if let Some(channel) = self.channel_registry
                .get_channel_by_platform(message.platform)
                .await
            {
                let message_id = message.metadata.get("message_id").map(|s| s.as_str());

                // 下载图片
                match channel.read().await.download_image(&image_key, message_id).await {
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
            let end = rest.find(|c: char| c.is_whitespace() || c == ']')
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
        use base64::{Engine as _, engine::general_purpose::STANDARD};

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

        // 调用 LLM 服务
        self.llm_service.process_message(message).await
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
            self.send_mixed_message(platform, channel_id, original, response).await
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

            if let Some(channel) = self.channel_registry.get_channel_by_platform(platform).await {
                channel.read().await.send(channel_id, &reply).await
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

                    if let Some(channel) = self.channel_registry.get_channel_by_platform(platform).await {
                        if let Err(e) = channel.read().await.send(channel_id, &reply).await {
                            error!("发送文本消息失败: {}", e);
                        }
                    }
                }
                MessagePart::Image { data, mime_type } => {
                    // 发送图片
                    self.send_image(platform, channel_id, original, &data, &mime_type).await?;
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
        use base64::{Engine as _, engine::general_purpose::STANDARD};

        let data = STANDARD.decode(image_data)
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

        if let Some(channel) = self.channel_registry.get_channel_by_platform(platform).await {
            channel.read().await.send(channel_id, &reply).await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to send image: {}", e),
                    correlation_id: Uuid::new_v4().to_string(),
                })?;

            info!("✅ 图片已发送到 {:?} 频道 {}", platform, channel_id);
        }

        Ok(())
    }
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
