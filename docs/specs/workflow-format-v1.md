# BeeBotOS Workflow Format Specification v1.1

> **版本**: v1.1  
> **状态**: 正式版本  
> **日期**: 2026-05-04  
> **关联文档**: [Skill Composition & Workflow Design](../agents/skill-composition-workflow-design.md)
>
> **v1.1 变更**: 新增 OpenClaw 格式兼容、JSON 支持、`{{now}}` / `{{env.*}}` 模板变量、`on_error` / `error_handler` 异常处理。

---

## 1. 概述

BeeBotOS Workflow 是一种**声明式配置格式**（YAML 或 JSON），用于定义可重复执行的标准业务流程。工作流通过 `WorkflowEngine` 解析执行，支持 DAG 依赖管理、模板变量替换、条件分支、超时重试、异常处理等高级特性。

**格式兼容**：v1.1 同时兼容 **BeeBotOS 原生格式** 和 **OpenClaw 格式**，同一文件可同时使用两种风格的字段。

工作流文件存放于以下目录：
- `data/workflows/` — 系统级工作流（内置）
- `examples/workflows/` — 示例工作流
- 用户自定义目录 — 可通过 Gateway API 动态注册

---

## 2. 文件格式

### 2.1 支持的格式

| 格式 | 扩展名 | 说明 |
|------|--------|------|
| YAML | `.yaml`, `.yml` | 推荐，人类可读 |
| JSON | `.json` | 适合程序化生成 |

> **注意**: YAML 是 JSON 的超集，因此 `serde_yaml` 也能解析纯 JSON 内容。

### 2.2 编码规范

- 文件编码：UTF-8
- YAML 缩进：2 个空格（禁止 Tab）
- 最大文件大小：建议不超过 1MB

---

## 3. 顶层字段详解

| 字段 | 类型 | 必填 | 默认值 | 说明 | OpenClaw 别名 |
|------|------|------|--------|------|--------------|
| `id` | string | 是¹ | — | 工作流唯一标识 | `name` |
| `name` | string | 是¹ | — | 工作流显示名称 | `id` |
| `description` | string | 是 | — | 工作流功能描述 | — |
| `version` | string | 否 | "1.0.0" | 语义化版本号 | — |
| `author` | string | 否 | null | 作者信息 | — |
| `tags` | string[] | 否 | [] | 分类标签 | — |
| `triggers` | Trigger[] | 否 | [] | 触发器定义 | — |
| `config` | Config | 否 | {} | 全局执行配置 | — |
| `steps` | Step[] | 是 | — | 执行步骤列表 | — |
| `error_handler` | ErrorHandler | 否 | null | 全局异常处理（OpenClaw） | — |

> ¹ `id` 和 `name` 至少填一个。若 `id` 为空，自动从 `name` 或文件名填充；若 `name` 为空，自动从 `id` 填充。

---

## 4. Triggers（触发器）

触发器定义工作流的启动条件。一个工作流可包含多个触发器。

### 4.1 触发器类型总览

```yaml
triggers:
  - type: cron
    schedule: "0 9 * * *"
    timezone: "Asia/Shanghai"

  - type: event
    source: "price_feed"        # OpenClaw alias: channel
    filter: "btc_usd_change > 0.02"  # OpenClaw alias: event_type

  - type: webhook
    path: "/webhook/daily-news"
    method: "POST"
    auth: "bearer_token"

  - type: manual
    allowed_users: ["admin"]    # OpenClaw 扩展
```

### 4.2 Cron Trigger

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `type` | string | 是 | 固定值 `"cron"` |
| `schedule` | string | 是 | Cron 表达式（标准 5 字段格式） |
| `timezone` | string | 否 | 时区，默认 `"UTC"` |

**Cron 表达式格式**：`分 时 日 月 周`

```yaml
# 每天上午 9 点
schedule: "0 9 * * *"

# 每 5 分钟
schedule: "*/5 * * * *"

# 每周一上午 8 点
schedule: "0 8 * * 1"
```

### 4.3 Event Trigger

| 字段 | 类型 | 必填 | 说明 | OpenClaw 别名 |
|------|------|------|------|--------------|
| `type` | string | 是 | 固定值 `"event"` | — |
| `source` | string | 是 | 事件源标识符 | `channel` |
| `filter` | string | 否 | 过滤表达式 | `event_type` |

