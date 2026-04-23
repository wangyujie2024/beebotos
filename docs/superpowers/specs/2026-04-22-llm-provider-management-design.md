# LLM 提供商管理页面设计文档

## 1. 概述

为 BeeBotOS Web 管理端新增"模型"页面，替代现有的 `config/beebotos.toml` 中 `[models]` 节的配置方式。所有 LLM 配置（包括 API key）通过 Web UI 存入数据库，不再通过环境变量或配置文件注入。

## 2. 核心设计原则

### 2.1 协议兼容（不再按 Provider 名称区分）

不再保留 KimiProvider、ZhipuProvider、DeepSeekProvider 等独立实现。所有外部 LLM 服务只按 API 协议分为两种：

- **openai-compatible**：OpenAI 兼容协议（绝大多数服务商：kimi、deepseek、zhipu、openai 等）
- **anthropic**：Anthropic 协议（claude 系列）

Gateway 启动时根据数据库中存储的 `protocol` 字段，使用统一的协议实现创建 provider。

### 2.2 预设 vs 自定义提供商

- **预设提供商**：kimi、openai、zhipu、deepseek、anthropic、ollama，自带默认 base_url 和推荐模型列表
- **自定义提供商**：用户手动填写 provider_id、名称、协议、base_url，完全灵活

### 2.3 API Key 加密存储

- 使用 **AES-256-GCM** 加密存储 API key
- Master key 从环境变量 `BEE__SECURITY__MASTER_KEY` 读取
- 前端展示时脱敏显示（仅显示前4位 + **** + 后4位）

### 2.4 级联删除

删除提供商时，自动删除其下所有关联模型（`ON DELETE CASCADE`）。

## 3. 数据库设计

### 3.1 表结构

```sql
-- 预设提供商种子数据
CREATE TABLE llm_providers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL UNIQUE,    -- 系统标识，如 "kimi"
    name TEXT NOT NULL,                  -- 显示名称，如 "Moonshot AI"
    protocol TEXT NOT NULL CHECK(protocol IN ('openai-compatible', 'anthropic')),
    base_url TEXT,                       -- API base URL
    api_key_encrypted TEXT,              -- 加密后的 API key
    enabled BOOLEAN NOT NULL DEFAULT true,
    is_default_provider BOOLEAN NOT NULL DEFAULT false,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE llm_models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id INTEGER NOT NULL,
    name TEXT NOT NULL,                  -- 模型 ID，如 "moonshot-v1-8k"
    display_name TEXT,                   -- 显示名称，如 "Moonshot V1 8K"
    is_default_model BOOLEAN NOT NULL DEFAULT false,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (provider_id) REFERENCES llm_providers(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_provider_id ON llm_providers(provider_id);
CREATE INDEX idx_models_provider ON llm_models(provider_id);
```

### 3.2 预设数据

Gateway 首次启动时自动插入预设提供商：

| provider_id | name | protocol | base_url (默认) |
|------------|------|----------|----------------|
| kimi | Moonshot AI | openai-compatible | https://api.moonshot.cn/v1 |
| openai | OpenAI | openai-compatible | https://api.openai.com/v1 |
| zhipu | 智谱 AI | openai-compatible | https://open.bigmodel.cn/api/paas/v4 |
| deepseek | DeepSeek | openai-compatible | https://api.deepseek.com/v1 |
| anthropic | Anthropic | anthropic | https://api.anthropic.com/v1 |
| ollama | Ollama (本地) | openai-compatible | http://localhost:11434 |

## 4. 后端 API 设计

