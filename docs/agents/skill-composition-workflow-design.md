# BeeBotOS Skill Composition & Workflow Orchestration 设计方案

> **版本**: v1.0  
> **状态**: 设计草案（待评审）  
> **日期**: 2026-04-30  
> **关联文档**: [Skill Execution Redesign](./skill-execution-redesign.md), [Agent Runtime](../architecture/04-agent-runtime.md), [DAG Scheduler](../../crates/agents/src/queue/dag_scheduler.rs)

---

## 1. 背景与动机

BeeBotOS 当前已具备完善的 **单体 Skill 执行能力**（WASM / Knowledge / Code-driven），但在面对复杂业务场景时，存在以下瓶颈：

| 痛点 | 现状 | 期望 |
|------|------|------|
| **Skill 孤岛** | 每个 Skill 独立执行，无法直接调用另一个 Skill | Skill 之间可以像"乐高积木"一样组合 |
| **执行顺序失控** | LLM 动态决策，同一句指令每次执行路径可能不同 | 关键业务流程需要**固定、可审计**的执行顺序 |
| **重复劳动** | 用户每天手动触发相同的 Skill 链（如"查新闻→写摘要→推送到飞书"） | 支持**定时/事件触发**的自动化流水线 |
| **多 Agent 协作** | 单 Agent 串行处理，复杂任务耗时过长 | 支持**子 Agent 并行**处理独立子任务 |

本方案参考 **OpenClaw** 的 Skill Composition 与 Workflow Orchestration 理念，在 BeeBotOS 现有架构（`DagScheduler`、`PlanningEngine`、`SkillRegistry`、`Agent`）基础上，设计一套**声明式工作流编排 + 程序化 Skill 组合**机制，使 BeeBotOS 从"个人助手"升级为"企业级数字员工"。

---

## 2. 设计目标

| 目标 | 说明 |
|------|------|
| **声明式配置** | 工作流通过 YAML/JSON 定义，支持热加载，无需重新编译 |
| **复用现有底座** | 基于已有的 `DagScheduler`、`PlanningEngine`、`SkillRegistry` 构建，不重复造轮子 |
| **双轨并行** | **Skill Composition**（运行时动态组合）与 **Workflow Orchestration**（预定义流水线）共存 |
| **数据传递标准化** | 步骤间通过结构化 Schema 传递数据，支持 `{{steps.<id>.output}}` 模板语法 |
| **多 Agent 协作** | 主 Agent 可调度多个子 Agent 并行执行任务，最后汇总结果 |
| **可观测性** | 工作流执行状态持久化，支持可视化监控、失败重试、断点续跑 |

---

## 3. 核心概念："术"与"法"的双轨模型

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        BeeBotOS 自动化双轨模型                               │
├───────────────────────────────┬─────────────────────────────────────────────┤
│      🧱 Skill Composition     │         ⚙️ Workflow Orchestration           │
│         （术 · 战术层）         │            （法 · 战略层）                    │
├───────────────────────────────┼─────────────────────────────────────────────┤
│ 运行时动态决策                 │ 预定义、可重复执行的标准流程                   │
│ LLM 根据上下文自主组合 Skill   │ 声明式 YAML 配置，固定步骤与分支               │
│ 适合：探索性、对话式任务         │ 适合：定时报表、审批流、CI/CD 等确定性流程       │
│ 例子："对比三家公司的财报"      │ 例子："每天早上 9 点自动生成行业简报并推送"      │
├───────────────────────────────┼─────────────────────────────────────────────┤
│ 实现方式：                     │ 实现方式：                                   │
│ • Skill Pipeline（顺序链）      │ • DagScheduler + WorkflowInstance            │
│ • Skill Parallel（并行组）      │ • YAML 工作流定义（Trigger + Steps）           │
│ • Skill Conditional（条件分支） │ • Cron / Event / Webhook 触发器              │
│ • Skill Loop（循环/重试）       │ • 持久化状态 + 可视化监控                      │
└───────────────────────────────┴─────────────────────────────────────────────┘
```

---

## 4. Skill Composition（技能组合）

Skill Composition 是**运行时动态组合**机制，允许 Agent 在执行过程中，根据任务需要串联、并行或条件调用多个 Skill。

### 4.1 组合模式总览

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Skill Composition 模式                               │
├──────────────────┬──────────────────┬──────────────────┬────────────────────┤
│   🔗 Pipeline    │   ⚡ Parallel    │   🔀 Conditional │   🔄 Loop/Retry    │
│    顺序链式       │     并行组       │     条件分支      │     循环/重试       │
├──────────────────┼──────────────────┼──────────────────┼────────────────────┤
│ Skill A →        │ Skill A          │ if condition     │ retry skill        │
│ Skill B →        │ Skill B          │   then Skill X   │   until success    │
│ Skill C          │ Skill C          │ else Skill Y     │   (max: 3)         │
│ (输出作为下一步输入)│ (结果汇总)        │                  │                    │
└──────────────────┴──────────────────┴──────────────────┴────────────────────┘
```

