# BeeBotOS Skill 执行架构重构方案

> **版本**: v1.0  
> **状态**: 设计草案  
> **日期**: 2026-04-29  
> **关联文档**: [Skill Format v1](../specs/skill-format-v1.md), [Agent Runtime](../architecture/04-agent-runtime.md), [Capability System](../specs/CAPABILITY_SYSTEM.md)

---

## 1. 背景与动机

当前 BeeBotOS 的 Skill 系统存在两条割裂的执行路径：

1. **内置 Markdown Skill** (`/skills/*.md`)：启动时被解析为 `prompt_template` + `description`，执行时直接作为 **LLM System Prompt** 回退使用。LLM 只是"扮演"该 Skill，无法调用外部脚本或系统工具。
2. **WASM Skill** (`skill.wasm` + `skill.yaml`)：通过 `wasmtime` 沙箱执行，能力强但开发门槛高，且当前 `/skills` 目录下无任何 WASM 技能包。

这导致了一个关键缺陷：**`.py` 和 `.js` 文件即使放在 Skill 目录中，也完全无法被调用执行**。系统既无 Python/JS 运行时，也未将脚本作为独立子进程调用的机制。

本方案参考 **OpenClaw** 的 Skill 执行模型，引入 **"知识驱动型"** 与 **"代码驱动型"** 双轨并行的 Skill 形态，使 LLM 通过阅读 `SKILL.md` "说明书"，动态组合内置工具或调用外部脚本，完成复杂任务。

---

## 2. 设计目标

| 目标 | 说明 |
|------|------|
| **兼容现有 WASM 能力** | 保留 `wasmtime` 沙箱执行路径，已部署的 WASM Skill 无需修改。 |
| **支持 `.py` / `.js` / `.sh` 脚本执行** | 通过子进程隔离执行 Skill 目录内的脚本，而非嵌入解释器。 |
| **延迟加载 (Lazy Loading)** | 启动时仅提取 Skill 元数据（名称+描述）注入 LLM 上下文，实际调用时才完整读取 `SKILL.md`，节省 Token。 |
| **LLM 工具链驱动** | LLM 不再直接"扮演" Skill，而是阅读 `SKILL.md` 后，通过调用 `file_read`、`process_exec`、`bash` 等**通用工具**完成任务。 |
| **安全沙箱化** | 所有子进程执行受 Capability 等级（L0-L10）和内核 Sandbox 约束，禁止越权访问。 |
| **低门槛扩展** | 开发者只需编写 Markdown 说明书 + 可选脚本，无需学习 WASM 或 Rust。 |

---

## 3. 核心理念：三种 Skill 形态共存

重构后，BeeBotOS 同时支持三种 Skill 实现形态：

```
┌─────────────────────────────────────────────────────────────────────┐
│                     BeeBotOS Skill 生态                             │
├──────────────────┬─────────────────────┬────────────────────────────┤
│  📦 WASM 原生型   │  📜 知识驱动型       │  🔧 代码驱动型              │
│  (现有保留)       │  (新增)              │  (新增)                    │
├──────────────────┼─────────────────────┼────────────────────────────┤
│ skill.yaml       │ SKILL.md            │ SKILL.md + script.py       │
│ skill.wasm       │ (纯文档)            │ (或 .js / .sh)             │
├──────────────────┼─────────────────────┼────────────────────────────┤
│ 由 Kernel 的     │ 由 LLM 通过阅读     │ 由 LLM 阅读 SKILL.md 后，   │
│ WasmEngine 直接  │ SKILL.md，组合调用  │ 通过 process_exec 工具      │
│ 编译执行          │ 内置工具链完成      │ 启动子进程执行脚本          │
├──────────────────┼─────────────────────┼────────────────────────────┤
│ 高性能、强隔离   │ 零代码、纯策略      │ 兼顾灵活性与复用性          │
│ 适合核心算法     │ 适合文件重组、      │ 适合数据分析、API 对接、    │
│                  │ 代码变更等确定性任务 │ 图表绘制等复杂逻辑          │
└──────────────────┴─────────────────────┴────────────────────────────┘
```

---

## 4. Skill 目录结构规范（新版）

### 4.1 工作区 Skill 目录