### 4.1 Admin API（新增）

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/v1/admin/llm/providers` | 列出所有提供商（含模型列表） |
| POST | `/api/v1/admin/llm/providers` | 创建自定义提供商 |
| PUT | `/api/v1/admin/llm/providers/:id` | 更新提供商 |
| DELETE | `/api/v1/admin/llm/providers/:id` | 删除提供商 |
| POST | `/api/v1/admin/llm/providers/:id/models` | 添加模型 |
| DELETE | `/api/v1/admin/llm/providers/:id/models/:model_id` | 删除模型 |
| PUT | `/api/v1/admin/llm/providers/:id/default` | 设为默认提供商 |
| PUT | `/api/v1/admin/llm/providers/:id/models/:model_id/default` | 设为默认模型 |

### 4.2 保留 API（修改数据源）

| 方法 | 路径 | 修改 |
|------|------|------|
| GET | `/api/v1/llm/config` | 从数据库读取，返回脱敏配置 |
| GET | `/api/v1/llm/metrics` | 保持不变 |
| GET | `/api/v1/llm/health` | 保持不变 |

### 4.3 请求/响应格式

**POST /api/v1/admin/llm/providers**（创建自定义提供商）
```json
{
  "provider_id": "my-custom",
  "name": "My Custom Provider",
  "protocol": "openai-compatible",
  "base_url": "https://api.example.com/v1",
  "api_key": "sk-xxxxxxxx"
}
```

**GET /api/v1/admin/llm/providers**（列表响应）
```json
{
  "providers": [
    {
      "id": 1,
      "provider_id": "kimi",
      "name": "Moonshot AI",
      "protocol": "openai-compatible",
      "base_url": "https://api.moonshot.cn/v1",
      "api_key_masked": "sk-12****34",
      "enabled": true,
      "is_default_provider": true,
      "models": [
        { "id": 1, "name": "moonshot-v1-8k", "display_name": "Moonshot V1 8K", "is_default_model": true }
      ]
    }
  ]
}
```

## 5. Gateway 启动逻辑重构

### 5.1 LlmService 改造

```rust
pub struct LlmService {
    db: Arc<sqlx::SqlitePool>,
    encryption: Arc<EncryptionService>,
    failover_provider: Arc<FailoverProvider>,
    metrics: Arc<LlmMetrics>,
    multimodal_processor: MultimodalProcessor,
}

impl LlmService {
    pub async fn new(
        db: Arc<sqlx::SqlitePool>,
        encryption: Arc<EncryptionService>,
    ) -> Result<Self, GatewayError> {
        // 1. 首次启动：检查并插入缺失的预设提供商（已有则跳过）
        Self::seed_providers(&db).await?;
        
        // 2. 从数据库加载所有启用的提供商
        let providers = Self::load_providers_from_db(&db, &encryption).await?;
        
        // 3. 构建 failover provider
        let failover = Self::build_failover_provider(providers).await?;
        
        Ok(Self { db, encryption, failover_provider: Arc::new(failover), ... })
    }
    
    /// 热重载：修改提供商配置后调用
    pub async fn reload_providers(&self) -> Result<(), GatewayError> {
        let providers = Self::load_providers_from_db(&self.db, &self.encryption).await?;
        let new_failover = Self::build_failover_provider(providers).await?;
        // 原子替换 failover_provider
        // 使用 Arc::make_mut 或 RwLock<Arc<...>>
        Ok(())
    }
}
```

### 5.2 Provider 创建（按协议）

```rust
fn create_provider_from_db(
    protocol: &str,
    base_url: String,
    api_key: String,
    default_model: String,
) -> Result<Arc<dyn LLMProvider>, String> {
    match protocol {
        "openai-compatible" => {
            let config = OpenAIConfig {
                base_url,
                api_key,
                default_model,
                timeout: Duration::from_secs(90),
                retry_policy: RetryPolicy::default(),
                organization: None,
            };
            Ok(Arc::new(OpenAIProvider::new(config)?))
        }
        "anthropic" => {
            let config = AnthropicConfig {
                base_url,
                api_key,
                default_model,
                timeout: Duration::from_secs(90),
                retry_policy: RetryPolicy::default(),
                version: "2023-06-01".to_string(),
            };
            Ok(Arc::new(AnthropicProvider::new(config)?))
        }
        _ => Err(format!("Unknown protocol: {}", protocol)),
    }
}
```

## 6. 前端设计

### 6.1 页面结构

- **主页面**（`/models`）：
  - 面包屑导航
  - "默认 LLM" 选择器（下拉框选择默认提供商的默认模型）
  - 提供商卡片网格（每个卡片显示：名称、协议、base_url、模型数量、默认模型标签）
  - "添加自定义提供商" 按钮

- **配置弹窗**（点击卡片进入编辑）：
  - 显示名称、协议（只读，预设不可改）、Base URL、API Key
  - "模型管理" 按钮 → 打开模型管理弹窗
  - "设为默认" 按钮
  - "删除提供商" 按钮（需确认）

- **模型管理弹窗**：
  - 当前模型列表（可删除）
  - 添加模型输入框（模型 ID + 显示名称）
  - 搜索/过滤

- **添加自定义提供商弹窗**：
  - provider_id（系统标识，英文小写）
  - 显示名称
  - 协议选择（下拉：OpenAI 兼容 / Anthropic）
  - Base URL
  - API Key

### 6.2 交互流程

```
用户进入 /models 页面
  → GET /api/v1/admin/llm/providers 加载列表
  → 渲染提供商卡片

