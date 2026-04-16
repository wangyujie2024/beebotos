# 实现 Web 管理后台「聊天」页面（含 Memory 协同）

## Phase 1：统一数据层
- [ ] 1.1 创建 `migrations_sqlite/003_add_chat_tables.sql`
- [ ] 1.2 实现 `apps/gateway/src/services/webchat_service.rs`
- [ ] 1.3 修改 `apps/gateway/src/services/mod.rs` 导出 webchat_service
- [ ] 1.4 实现 `apps/gateway/src/handlers/http/webchat.rs` REST API
- [ ] 1.5 修改 `apps/gateway/src/handlers/http/mod.rs` 导出 webchat
- [ ] 1.6 在 `apps/gateway/src/main.rs` 注册路由并注入 `WebchatService`

## Phase 2：Memory 协同完整实现
- [ ] 2.1 修改 `AppState` 增加 `memory_system` 字段
- [ ] 2.2 在 `main.rs` 中初始化 `UnifiedMemorySystem`
- [ ] 2.3 修改 `MessageProcessor` 注入 `memory_system` 和 `webchat_service`
- [ ] 2.4 在 `MessageProcessor` 中实现记忆检索并注入 LLM context
- [ ] 2.5 在 `MessageProcessor` 中实现消息处理后回写 Memory
- [ ] 2.6 在 `MessageProcessor` 中实现聊天消息持久化（user + assistant）

## Phase 3：AgentRuntime Memory 注入
- [ ] 3.1 修改 `AgentRuntimeManager` 注入 `memory_system`
- [ ] 3.2 修改 `AgentInstance::new` 给 Agent 注入 memory_system
- [ ] 3.3 修改 `main.rs` 初始化时传入 memory_system

## Phase 4：前端重构
- [ ] 4.1 修改 `apps/web/src/api/webchat.rs` 修正 user_id
- [ ] 4.2 修改 `apps/web/src/pages/webchat.rs`：挂载时加载会话列表
- [ ] 4.3 修改 `apps/web/src/pages/webchat.rs`：切换会话加载历史消息
- [ ] 4.4 修改 `apps/web/src/pages/webchat.rs`：新建会话调用后端 API
- [ ] 4.5 替换内联组件为 `SessionList`、`SidePanel`、`UsagePanelComponent`

## Phase 5：前端美化
- [ ] 5.1 补充/新建 CSS（会话列表、消息气泡、输入区、头部、滑入动画）

## Phase 6：验证
- [ ] 6.1 `cargo build --workspace` 编译通过
- [ ] 6.2 启动服务，验证聊天页面完整流程
- [ ] 6.3 验证消息持久化和 Memory 回写/检索

---

## Review