```
skills/
├── coding/
│   ├── python_developer/
│   │   └── SKILL.md                 # 知识驱动型：纯文档
│   ├── rust_developer/
│   │   └── SKILL.md
│   └── code_reviewer/
│       ├── SKILL.md                 # 代码驱动型：文档 + 脚本
│       └── review.py                # 可选：被 process_exec 调用
├── daily/
│   └── weather_assistant/
│       ├── SKILL.md
│       └── fetch_weather.py         # Python 脚本
├── crypto-trading-bot/
│   ├── SKILL.md
│   ├── market_data.js               # Node.js 脚本
│   └── backtest.py
└── analytics/
    └── data_analyst/
        ├── SKILL.md
        └── analyze.py
```

### 4.2 用户级 Skill 目录（可选扩展）

```
~/.beebotos/skills/          # 用户自定义 Skill（优先级次之于工作区）
```

### 4.3 SKILL.md 规范模板

```markdown
---
name: weather-assistant          # 唯一标识（snake_case）
version: 1.0.0
description: 获取指定城市的实时天气信息
author: BeeBotOS Team
license: MIT
category: daily
tags: [weather, api, daily]
---

# Weather Assistant

## 概述

本 Skill 用于获取指定城市的实时天气数据。支持国内主要城市。

## 可用工具 / 脚本

- `fetch_weather.py` — 主脚本，接收 `--city` 参数，返回 JSON 格式天气数据

## 使用方法

1. 读取用户提供的城市名称
2. 调用脚本：`python3 {SKILL_DIR}/fetch_weather.py --city "城市名"`
3. 将脚本输出的 JSON 解析为自然语言回复用户

## 参数说明

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| city | string | 是 | 城市中文名或英文名 |

## 输出格式

脚本返回 JSON：
```json
{
  "city": "Beijing",
  "temperature": 22,
  "condition": "Sunny",
  "humidity": 45
}
```

## 示例

用户："今天北京天气怎么样？"
→ 执行：`python3 {SKILL_DIR}/fetch_weather.py --city "北京"`
→ 回复："北京今天晴天，22°C，湿度45%。"

## 注意事项

- 若城市名不明确，先询问用户确认
- 脚本失败时（非零退出码），向用户说明服务暂不可用
```

> **占位符约定**: `{SKILL_DIR}` 由执行引擎在调用时替换为该 Skill 的绝对路径。

---

## 5. 阶段一：Skill 发现与延迟加载 (Lazy Loading)

### 5.1 启动扫描流程

```rust
// crates/agents/src/skills/discovery.rs (新增)
pub struct SkillDiscovery {
    paths: Vec<PathBuf>, // [workspace/skills/, ~/.beebotos/skills/]
}

impl SkillDiscovery {
    pub async fn scan(&self) -> Vec<SkillMetadata> {
        // 1. 递归扫描所有子目录
        // 2. 只读取 SKILL.md 的 Front Matter (---...---) 或第一行标题
        // 3. 提取：id, name, version, description, category, tags
        // 4. 不读取 ## 以下的正文内容（延迟加载）
    }
}
```

### 5.2 元数据结构

```rust
#[derive(Debug, Clone)]
pub struct SkillMetadata {
    pub id: String,               // "weather-assistant"
    pub name: String,             // "Weather Assistant"
    pub version: Version,
    pub description: String,      // 一句话描述，用于注入 System Prompt
    pub category: String,
    pub tags: Vec<String>,
    pub path: PathBuf,            // Skill 所在目录绝对路径
    pub kind: SkillKind,          // Knowledge | Code | Wasm
}

pub enum SkillKind {
    Knowledge,   // 仅有 SKILL.md
    Code,        // SKILL.md + .py/.js/.sh
    Wasm,        // skill.yaml + skill.wasm (现有)
}
```

### 5.3 注入 LLM 系统提示词

启动时，将所有 `SkillMetadata` 格式化为技能清单，注入 Agent 的 System Prompt：

```text
You are a BeeBotOS agent. You have access to the following skills:

1. weather-assistant (daily) — 获取指定城市的实时天气信息
2. python-developer (coding) — Python 代码生成与审查
3. code-reviewer (coding) — 自动化代码审查

When a user request matches a skill, read its SKILL.md first using the
file_read tool, then follow the instructions to use available tools or scripts.
```

> **Token 控制**: 仅注入元数据（约 20-50 token / skill），而非完整 Markdown 正文。

### 5.4 优先级策略

同名 Skill 的加载优先级：

```
工作区 Skill (./skills/) > 用户 Skill (~/.beebotos/skills/) > 内置 WASM Skill
```

---

## 6. 阶段二：LLM 工具调用框架扩展

当前 BeeBotOS 已有 MCP 工具基础设施，但缺少通用的**系统级工具**。需要扩展 `ToolRegistry`，使 LLM 能够操作文件系统和启动子进程。