### 4.2 模式一：Skill Pipeline（顺序链式）

上一个 Skill 的输出作为下一个 Skill 的输入，形成处理链。

**触发方式**：Agent 的 `handle_llm_task` 检测到用户请求包含多步骤意图时，自动构建 Pipeline。

**代码示例**：
```rust
// crates/agents/src/skills/composition/pipeline.rs（新增）
pub struct SkillPipeline {
    steps: Vec<PipelineStep>,
}

pub struct PipelineStep {
    pub skill_id: String,
    pub input_mapping: InputMapping,  // 如何从上游输出提取本步输入
    pub output_schema: Option<serde_json::Value>, // 期望的输出结构
}

impl SkillPipeline {
    pub async fn execute(&self, initial_input: &str, agent: &Agent) -> Result<String, AgentError> {
        let mut current_output = initial_input.to_string();
        
        for (idx, step) in self.steps.iter().enumerate() {
            let skill = agent.skill_registry
                .as_ref()
                .and_then(|r| r.get(&step.skill_id).await)
                .ok_or_else(|| AgentError::Execution(format!("Skill {} not found", step.skill_id)))?;
            
            // 根据 input_mapping 构造输入
            let step_input = step.input_mapping.apply(&current_output)?;
            
            let result = agent.execute_registered_skill(&skill, &step_input, None).await?;
            current_output = result.output;
            
            info!("Pipeline step {}/{} completed: skill={}", idx + 1, self.steps.len(), step.skill_id);
        }
        
        Ok(current_output)
    }
}
```

**典型场景**：
```
用户："生成今日 AI 行业热点报告"
Pipeline:
  1. daily_news (搜索热点) → 返回新闻列表 JSON
  2. llm_summarizer (提炼摘要) → 输入: {{step1.output.articles}}
  3. report_generator (生成报告) → 输入: {{step2.output.summaries}}
```

### 4.3 模式二：Skill Parallel（并行组）

当任务可拆分为多个独立子任务时，并行执行多个 Skill，最后汇总结果。

**代码示例**：
```rust
// crates/agents/src/skills/composition/parallel.rs（新增）
pub struct SkillParallel {
    branches: Vec<ParallelBranch>,
    pub merge_strategy: MergeStrategy,
}

pub enum MergeStrategy {
    Concat,           // 字符串拼接
    JsonMerge,        // JSON 数组合并
    LlmSummarize,     // 调用 LLM 汇总
    Custom(String),   // 指定汇总 Skill ID
}

impl SkillParallel {
    pub async fn execute(&self, input: &str, agent: &Agent) -> Result<String, AgentError> {
        let futures: Vec<_> = self.branches.iter().map(|branch| {
            let skill = agent.skill_registry.as_ref().unwrap().get(&branch.skill_id);
            async move {
                let skill = skill.await?;
                let result = agent.execute_registered_skill(&skill, input, None).await?;
                Ok::<_, AgentError>((branch.id.clone(), result.output))
            }
        }).collect();
        
        let results = futures::future::join_all(futures).await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        
        self.merge_strategy.merge(results)
    }
}
```

**典型场景**：
```
用户："对比阿里巴巴、腾讯、字节跳动三家公司的 Q3 财报"
Parallel:
  Branch A: financial_analyzer (阿里巴巴 Q3 财报)
  Branch B: financial_analyzer (腾讯 Q3 财报)
  Branch C: financial_analyzer (字节跳动 Q3 财报)
Merge: LlmSummarize → "以下是三家公司的 Q3 财报对比..."
```

### 4.4 模式三：Skill Conditional（条件分支）

根据上一步的结果状态或内容，决定下一步执行路径。

**代码示例**：
```rust
// crates/agents/src/skills/composition/conditional.rs（新增）
pub struct SkillConditional {
    pub condition: Condition,
    pub then_branch: Box<CompositionNode>,
    pub else_branch: Option<Box<CompositionNode>>,
}

pub enum Condition {
    ExitCodeEq(i32),              // 子进程退出码判断
    OutputContains(String),       // 输出包含某字符串
    JsonPathEq { path: String, value: String }, // JSON 路径匹配
    LlmJudge { prompt: String },  // 让 LLM 判断布尔值
}
```

**典型场景**：
```
Workflow:
  Step 1: code_linter (代码检查)
  Step 2:
    condition: "{{step1.output.exit_code}} == 0"
    then: code_deploy (部署)
    else: notify_developer (通知开发者修复)
```

### 4.5 模式四：Skill Loop（循环/重试）

重复执行某个 Skill 直到满足条件或达到最大尝试次数。