点击"添加自定义提供商"
  → 填写表单 → POST /api/v1/admin/llm/providers
  → 成功：刷新列表

点击卡片"配置"
  → 打开配置弹窗（PUT 更新基础信息）
  → 点击"模型管理"
    → 打开模型管理弹窗
    → 添加模型：POST /api/v1/admin/llm/providers/:id/models
    → 删除模型：DELETE ...
    → 设为默认：PUT .../default

删除提供商
  → 确认弹窗 → DELETE /api/v1/admin/llm/providers/:id
  → 级联删除所有模型
```

## 7. 代码清理清单

### 7.1 删除冗余 Provider 实现

以下文件从 `crates/agents/src/llm/providers/` 中删除（它们都是 OpenAI 兼容协议的重复 wrapper）：

- `kimi.rs` — 使用 OpenAI 兼容协议
- `deepseek.rs` — 使用 OpenAI 兼容协议
- `zhipu.rs` — 使用 OpenAI 兼容协议
- `doubao.rs` — 使用 OpenAI 兼容协议
- `qwen.rs` — 使用 OpenAI 兼容协议
- `gemini.rs` — 使用 OpenAI 兼容协议
- `claude.rs` — 与 `anthropic.rs` 重复，保留 anthropic.rs

保留的文件：
- `openai.rs` — OpenAI 兼容协议实现
- `anthropic.rs` — Anthropic 协议实现
- `ollama.rs` — 本地 Ollama（特殊本地部署逻辑）
- `mod.rs` — 清理后只保留上述导出

**注意**：删除 `claude.rs` 后，所有引用 `ClaudeConfig` / `ClaudeProvider` 的代码（如 `llm_service.rs`）需改为引用 `anthropic.rs` 中的 `AnthropicConfig` / `AnthropicProvider`。

### 7.2 删除配置文件中 LLM 相关配置

从 `config/beebotos.toml` 中删除整个 `[models]` 节及所有子节（`[models.kimi]`、`[models.zhipu]` 等）。

### 7.3 删除环境变量读取逻辑

`LlmService` 中不再从环境变量读取 API key、base_url、model。

### 7.4 修改 AppState

`apps/gateway/src/main.rs`：
- `LlmService::new(config.clone())` → `LlmService::new(db.clone(), encryption.clone()).await`
- 新增 admin API 路由注册

### 7.5 保留 Gateway 外部接口

`ProviderFactory` 和 `FailoverProvider` 在 `crates/agents` 中的定义保留，供其他模块使用。Gateway 内部不再使用 `ProviderFactory::from_env()`。

## 8. 数据流

```
[Web UI] ←→ [Admin API Handlers]
                  ↓
         [LlmService: reload_providers()]
                  ↓
         [Database: llm_providers + llm_models]
                  ↓
         [EncryptionService: decrypt api_key]
                  ↓
         [Protocol-based Provider Creation]
                  ↓
         [OpenAIProvider / ClaudeProvider]
                  ↓
         [FailoverProvider]
                  ↓
         [LLM API Call]
```

## 9. 错误处理

| 场景 | 行为 |
|------|------|
| 未配置任何提供商 | Gateway 启动报错，提示用户通过 Web UI 配置 |
| API key 解密失败 | 记录错误日志，跳过该 provider，继续加载其他 |
| 默认提供商被删除 | 自动将第一个启用的提供商设为默认 |
| 热重载失败 | 保留旧的 failover_provider，记录错误 |
| 删除预设提供商 | 允许删除，但重启后会重新 seed（自定义的不受影响） |

## 10. 安全考量

- API key 使用 AES-256-GCM 加密，master key 不存入数据库
- 所有 admin API 需要认证（复用现有 auth middleware）
- 响应中 API key 始终脱敏
- 删除操作需二次确认（前端确认弹窗）

## 11. 范围边界

**本次实现包含：**
- 数据库 migration 和预设数据 seed
- 后端 Admin API（增删改查提供商和模型）
- 前端"模型"页面（列表、配置弹窗、模型管理弹窗、添加自定义提供商弹窗）
- Gateway 启动逻辑重构（从数据库加载 provider）
- 冗余 provider 代码清理
- 配置文件 LLM 配置移除

**本次不包含：**
- 测试连接功能（验证 API key 有效性）
- 自动发现模型列表
- 模型参数配置（temperature、max_tokens 等）
- 使用统计和用量报表
- 多租户隔离