### 6.1 新增内置工具集

| 工具名 | 能力 | Capability 要求 | 说明 |
|--------|------|----------------|------|
| `file_read` | 读取文件内容 | L1FileRead | 读取 Skill 目录或工作区文件 |
| `file_write` | 写入文件内容 | L2FileWrite | 修改工作区文件 |
| `file_list` | 列出目录内容 | L1FileRead | 查看 Skill 目录结构 |
| `process_exec` | 执行子进程命令 | L3NetworkOut + L2FileWrite | 启动 `.py` / `.js` / `.sh` 脚本 |
| `bash_shell` | 执行 Bash 命令 | L3NetworkOut | 通用 Shell 命令（受限） |
| `web_search` | 网络搜索 | L3NetworkOut | 现有能力保留 |
| `browser_navigate` | 浏览器自动化 | L3NetworkOut | 现有能力保留 |

### 6.2 process_exec 工具定义

```json
{
  "name": "process_exec",
  "description": "Execute an external script or command in a sandboxed subprocess. The working directory is restricted to the skill's directory or the workspace root.",
  "input_schema": {
    "type": "object",
    "required": ["command"],
    "properties": {
      "command": {
        "type": "string",
        "description": "The command to execute, e.g. 'python3 fetch_weather.py --city Beijing'"
      },
      "working_dir": {
        "type": "string",
        "description": "Optional working directory. Defaults to the skill's directory."
      },
      "timeout_ms": {
        "type": "integer",
        "default": 30000,
        "description": "Maximum execution time in milliseconds."
      },
      "env": {
        "type": "object",
        "description": "Optional environment variables."
      }
    }
  }
}
```

### 6.3 LLM 工具调用循环 (ReAct 模式)

```
User Input
    ↓
Agent LLM 分析意图 → 匹配 SkillMetadata
    ↓
若需使用 Skill → 调用 file_read 读取 {SKILL_DIR}/SKILL.md
    ↓
LLM 阅读 SKILL.md → 理解可用脚本与参数
    ↓
LLM 决定调用 process_exec / bash_shell / file_write 等工具
    ↓
工具执行器在沙箱中运行 → 返回结果给 LLM
    ↓
LLM 整合结果 → 生成最终回复
```

---

## 7. 阶段三：Skill 执行引擎

### 7.1 执行路径路由

`Agent::execute_skill()` 根据 `SkillMetadata.kind` 路由到不同执行器：

```rust
// crates/agents/src/skills/execution_router.rs (新增)
pub async fn execute(&self, skill_id: &str, user_input: &str) -> Result<SkillOutput, SkillError> {
    let meta = self.registry.get_metadata(skill_id).await?;
    
    match meta.kind {
        SkillKind::Wasm => {
            // 现有路径：WasmEngine 执行
            self.wasm_executor.execute(skill_id, user_input).await
        }
        SkillKind::Knowledge => {
            // 知识驱动：读取 SKILL.md，通过 LLM ReAct 工具链执行
            self.knowledge_executor.execute(&meta, user_input).await
        }
        SkillKind::Code => {
            // 代码驱动：读取 SKILL.md，通过 LLM ReAct 调用 process_exec
            self.code_executor.execute(&meta, user_input).await
        }
    }
}
```

### 7.2 知识驱动型执行流程

```rust
// crates/agents/src/skills/knowledge_executor.rs (新增)
pub async fn execute(&self, meta: &SkillMetadata, user_input: &str) -> Result<SkillOutput, SkillError> {
    // 1. 延迟加载：读取 SKILL.md 全文
    let skill_md = tokio::fs::read_to_string(meta.path.join("SKILL.md")).await?;
    
    // 2. 构建 ReAct 提示词
    let system_prompt = format!(
        "You are the '{}' skill. Follow the instructions below to complete the task.\n\n{}",
        meta.name, skill_md
    );
    
    // 3. 启动 LLM 工具调用循环
    let mut messages = vec![
        Message::system(system_prompt),
        Message::user(user_input),
    ];
    
    loop {
        let response = self.llm.complete_with_tools(messages.clone(), &self.tools).await?;
        
        if let Some(tool_calls) = response.tool_calls {
            for call in tool_calls {
                let result = self.tool_registry.execute(&call.name, call.arguments).await?;
                messages.push(Message::tool_result(call.id, result));
            }
        } else {
            // LLM 已生成最终回复
            return Ok(SkillOutput::new(response.content));
        }
    }
}
```