**代码示例**：
```rust
// crates/agents/src/skills/composition/loop.rs（新增）
pub struct SkillLoop {
    pub body: Box<CompositionNode>,
    pub until: LoopCondition,
    pub max_iterations: usize,
    pub backoff_ms: u64, // 指数退避
}

pub enum LoopCondition {
    ExitCodeEq(i32),
    OutputContains(String),
    LlmJudge { prompt: String },
}
```

**典型场景**：
```
Loop:
  body: web_scraper (抓取分页数据)
  until: "{{output.has_more}} == false"
  max: 20
  backoff: 1000ms
```

### 4.6 新增：SkillCallTool（Skill 间调用工具）

为了让单个 Skill 在执行过程中也能调用其他 Skill，在 `tool_set.rs` 中新增 `SkillCallTool`：

```rust
// crates/agents/src/skills/tool_set.rs（扩展）
pub struct SkillCallTool {
    skill_registry: Arc<SkillRegistry>,
    agent_ref: Weak<Agent>, // 用于调用 execute_registered_skill
}

#[async_trait::async_trait]
impl SkillTool for SkillCallTool {
    fn name(&self) -> &str { "skill_call" }
    
    fn description(&self) -> &str {
        "调用另一个已注册的 Skill。Parameters: skill_id (string), input (string), params (object, optional)"
    }
    
    async fn execute(&self, params: &Value) -> Result<String, String> {
        let skill_id = params["skill_id"].as_str().ok_or("Missing skill_id")?;
        let input = params["input"].as_str().unwrap_or("");
        
        let skill = self.skill_registry.get(skill_id).await
            .ok_or_else(|| format!("Skill '{}' not found", skill_id))?;
        
        if let Some(agent) = self.agent_ref.upgrade() {
            let result = agent.execute_registered_skill(&skill, input, None).await
                .map_err(|e| e.to_string())?;
            Ok(result.output)
        } else {
            Err("Agent no longer available".to_string())
        }
    }
}
```

---

## 5. Workflow Orchestration（工作流编排）

Workflow Orchestration 是**预定义、可重复执行**的标准流程，通过声明式 YAML 文件描述，由 `DagScheduler` 驱动执行。

### 5.1 架构总览

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      Workflow Orchestration 架构                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   User / Cron / Webhook / Event                                             │
│        │                                                                    │
│        ▼                                                                    │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                    │
│   │   Trigger   │───▶│   Workflow  │───▶│ DagScheduler│                    │
│   │   Engine    │    │   Registry  │    │   Engine    │                    │
│   └─────────────┘    └─────────────┘    └──────┬──────┘                    │
│                                                 │                           │
│        ┌────────────────────────────────────────┘                           │
│        ▼                                                                    │
│   ┌─────────────────────────────────────────────────────────┐              │
│   │              WorkflowInstance (Runtime State)            │              │
│   │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐    │              │
│   │  │ Step 1  │─▶│ Step 2  │─▶│ Step 3  │─▶│ Step 4  │    │              │
│   │  │(skill A)│  │(skill B)│  │(skill C)│  │(skill D)│    │              │
│   │  └─────────┘  └─────────┘  └─────────┘  └─────────┘    │              │
│   │       │            │            │            │          │              │
│   │       └────────────┴────────────┴────────────┘          │              │
│   │                    │ 结果聚合 / 通知                      │              │
│   └─────────────────────────────────────────────────────────┘              │
│                              │                                              │
│                              ▼                                              │
│                        ┌─────────────┐                                      │
│                        │  StateStore │  (持久化: SQLite/PostgreSQL)         │
│                        │   (CQRS)    │                                      │
│                        └─────────────┘                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 YAML 工作流定义规范

工作流文件存放于 `workflows/` 目录，支持热加载。