### 4.4 Webhook Trigger

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `type` | string | 是 | 固定值 `"webhook"` |
| `path` | string | 是 | URL 路径（如 `/webhook/my-workflow`） |
| `method` | string | 否 | HTTP 方法，默认 `"POST"` |
| `auth` | string | 否 | 认证类型（如 `"bearer_token"`） |

### 4.5 Manual Trigger

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `type` | string | 是 | 固定值 `"manual"` |
| `allowed_users` | string[] | 否 | 仅限指定用户触发（OpenClaw 扩展） |

支持通过以下方式手动触发：
- Gateway REST API: `POST /api/v2/workflows/{id}/execute`
- 聊天命令: `/workflow {workflow_id}`
- 自然语言匹配（需包含 `manual` 触发器）

---

## 5. Global Config（全局配置）

```yaml
config:
  timeout_sec: 300              # 整个工作流超时（秒），默认 300
  max_retries: 2                # 默认步骤重试次数，默认 0
  continue_on_failure: false    # 某步失败时是否继续执行后续步骤，默认 false
  notify_on_complete: true      # 完成时是否生成通知，默认 false
```

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `timeout_sec` | integer | 否 | 300 | 工作流整体超时 |
| `max_retries` | integer | 否 | 0 | 步骤默认重试次数 |
| `continue_on_failure` | boolean | 否 | false | 失败时是否继续 |
| `notify_on_complete` | boolean | 否 | false | 完成时通知 |

---

## 6. Steps（执行步骤）

步骤是工作流的基本执行单元，每个步骤调用一个已注册的 Skill。

### 6.1 步骤结构

```yaml
steps:
  - id: "step_id"               # 步骤唯一标识（必填，workflow 内唯一）
    name: "Step Name"           # 显示名称（BeeBotOS 必填，OpenClaw 可选）
    skill: "skill_id"           # 调用的 Skill ID（必填）
    params:                     # Skill 参数（支持模板变量）
      key: "value"
      input: "{{steps.prev_step.output}}"
    depends_on:                 # 依赖步骤列表（可选，支持 DAG）
      - "prev_step_id"
    condition: "true"           # 条件表达式（可选）
    timeout_sec: 30             # 步骤超时（可选，覆盖全局配置）
    retries: 2                  # 步骤重试（可选，覆盖全局配置）
    on_error:                   # 步骤级异常处理（OpenClaw）
      action: "retry"
      max_retries: 3
      delay_seconds: 10
      fallback:
        skill: "slack-notify"
        params:
          text: "步骤失败: {{error.message}}"
```

### 6.2 字段详解

| 字段 | 类型 | 必填 | 默认值 | 说明 | OpenClaw 别名 |
|------|------|------|--------|------|--------------|
| `id` | string | 是 | — | 步骤唯一标识 | — |
| `name` | string | 否² | — | 步骤显示名称 | — |
| `skill` | string | 是 | — | SkillRegistry 中注册的 skill_id | — |
| `params` | object/string | 否 | null | 参数对象 | `input` |
| `depends_on` | string[] | 否 | null | 依赖步骤 ID 列表 | `dependencies` |
| `condition` | string | 否 | null | 条件表达式 | — |
| `timeout_sec` | integer | 否 | 全局值 | 步骤超时（秒） | — |
| `retries` | integer | 否 | 全局值 | 步骤重试次数 | — |
| `on_error` | OnError | 否 | null | 步骤级异常处理 | — |

> ² OpenClaw 格式中 `name` 可省略，自动使用 `id` 的值。

### 6.3 依赖与 DAG

工作流支持有向无环图（DAG）依赖：

```yaml
steps:
  - id: fetch_a
    skill: http_request
  - id: fetch_b
    skill: http_request
  - id: merge
    skill: data_merger
    depends_on: ["fetch_a", "fetch_b"]   # 并行获取后合并
```

**约束**：
- 不支持循环依赖（WorkflowEngine 会检测并返回错误）
- 无依赖的步骤默认并行执行（当通过 `DagScheduler` 执行时）
- 直接执行模式下按拓扑排序顺序串行执行

---

## 7. 异常处理（OpenClaw 兼容）

### 7.1 步骤级异常处理 (`on_error`)

当步骤执行失败时，根据 `on_error` 配置决定后续行为：