### 7.3 代码驱动型执行流程

与知识驱动型类似，但 `SKILL.md` 中明确引用了可执行脚本。LLM 会构造类似如下命令：

```bash
python3 /absolute/path/to/skills/weather_assistant/fetch_weather.py --city "Beijing"
```

`process_exec` 工具的实现：

```rust
// crates/agents/src/tools/process_exec.rs (新增)
pub struct ProcessExecTool;

#[async_trait]
impl Tool for ProcessExecTool {
    fn name(&self) -> &str { "process_exec" }
    
    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let cmd = args["command"].as_str().ok_or(ToolError::MissingArg("command"))?;
        let timeout = args["timeout_ms"].as_u64().unwrap_or(30000);
        let work_dir = args["working_dir"].as_str();
        
        // Capability 校验
        self.check_capability("process:exec").await?;
        
        // 路径安全检查：禁止访问 /etc, /root, /home 等敏感目录
        let allowed_prefixes = self.get_allowed_work_dirs().await;
        
        // 执行子进程
        let mut command = tokio::process::Command::new("sh");
        command.arg("-c").arg(cmd);
        command.kill_on_drop(true);
        
        if let Some(dir) = work_dir {
            command.current_dir(dir);
        }
        
        let output = tokio::time::timeout(
            Duration::from_millis(timeout),
            command.output()
        ).await.map_err(|_| ToolError::Timeout)?;
        
        Ok(json!({
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "exit_code": output.status.code().unwrap_or(-1),
        }))
    }
}
```

---

## 8. 安全模型与 Capability 集成

### 8.1 子进程沙箱约束

所有 `process_exec` 和 `bash_shell` 调用必须受以下约束：

| 约束维度 | 策略 | 实现方式 |
|----------|------|----------|
| **文件系统** | 只能访问 Skill 所在目录、工作区目录和 `/tmp` | 启动前校验 `current_dir` 和命令中的路径参数 |
| **网络** | 默认禁止；需要网络时必须声明 `permissions: ["network:out"]` | Capability L3NetworkOut 校验 |
| **执行时间** | 默认 30s，可配置 | Tokio `timeout` |
| **环境变量** | 禁止继承敏感 env（如 `JWT_SECRET`, `PRIVATE_KEY`） | 清空 env，仅注入白名单变量 |
| **资源限制** | CPU 1 core，内存 512MB | `ulimit` / cgroups (Linux) |

### 8.2 Capability 等级映射

```rust
// crates/kernel/src/capabilities/levels.rs (扩展)
pub enum CapabilityLevel {
    L0LocalCompute = 0,       // 纯本地计算，无 IO
    L1FileRead = 1,           // file_read
    L2FileWrite = 2,          // file_write
    L3NetworkOut = 3,         // process_exec (含网络脚本), web_search
    L4ProcessExec = 4,        // bash_shell, 任意子进程
    // ... 保留 L5-L10
}
```

Skill 在 `SKILL.md` 的 Front Matter 中声明所需权限：

```yaml
---
name: weather-assistant
permissions:
  - file:read
  - network:out
  - process:exec
capability_level: 3
---
```

### 8.3 脚本目录隔离

每个代码驱动型 Skill 的子进程默认 `cwd` 为该 Skill 的目录，禁止通过 `../../` 逃逸：

```rust
fn validate_path_in_skill_dir(path: &Path, skill_dir: &Path) -> Result<(), SecurityError> {
    let canonical = path.canonicalize()?;
    let skill_canonical = skill_dir.canonicalize()?;
    if !canonical.starts_with(&skill_canonical) {
        return Err(SecurityError::PathEscape);
    }
    Ok(())
}
```

---

## 9. 与现有系统的兼容性

### 9.1 内置 Markdown Skill 迁移

当前 `/skills/**/*.md` 文件需要从**扁平文件**迁移为**目录化结构**，并改名为 `SKILL.md`：

```bash
# 迁移前
skills/coding/python_developer.md

# 迁移后
skills/coding/python_developer/SKILL.md
```

若保留旧路径，系统可维持一个**兼容层**：

```rust
// builtin_loader.rs 兼容性处理
if path.is_file() && path.extension() == Some("md") {
    // 旧格式：直接作为知识驱动型 Skill 加载
    let skill_dir = path.parent().unwrap();
    let skill_id = path.file_stem().unwrap();
    register_as_knowledge_skill(skill_id, skill_dir).await;
}
```

### 9.2 WASM Skill 兼容