```yaml
# workflows/daily_tech_news.yaml
name: "daily_tech_news"
description: "每天早上 9 点抓取科技新闻，生成摘要并推送到飞书"
version: "1.0.0"
author: "BeeBotOS Team"
tags: ["daily", "news", "feishu"]

# ───────────────────────────────────────────
# 触发器 (Trigger)
# ───────────────────────────────────────────
triggers:
  - type: cron
    schedule: "0 9 * * *"           # 每天上午 9 点
    timezone: "Asia/Shanghai"
    
  - type: event
    source: "user_command"
    filter: "message.content == '立即生成早报'"
    
  - type: webhook
    path: "/webhook/workflows/daily_tech_news"
    method: "POST"
    auth: "bearer_token"

# ───────────────────────────────────────────
# 全局配置
# ───────────────────────────────────────────
config:
  timeout_sec: 300                  # 整个工作流超时 5 分钟
  max_retries: 2
  continue_on_failure: false        # 某步失败时是否继续
  notify_on_complete: true          # 完成时通知管理员
  
# ───────────────────────────────────────────
# 执行步骤 (Steps)
# ───────────────────────────────────────────
steps:
  # Step 1: 抓取 RSS 新闻源
  - id: fetch_news
    name: "Fetch Tech News"
    skill: rss_reader               # 调用 SkillRegistry 中的 skill
    params:
      url: "https://techcrunch.com/feed/"
      limit: 10
    timeout_sec: 30
    retries: 2
    
  # Step 2: 并行抓取多个源（与 Step 1 无依赖，但示例中depends_on step1）
  - id: fetch_hacker_news
    name: "Fetch Hacker News"
    skill: rss_reader
    depends_on: ["fetch_news"]      # 显式依赖声明
    params:
      url: "https://news.ycombinator.com/rss"
      limit: 10
    timeout_sec: 30
    
  # Step 3: AI 提炼摘要（依赖 Step 1 和 Step 2）
  - id: summarize
    name: "Summarize with AI"
    skill: llm_summarizer
    depends_on: ["fetch_news", "fetch_hacker_news"]
    params:
      input: |
        TechCrunch:
        {{steps.fetch_news.output.articles}}
        
        HackerNews:
        {{steps.fetch_hacker_news.output.articles}}
      prompt_template: "tech_news_prompt.md"
      max_tokens: 2000
    timeout_sec: 60
    
  # Step 4: 条件分支：如果摘要太短，补充搜索
  - id: supplement_search
    name: "Supplement Search"
    skill: web_search
    condition: "{{steps.summarize.output.word_count}} < 200"
    depends_on: ["summarize"]
    params:
      query: "今日 AI 行业重大新闻"
      limit: 5
    timeout_sec: 30
    
  # Step 5: 生成 Markdown 报告
  - id: generate_report
    name: "Generate Report"
    skill: report_generator
    depends_on: ["summarize", "supplement_search"]
    params:
      title: "📰 科技早报"
      content: "{{steps.summarize.output.summary}}"
      supplement: "{{steps.supplement_search.output.results}}"
      format: "markdown"
    timeout_sec: 30
    
  # Step 6: 推送到飞书
  - id: push_feishu
    name: "Push to Feishu"
    skill: feishu_bot
    depends_on: ["generate_report"]
    params:
      webhook_url: "${FEISHU_WEBHOOK_URL}"
      message: |
        {{steps.generate_report.output.content}}
      format: "markdown"
    timeout_sec: 15
    retries: 3
    
  # Step 7: 错误处理（fallback）
  - id: notify_admin
    name: "Notify Admin on Failure"
    skill: email_sender
    condition: "{{workflow.any_failed}} == true"
    params:
      to: "admin@beebotos.io"
      subject: "[Workflow Alert] daily_tech_news failed"
      body: "{{workflow.error_log}}"
```

### 5.3 关键语法说明

| 语法 | 说明 | 示例 |
|------|------|------|
| `{{steps.<id>.output}}` | 引用上游步骤的输出 | `{{steps.summarize.output.summary}}` |
| `{{steps.<id>.status}}` | 引用上游步骤的执行状态 | `{{steps.fetch_news.status}} == 'completed'` |
| `{{workflow.any_failed}}` | 工作流级别状态 | `{{workflow.any_failed}} == true` |
| `{{workflow.error_log}}` | 错误日志汇总 | 用于通知步骤 |
| `${ENV_VAR}` | 环境变量注入 | `${FEISHU_WEBHOOK_URL}` |
| `depends_on` | 依赖声明，支持 DAG | `depends_on: ["step_a", "step_b"]` |
| `condition` | 条件执行 | `condition: "{{steps.x.status}} == 'failed'"` |
| `retries` | 重试次数 | `retries: 3` |
| `timeout_sec` | 单步超时 | `timeout_sec: 30` |

### 5.4 Trigger 引擎设计

```rust
// crates/agents/src/workflow/trigger.rs（新增）
pub enum TriggerType {
    Cron { schedule: String, timezone: String },
    Event { source: String, filter: String },      // 如：新邮件、Git Push
    Webhook { path: String, method: String, auth: WebhookAuth },
    Manual,                                         // 用户通过 API/聊天触发
}

pub struct TriggerEngine {
    cron_scheduler: CronScheduler,      // 基于 tokio-cron-scheduler
    event_bus: broadcast::Receiver<SystemEvent>,
    webhook_routes: HashMap<String, WorkflowId>,
}

impl TriggerEngine {
    /// 注册工作流的所有触发器
    pub async fn register(&mut self, workflow: &WorkflowDefinition) -> Result<()> {
        for trigger in &workflow.triggers {
            match trigger {
                TriggerType::Cron { schedule, timezone } => {
                    self.cron_scheduler.add(schedule, timezone, workflow.id.clone()).await?;
                }
                TriggerType::Event { source, filter } => {
                    self.event_bus.subscribe(source, filter, workflow.id.clone());
                }
                TriggerType::Webhook { path, .. } => {
                    self.webhook_routes.insert(path.clone(), workflow.id.clone());
                }
                TriggerType::Manual => {}
            }
        }
        Ok(())
    }
}
```

### 5.5 与 DagScheduler 的集成

