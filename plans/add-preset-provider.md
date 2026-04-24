# 添加预置大模型提供商 - 实现方案规划

## 现状分析

当前实现：
- 后端：`PRESET_PROVIDERS` 常量硬编码在 `llm_provider_db.rs` 中
- 启动时 `seed_providers()` 检查 provider_id 是否存在，不存在则插入
- 前端：`provider_icon_info()` 和 `provider_type_label()` 硬编码匹配 provider_id
- **问题**：添加新预置提供商需要修改前后端代码 + 重新编译

数据库现状：
```
llm_providers 表：id, provider_id, name, protocol, base_url, api_key_encrypted, enabled, is_default_provider, created_at, updated_at
```

## 方案对比

### 方案A：SQL种子脚本（最轻量）

**思路**：不修改后端代码，直接通过 SQL 脚本插入预置数据，前端做最小修改。

**实现**：
1. 创建 `scripts/seed_providers.sql`：
   ```sql
   INSERT OR IGNORE INTO llm_providers (provider_id, name, protocol, base_url, enabled)
   VALUES ('kimi-china', 'Kimi (China)', 'openai-compatible', 'https://api.moonshot.cn/v1', 1);
   ```
2. 前端 `provider_icon_info` / `provider_type_label` 添加 `kimi-china` 映射
3. 执行 SQL 脚本 → 重新编译前端 WASM → 刷新页面

**优点**：
- 后端零修改、零编译
- 数据库已有数据，即使删除代码中的 PRESET_PROVIDERS，数据也不丢失

**缺点**：
- 前端仍需硬编码图标/标签
- 每新增一个提供商都要修改前端并重新编译 WASM

---

### 方案B：后端API扩展图标/标签（推荐）

**思路**：将图标和标签信息存储在数据库中，通过API返回给前端，前端彻底解耦。

**实现**：
1. 数据库 `llm_providers` 表新增字段：`icon`, `icon_color`, `type_label`
2. 修改 `LlmProviderDb` 和 API 响应类型，返回这些字段
3. 前端移除 `provider_icon_info` / `provider_type_label` 函数，直接从 API 数据渲染
4. `seed_providers` 扩展为同时写入 icon/icon_color/type_label
5. 对已存在的数据库，提供迁移脚本填充新字段

**优点**：
- 添加新预置提供商只需执行 SQL INSERT（或在 PRESET_PROVIDERS 中添加一行）
- 前端无需修改、无需重新编译
- 最符合"在数据库添加默认数据即可"

**缺点**：
- 需要一次性的 schema 变更和前后端改造
- 改动范围较大（涉及 DB、API、前端渲染）

---

### 方案C：配置文件驱动

**思路**：将预置提供商列表从代码迁移到 `config/presets.toml`，启动时读取并同步到数据库。

**实现**：
1. 创建 `config/llm_providers.toml`：
   ```toml
   [[provider]]
   provider_id = "kimi-china"
   name = "Kimi (China)"
   protocol = "openai-compatible"
   base_url = "https://api.moonshot.cn/v1"
   icon = "🌙"
   icon_color = "#4f6ef7"
   type_label = "代理"
   ```
2. `seed_providers` 改为读取该配置文件
3. 前端同方案B，从API获取图标/标签

**优点**：
- 添加新提供商只需改配置文件，零代码修改
- 最灵活

**缺点**：
- 需要实现 TOML 解析 + 文件监控/热重载逻辑
- 改动最大

---

## 推荐方案：B（后端API扩展）

理由：
- 一次性改造后，后续添加预置提供商只需一条 SQL 或改一行配置
- 最符合用户"在数据库添加默认数据即可"的诉求
- 改动虽然较大，但结构清晰，未来维护成本低