```yaml
steps:
  - id: send_email
    skill: email-notify
    params:
      to: ["team@example.com"]
    on_error:
      action: "retry"           # retry | skip | fail
      max_retries: 3            # 覆盖 steps.retries
      delay_seconds: 10         # 重试间隔（秒）
      fallback:
        skill: "slack-notify"
        params:
          channel: "#alerts"
          text: "邮件发送失败"
```

**`action` 取值**：

| 值 | 行为 | 说明 |
|-----|------|------|
| `retry` | 重试执行 | 使用 `on_error.max_retries` 覆盖 `retries`，支持 `delay_seconds` |
| `skip` | 跳过步骤 | 该步骤标记为 skipped，后续依赖步骤继续执行 |
| `fail` | 终止流程 | 默认行为，根据 `config.continue_on_failure` 决定是否继续 |

**`fallback`**: 当所有重试耗尽后，执行指定的 fallback skill。fallback 执行成功则该步骤标记为 completed。

### 7.2 全局异常处理 (`error_handler`)

当工作流中任意步骤失败时触发：

```yaml
error_handler:
  step: "any"                   # "any" 捕获所有步骤，或指定 step_id
  action: "fail"                # 当前仅支持记录，fallback 必执行
  fallback:
    skill: "slack-notify"
    params:
      channel: "#ops"
      text: "工作流 {{workflow.id}} 失败: {{workflow.error_log}}"
```

**`step` 取值**：
- `"any"` — 捕获所有步骤的异常（默认）
- `"specific_step_id"` — 仅捕获指定步骤的异常

---

## 8. 模板语法

步骤参数支持模板变量替换，用于步骤间数据传递。

### 8.1 语法总览

| 语法 | 说明 | 来源 |
|------|------|------|
| `{{steps.<id>.output}}` | 引用上游步骤输出 | BeeBotOS |
| `{{steps.<id>.output.<path>}}` | 引用 JSON 字段 | BeeBotOS |
| `{{steps.<id>.status}}` | 引用步骤状态 | BeeBotOS |
| `{{workflow.any_failed}}` | 是否有步骤失败 | BeeBotOS |
| `{{workflow.error_log}}` | 错误日志汇总 | BeeBotOS |
| `{{workflow.duration}}` | 工作流耗时（秒） | BeeBotOS |
| `{{input.<path>}}` | 触发上下文输入 | BeeBotOS |
| `{{now}}` | 当前时间戳（ISO 8601） | OpenClaw |
| `{{env.VAR_NAME}}` | 读取环境变量 | OpenClaw |
| `${ENV_VAR}` | 环境变量注入（ legacy ） | BeeBotOS |

### 8.2 JSON Path 访问

默认使用**点号分割**：
```yaml
params:
  title: "{{steps.fetch_news.output.articles.0.title}}"
```

如果 JSON 字段名包含点号（如 `"foo.bar"`），使用 **JSON Pointer**（RFC 6901）语法：
```yaml
params:
  value: "{{steps.fetch_news.output./foo.bar/baz}}"
```

JSON Pointer 特殊字符转义：
- `~1` → `/`
- `~0` → `~`

### 8.3 条件表达式

`condition` 字段支持以下表达式：

```yaml
# 布尔值
condition: "true"

# 数值比较
condition: "{{steps.summarize.output.word_count}} < 200"

# 字符串相等
condition: "{{steps.check.output.status}} == ok"

# 字符串不等
condition: "{{steps.check.output.status}} != error"
```

支持的运算符：`<`, `>`, `==`, `!=`, `<=`, `>=`

### 8.4 环境变量

**OpenClaw 风格**（推荐）：
```yaml
params:
  webhook_url: "{{env.FEISHU_WEBHOOK_URL}}"
```

**BeeBotOS legacy 风格**（仍然支持）：
```yaml
params:
  webhook_url: "${FEISHU_WEBHOOK_URL}"
```

环境变量在运行时从进程环境读取，缺失时会报错。

---

## 9. 完整示例

### 9.1 每日科技早报（BeeBotOS 风格 YAML）