BeeBotOS 已具备 `DagScheduler`（`crates/agents/src/queue/dag_scheduler.rs`），本方案将其作为 Workflow 的底层执行引擎：

```rust
// crates/agents/src/workflow/engine.rs（新增）
pub struct WorkflowEngine {
    dag_scheduler: Arc<DagScheduler>,
    skill_registry: Arc<SkillRegistry>,
    state_store: Arc<StateStore>,
}

impl WorkflowEngine {
    /// 将 YAML Workflow 编译为 DagWorkflow 并提交执行
    pub async fn submit(&self, def: &WorkflowDefinition, trigger_context: Value) -> Result<WorkflowInstanceId> {
        // 1. 编译：将 WorkflowDefinition 转换为 DagWorkflow
        let dag_workflow = self.compile_to_dag(def)?;
        
        // 2. 创建 WorkflowInstance（持久化状态）
        let instance = WorkflowInstance::new(def.id.clone(), trigger_context);
        let instance_id = instance.id.clone();
        self.state_store.save_instance(&instance).await?;
        
        // 3. 为每个步骤创建 DagTask
        for step in &def.steps {
            let dag_task = DagTask {
                id: step.id.clone(),
                name: step.name.clone(),
                task_type: "skill_execution".to_string(),
                parameters: self.build_step_params(step, &instance),
                priority: 5,
                estimated_duration_sec: step.timeout_sec.unwrap_or(30),
                required_capabilities: vec!["skill:call".to_string()],
                ..Default::default()
            };
            dag_workflow.add_task(dag_task);
        }
        
        // 4. 添加依赖边
        for step in &def.steps {
            if let Some(deps) = &step.depends_on {
                for dep in deps {
                    dag_workflow.add_dependency(dep, &step.id);
                }
            }
        }
        
        // 5. 提交到 DagScheduler 执行
        self.dag_scheduler.submit_workflow(dag_workflow, instance_id.clone()).await?;
        
        Ok(instance_id)
    }
    
    /// 编译步骤参数：解析模板语法 {{steps.x.output}}
    fn build_step_params(&self, step: &WorkflowStep, instance: &WorkflowInstance) -> Value {
        let mut params = step.params.clone();
        
        // 递归替换模板变量
        replace_template_vars(&mut params, |path| {
            self.resolve_template_path(path, instance)
        });
        
        params
    }
}
```

### 5.6 Workflow 状态持久化

```rust
// crates/agents/src/workflow/state.rs（新增）
pub struct WorkflowInstance {
    pub id: WorkflowInstanceId,
    pub workflow_id: WorkflowId,
    pub status: WorkflowStatus,          // Pending / Running / Completed / Failed / Cancelled
    pub step_states: HashMap<String, StepState>,
    pub trigger_context: Value,          // 触发时的上下文（如 Cron 时间、Webhook payload）
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_log: Vec<WorkflowError>,
}

pub struct StepState {
    pub step_id: String,
    pub status: StepStatus,              // Pending / Ready / Running / Completed / Failed / Skipped
    pub input: Value,
    pub output: Option<Value>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub retry_count: u32,
    pub error: Option<String>,
}
```

---

## 6. Multi-Agent Workflow（多 Agent 协作）

### 6.1 主 Agent + 子 Agent 模式

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Multi-Agent Workflow                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌─────────────┐                                                           │
│   │  Main Agent │  ← 用户交互入口，负责任务拆解与结果汇总                      │
│   │   (主编)    │                                                           │
│   └──────┬──────┘                                                           │
│          │ "研究这三家公司：阿里、腾讯、字节"                                  │
│          ▼                                                                  │
│   ┌──────────────────────────────────────────────────────────┐             │
│   │                    SubAgent Pool                          │             │
│   │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐      │             │
│   │  │  Agent A    │  │  Agent B    │  │  Agent C    │      │             │
│   │  │ (研究员-阿里)│  │ (研究员-腾讯)│  │ (研究员-字节)│      │             │
│   │  │ 独立 Session│  │ 独立 Session│  │ 独立 Session│      │             │
│   │  │ 独立 Memory │  │ 独立 Memory │  │ 独立 Memory │      │             │
│   │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘      │             │
│   │         │                │                │              │             │
│   │         └────────────────┴────────────────┘              │             │
│   │                          │                               │             │
│   │                    join_all (并行执行)                    │             │
│   │                          │                               │             │
│   │                    返回三份独立报告                        │             │
│   └──────────────────────────┬───────────────────────────────┘             │
│                              │                                              │
│                              ▼                                              │
│   ┌─────────────────────────────────────────────────────────┐              │
│   │  Main Agent 汇总整合                                     │              │
│   │  "基于 A/B/C 三份报告，生成最终对比分析..."               │              │
│   └─────────────────────────────────────────────────────────┘              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 6.2 实现方式

在现有 `PlanningEngine` 的 `Action::Delegate` 基础上扩展：