现有 `SkillLoader::load_skill()` 和 `SkillExecutor` 逻辑**完全保留**，`SkillKind::Wasm` 直接路由到原有执行路径。

### 9.3 Gateway API 兼容

HTTP / gRPC Skill 安装接口 (`apps/gateway/src/handlers/http/skills.rs`) 需要识别新格式的 Skill 包（ZIP 包含 `SKILL.md`）：

```rust
fn detect_skill_kind(extracted_dir: &Path) -> SkillKind {
    if extracted_dir.join("skill.wasm").exists() {
        SkillKind::Wasm
    } else if extracted_dir.join("SKILL.md").exists() {
        if has_executable_scripts(extracted_dir) {
            SkillKind::Code
        } else {
            SkillKind::Knowledge
        }
    } else {
        SkillKind::Knowledge // 兜底
    }
}
```

---

## 10. 实施路线图

### Phase 1: 基础设施（1-2 周）

- [ ] 新建 `crates/agents/src/skills/discovery.rs` — Skill 扫描与元数据提取
- [ ] 新建 `crates/agents/src/skills/knowledge_executor.rs` — 知识驱动型执行器
- [ ] 新建 `crates/agents/src/skills/code_executor.rs` — 代码驱动型执行器
- [ ] 扩展 `ToolRegistry`，新增 `file_read`, `file_write`, `file_list`, `process_exec`, `bash_shell`
- [ ] 修改 `builtin_loader.rs`，支持目录化 Skill 结构，同时保留 `.md` 文件兼容层

### Phase 2: 安全加固（1 周）

- [ ] 在 `process_exec` 中实现路径逃逸检测
- [ ] 集成 Capability 校验到所有新增工具
- [ ] 实现子进程资源限制（timeout, env 过滤）
- [ ] 安全审计：禁止 `rm -rf /`, `curl | sh` 等危险模式

### Phase 3: LLM 集成（1 周）

- [ ] 修改 Agent System Prompt 生成逻辑，注入 Skill 元数据清单
- [ ] 实现 LLM ReAct 工具调用循环（`complete_with_tools`）
- [ ] 支持 Function Calling 的 Provider（Kimi, OpenAI, Anthropic）适配

### Phase 4: 迁移与测试（1 周）

- [ ] 将现有 `/skills/**/*.md` 迁移为 `/skills/**/SKILL.md` 目录结构
- [ ] 为热门 Skill 添加 `.py` / `.js` 脚本示例（如 `weather_assistant/fetch_weather.py`）
- [ ] 编写集成测试：知识驱动型、代码驱动型、WASM 型各至少一个 e2e 测试
- [ ] 更新 `docs/specs/skill-format-v1.md`，纳入新规范

---

## 11. 附录

### 附录 A: 与 OpenClaw 的对比

| 维度 | OpenClaw | BeeBotOS（重构后） |
|------|----------|-------------------|
| Skill 说明书 | `SKILL.md` | `SKILL.md`（兼容） |
| 脚本执行 | `python3` / `node` 子进程 | `python3` / `node` 子进程（相同） |
| 沙箱 | 基础目录隔离 | **Capability L0-L10 + 内核沙箱 + 路径校验**（更强） |
| WASM 支持 | 无 | **原生 wasmtime 支持** |
| 延迟加载 | 是 | 是 |
| 工具链 | `exec`, `read`, `write`, `bash` | `process_exec`, `file_read`, `file_write`, `bash_shell` + MCP |

### 附录 B: 最小可运行示例

**目录**: `skills/daily/hello_world/`

**SKILL.md**:
```markdown
---
name: hello-world
description: A minimal example skill that runs a Python script
category: daily
permissions:
  - process:exec
---

# Hello World

Run the Python script to greet the user.

## Usage

Execute: `python3 {SKILL_DIR}/hello.py --name "UserName"`
```

**hello.py**:
```python
import argparse
parser = argparse.ArgumentParser()
parser.add_argument("--name", required=True)
args = parser.parse_args()
print(f"Hello, {args.name} from BeeBotOS Skill!")
```

**调用流程**:
1. 用户输入："运行 hello world"
2. LLM 识别 Skill `hello-world`
3. LLM 调用 `file_read` 读取 `SKILL.md`
4. LLM 调用 `process_exec` 执行 `python3 skills/daily/hello_world/hello.py --name "User"`
5. Agent 返回："Hello, User from BeeBotOS Skill!"

---

**文档维护者**: BeeBotOS Core Team  
**下次评审日期**: 2026-05-29