```yaml
id: daily_tech_news
name: "Daily Tech News Briefing"
description: "每天早上 9 点抓取科技新闻，生成摘要并推送到飞书"
version: "1.0.0"
author: "BeeBotOS Team"
tags: ["daily", "news", "feishu"]

triggers:
  - type: cron
    schedule: "0 9 * * *"
    timezone: "Asia/Shanghai"
  - type: manual

config:
  timeout_sec: 300
  continue_on_failure: false
  notify_on_complete: true

steps:
  - id: fetch_news
    name: "Fetch Tech News"
    skill: rss_reader
    params:
      url: "https://news.ycombinator.com/rss"
      limit: 10
    timeout_sec: 30
    retries: 2

  - id: summarize
    name: "Summarize with AI"
    skill: llm_summarizer
    depends_on: ["fetch_news"]
    params:
      input: "{{steps.fetch_news.output}}"
      style: "bullet_points"
    timeout_sec: 60

  - id: push_feishu
    name: "Push to Feishu"
    skill: feishu_bot
    depends_on: ["summarize"]
    params:
      webhook_url: "${FEISHU_WEBHOOK_URL}"
      message: "{{steps.summarize.output}}"
    timeout_sec: 15
    retries: 3
```

### 9.2 OpenClaw 风格 JSON（100% 兼容）

```json
{
  "name": "daily-industry-report",
  "description": "每日行业动态报告生成",
  "version": "1.0.0",
  "triggers": [
    {
      "type": "cron",
      "schedule": "0 9 * * *",
      "timezone": "Asia/Shanghai"
    }
  ],
  "config": {
    "timeout_sec": 600,
    "notify_on_complete": true
  },
  "steps": [
    {
      "id": "fetch_news",
      "skill": "data_analyst",
      "input": {
        "query": "AI industry trends 2024",
        "max_results": 10
      },
      "timeout_sec": 120,
      "retries": 2
    },
    {
      "id": "generate_summary",
      "skill": "python_developer",
      "dependencies": ["fetch_news"],
      "input": {
        "task": "generate_report",
        "date": "{{now}}"
      }
    },
    {
      "id": "send_email",
      "skill": "email_writer",
      "dependencies": ["generate_summary"],
      "input": {
        "to": ["team@example.com"],
        "subject": "【自动报告】{{now}}",
        "content": "{{steps.generate_summary.output}}"
      },
      "on_error": {
        "action": "retry",
        "max_retries": 3,
        "delay_seconds": 10,
        "fallback": {
          "skill": "slack-notify",
          "input": {
            "text": "报告生成失败"
          }
        }
      }
    }
  ],
  "error_handler": {
    "step": "any",
    "action": "fail",
    "fallback": {
      "skill": "slack-notify",
      "input": {
        "channel": "#alerts",
        "text": "工作流失败: {{workflow.error_log}}"
      }
    }
  }
}
```

### 9.3 混合风格（同一文件同时使用两种风格）

```yaml
# 顶层使用 OpenClaw 的 name 作为 id
name: "hybrid-workflow"
description: "展示混合风格兼容性"

# triggers 使用 BeeBotOS 风格
triggers:
  - type: manual

# steps 使用 OpenClaw 的 input / dependencies
steps:
  - id: step1
    skill: data_analyst
    input:                      # OpenClaw 风格
      query: "test"

  - id: step2
    skill: email_writer
    dependencies: [step1]       # OpenClaw 风格
    params:                     # BeeBotOS 风格（与 input 等价）
      to: ["admin@example.com"]
    on_error:                   # OpenClaw 风格
      action: skip
```

---

## 10. 版本历史

| 版本 | 日期 | 变更 |
|------|------|------|
| v1.0 | 2026-04-30 | 初始版本。支持 Cron/Event/Webhook/Manual 触发器、DAG 依赖、模板变量、条件表达式、JSON Pointer、环境变量注入。 |
| v1.1 | 2026-05-04 | **OpenClaw 兼容**: 支持 `name` 作为 `id` 别名、`input` 作为 `params` 别名、`dependencies` 作为 `depends_on` 别名、`channel`/`event_type` 作为 Event trigger 别名。新增 JSON 格式支持、`{{now}}` / `{{env.*}}` 模板变量、`on_error` / `error_handler` 异常处理。 |

---

## 11. 参考

- [YAML 1.2 Spec](https://yaml.org/spec/1.2.2/)
- [JSON Pointer (RFC 6901)](https://datatracker.ietf.org/doc/html/rfc6901)
- [Cron Expression Format](https://en.wikipedia.org/wiki/Cron)