```rust
// crates/agents/src/planning/plan.rs（扩展现有 Action）
pub enum Action {
    ToolUse { tool_name: String, parameters: Value },
    LLMReasoning { prompt: String, context: Value },
    SubPlan { plan_id: PlanId },
    Delegate { 
        agent_id: String,           // 子 Agent ID
        task: String,               // 任务描述
        skill_hint: Option<String>, // 推荐使用的 Skill
        output_schema: Option<Value>, // 期望输出结构
    },
    Wait { condition: String, timeout: Duration },
    UserInteraction { question: String },
    // ─── 新增：并行委托 ───
    ParallelDelegate {
        branches: Vec<DelegateBranch>,
        merge_strategy: MergeStrategy,
    },
}

pub struct DelegateBranch {
    pub branch_id: String,
    pub agent_config: AgentConfig,  // 子 Agent 的配置（可独立指定 LLM、Memory 等）
    pub task: String,
    pub skill_hint: Option<String>,
}
```

### 6.3 Gateway API 集成

```rust
// apps/gateway/src/handlers/http/workflows.rs（新增）

/// 列出所有已注册的工作流
async fn list_workflows(State(state): State<AppState>) -> Result<Json<Vec<WorkflowSummary>>, GatewayError>;

/// 手动触发工作流
async fn trigger_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<WorkflowInstanceId>, GatewayError>;

/// 查询工作流实例状态
async fn get_workflow_status(
    State(state): State<AppState>,
    Path(instance_id): Path<String>,
) -> Result<Json<WorkflowInstance>, GatewayError>;

/// 取消正在执行的工作流
async fn cancel_workflow(
    State(state): State<AppState>,
    Path(instance_id): Path<String>,
) -> Result<Json<()>, GatewayError>;

/// Webhook 触发端点（动态路由）
async fn workflow_webhook(
    State(state): State<AppState>,
    Path(workflow_path): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<WorkflowInstanceId>, GatewayError>;
```

---

## 7. 与现有系统的兼容性

### 7.1 与 PlanningEngine 的关系

```
┌─────────────────────────────────────────────────────────────────────────────┐
│              PlanningEngine vs WorkflowEngine 协作关系                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   用户输入                                                                   │
│      │                                                                      │
│      ▼                                                                      │
│   ┌─────────────────┐                                                       │
│   │ 复杂度检测       │  "生成今日早报" → 检测到预定义 Workflow → 直接执行    │
│   │ (Agent::handle_ │  "随便聊聊" → 简单对话 → 走原有 LLM 路径              │
│   │  llm_task)      │  "分析这三份财报" → 复杂任务 → 走 PlanningEngine      │
│   └─────────────────┘                                                       │
│                                                                             │
│   三种执行路径：                                                             │
│                                                                             │
│   1️⃣ 简单对话 ───────────────────────▶ 原有 LLM Chat 路径（无变化）          │
│                                                                             │
│   2️⃣ 预定义 Workflow ─────────────────▶ WorkflowEngine → DagScheduler       │
│      （如 daily_news.yaml 已存在）                                          │
│                                                                             │
│   3️⃣ 复杂任务（无预定义 Workflow）─────▶ PlanningEngine → HybridPlanner     │
│      （动态生成 Plan，PlanStep 可引用 Workflow 作为 SubPlan）               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**关键设计**：`PlanningEngine` 的 `Action::SubPlan` 可以引用一个预定义的 Workflow，实现**动态规划 + 预定义流程**的混合模式。

### 7.2 与 SkillRegistry 的关系

- Workflow 的 `steps[].skill` 字段直接映射到 `SkillRegistry` 中的 `skill_id`
- Workflow 执行时通过 `Agent::execute_registered_skill()` 调用 Skill，复用现有的 WASM / Knowledge / Code-driven 执行路径
- Skill 的 `permissions` 声明仍然生效，Workflow 执行前统一校验 Capability

### 7.3 与现有 Gateway 的集成点

| 组件 | 集成方式 |
|------|---------|
| `MessageProcessor` | `try_match_skill()` 之后增加 `try_match_workflow()` — 若用户指令匹配某 Workflow 的触发条件，直接提交 WorkflowEngine |
| `AgentResolver` | 无变化，Workflow 在 Gateway 层调度，不经过 AgentResolver |
| `AgentRuntime` | `execute_task()` 支持新的 `TaskType::WorkflowExecution`，Agent 收到后调用 `WorkflowEngine::submit()` |
| `StateStore` | 扩展 `StateQuery` 支持 `ListWorkflowInstances`、`GetWorkflowInstance` |

---

## 8. 实施路线图

### Phase 1: 基础设施（2 周）

- [ ] 新建 `crates/agents/src/workflow/` 模块目录
  - [ ] `definition.rs` — YAML/JSON 工作流定义解析
  - [ ] `trigger.rs` — Cron / Event / Webhook / Manual 触发器
  - [ ] `engine.rs` — WorkflowEngine（编译为 DagWorkflow + 提交调度）
  - [ ] `state.rs` — WorkflowInstance / StepState 状态模型
  - [ ] `template.rs` — `{{steps.x.output}}` 模板解析引擎
- [ ] 扩展 `DagScheduler`，支持与 Agent Runtime 的集成接口
- [ ] 新建 `crates/agents/src/skills/composition/` 模块
  - [ ] `pipeline.rs` — SkillPipeline 顺序链
  - [ ] `parallel.rs` — SkillParallel 并行组
  - [ ] `conditional.rs` — SkillConditional 条件分支
  - [ ] `loop.rs` — SkillLoop 循环/重试
- [ ] 扩展 `tool_set.rs`，新增 `SkillCallTool`

### Phase 2: Gateway 集成（1 周）

- [ ] 新建 `apps/gateway/src/handlers/http/workflows.rs` — REST API
- [ ] 扩展 `MessageProcessor::try_match_workflow()` — 聊天触发 Workflow
- [ ] 扩展 `AgentRuntime::execute_task()` — 支持 `TaskType::WorkflowExecution`
- [ ] 新建 `workflows/` 目录，迁移示例工作流

### Phase 3: Trigger 引擎（1 周）

- [ ] 集成 `tokio-cron-scheduler` 实现 Cron 触发
- [ ] 实现 Event Bus 订阅机制（监听系统事件触发 Workflow）
- [ ] 实现 Webhook 动态路由注册
- [ ] 支持环境变量注入（`${ENV_VAR}`）

### Phase 4: 多 Agent 与可视化（2 周）

- [ ] 扩展 `PlanningEngine::Action::ParallelDelegate`
- [ ] 实现子 Agent 自动创建与销毁生命周期管理
- [ ] Workflow 状态持久化到 SQLite/PostgreSQL
- [ ] 基础 Dashboard API（列出实例、查看状态、查看日志）
- [ ] 编写完整示例工作流（daily_news、financial_report、manga_pipeline）

### Phase 5: 测试与文档（1 周）

- [ ] 单元测试：模板解析、条件判断、DAG 编译
- [ ] 集成测试：端到端 Workflow 执行（Cron + Manual + Webhook）
- [ ] 性能测试：并行 10 个 Skill 的 Workflow 执行延迟
- [ ] 更新 `docs/specs/workflow-format-v1.md`

---

## 9. 附录

### 附录 A: 完整 YAML 示例 — 多 Agent 内容工厂

```yaml
# workflows/content_factory.yaml
name: "multi_agent_content_factory"
description: "针对特定主题，自动完成调研、撰写、整合的全套内容生产流程"
version: "1.0.0"

triggers:
  - type: manual
  - type: webhook
    path: "/webhook/content-factory"
    method: "POST"

config:
  timeout_sec: 600
  continue_on_failure: false

steps:
  # Step 1: 主编 Agent 拆解任务
  - id: plan_tasks
    name: "Plan Content Tasks"
    skill: planning_engine
    params:
      goal: "为 '{{input.topic}}' 生成一份完整的市场分析报告"
      strategy: "hybrid"
    timeout_sec: 60
    
  # Step 2: 并行执行三个子 Agent 调研
  - id: research_parallel
    name: "Parallel Research"
    skill: parallel_delegate
    depends_on: ["plan_tasks"]
    params:
      branches:
        - agent_role: "market_researcher"
          task: "研究 '{{input.topic}}' 的市场规模与竞争格局"
          skill_hint: "market_analysis"
        - agent_role: "tech_researcher"
          task: "研究 '{{input.topic}}' 的技术趋势与核心专利"
          skill_hint: "tech_research"
        - agent_role: "user_researcher"
          task: "研究 '{{input.topic}}' 的用户画像与需求痛点"
          skill_hint: "user_research"
      merge_strategy: "json_merge"
    timeout_sec: 120
    
  # Step 3: 写手 Agent 基于调研报告创作文案
  - id: draft_content
    name: "Draft Content"
    skill: content_writer
    depends_on: ["research_parallel"]
    params:
      research_data: "{{steps.research_parallel.output}}"
      style: "{{input.style}}"
      word_count: 2000
    timeout_sec: 120
    
  # Step 4: 审核 Agent 质量检查
  - id: review_content
    name: "Review Content"
    skill: content_reviewer
    depends_on: ["draft_content"]
    params:
      content: "{{steps.draft_content.output.article}}"
      criteria: ["准确性", "可读性", "原创性"]
    timeout_sec: 60
    
  # Step 5: 条件分支：审核通过则发布，否则返修
  - id: publish_or_revise
    name: "Publish or Revise"
    skill: conditional_router
    depends_on: ["review_content"]
    params:
      condition: "{{steps.review_content.output.score}} >= 80"
      then_skill: "publisher"
      then_params:
        content: "{{steps.draft_content.output.article}}"
        channel: "{{input.publish_channel}}"
      else_skill: "content_writer"
      else_params:
        research_data: "{{steps.research_parallel.output}}"
        revision_notes: "{{steps.review_content.output.feedback}}"
    timeout_sec: 30
```

### 附录 B: 完整 YAML 示例 — 全自动漫剧生产流水线

```yaml
# workflows/manga_pipeline.yaml
name: "manga_video_pipeline"
description: "从创意主题到 AI 漫剧短视频的端到端自动化生产"
version: "1.0.0"

triggers:
  - type: cron
    schedule: "0 2 * * *"         # 每天凌晨 2 点自动生产
  - type: manual

config:
  timeout_sec: 1800               # 30 分钟（视频合成耗时较长）
  max_retries: 2
  continue_on_failure: false

steps:
  # Step 1: 创意生成
  - id: generate_idea
    name: "Generate Story Idea"
    skill: story_idea_generator
    params:
      theme: "{{input.theme}}"
      genre: "{{input.genre}}"
      target_duration: "{{input.duration}}"
    timeout_sec: 60
    
  # Step 2: 剧本生成
  - id: generate_script
    name: "Generate Script"
    skill: seed_script_gen
    depends_on: ["generate_idea"]
    params:
      idea: "{{steps.generate_idea.output.idea}}"
      characters: "{{input.characters}}"
      scenes: "{{input.scenes}}"
    timeout_sec: 120
    
  # Step 3: 分镜设计（可并行：每集/每场景独立）
  - id: storyboard_design
    name: "Design Storyboard"
    skill: seed_storyboard
    depends_on: ["generate_script"]
    params:
      script: "{{steps.generate_script.output.script}}"
      art_style: "{{input.art_style}}"
    timeout_sec: 300
    retries: 2
    
  # Step 4: 素材生成（图像 + 语音）
  - id: generate_assets
    name: "Generate Assets"
    skill: seed_asset_gen
    depends_on: ["storyboard_design"]
    params:
      storyboard: "{{steps.storyboard_design.output.storyboard}}"
      voice_actor: "{{input.voice_actor}}"
    timeout_sec: 600
    
  # Step 5: 视频合成
  - id: video_compose
    name: "Compose Video"
    skill: seed_video_gen
    depends_on: ["generate_assets"]
    params:
      assets: "{{steps.generate_assets.output.assets}}"
      bgm: "{{input.bgm}}"
      resolution: "{{input.resolution}}"
    timeout_sec: 600
    
  # Step 6: 后处理（字幕、转码、封面）
  - id: post_process
    name: "Post Process"
    skill: video_post_processor
    depends_on: ["video_compose"]
    params:
      video: "{{steps.video_compose.output.video_url}}"
      add_subtitles: true
      generate_thumbnail: true
    timeout_sec: 120
    
  # Step 7: 发布到平台
  - id: publish
    name: "Publish Video"
    skill: video_publisher
    depends_on: ["post_process"]
    params:
      video: "{{steps.post_process.output.final_video}}"
      thumbnail: "{{steps.post_process.output.thumbnail}}"
      platforms: "{{input.platforms}}"    # ["bilibili", "douyin", "youtube"]
      title: "{{steps.generate_script.output.title}}"
    timeout_sec: 60
    
  # Step 8: 完成通知
  - id: notify_complete
    name: "Notify Completion"
    skill: feishu_bot
    depends_on: ["publish"]
    params:
      message: |
        🎬 漫剧《{{steps.generate_script.output.title}}》已自动完成！
        观看链接：{{steps.publish.output.urls}}
        制作耗时：{{workflow.duration}} 秒
    timeout_sec: 15
```

### 附录 C: 与 OpenClaw 的能力对比

| 维度 | OpenClaw | BeeBotOS（本方案） |
|------|----------|-------------------|
| Skill 组合 | Pipeline + Parallel + Conditional | ✅ 全部支持 + Loop/Retry |
| Workflow 定义 | YAML | ✅ YAML（兼容语法） |
| Trigger | Cron / Event / Webhook / Manual | ✅ 全部支持 |
| 数据传递 | `{{steps.x.output}}` | ✅ 相同语法 |
| 多 Agent | Subagent 独立 Session | ✅ ParallelDelegate + 独立 Memory |
| 执行引擎 | 自定义 | ✅ 复用现有 `DagScheduler` |
| 持久化 | 基础 | ✅ WorkflowInstance + StepState 全状态持久化 |
| 安全 | 目录隔离 | ✅ Capability L0-L10 + 内核沙箱 + Skill 权限声明 |
| WASM 支持 | 无 | ✅ 原生 wasmtime |
| 可视化 | `openclaw dashboard` | 🚧 待实现（Phase 4） |

---

**文档维护者**: BeeBotOS Core Team  
**下次评审日期**: 2026-05-30

&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&&


