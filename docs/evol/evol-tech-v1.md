# BeeBotOS 自主学习与进化机制技术需求文档

**文档编号**: evol-tech-v1.md  
**项目名称**: BeeBotOS (Web4.0 AI Autonomous Agent Operating System)  
**版本**: v1.0.0  
**日期**: 2026-05-05  
**状态**: Draft / 技术需求评审  

---

## 1. 文档目标与范围

### 1.1 目标
本文档定义 BeeBotOS 中 **自主学习进化机制（Self-Learning Evolution Engine, SLEE）** 的技术架构与模块需求，涵盖：
- **记忆系统进化（Memory Evolution）**：通过分层持久化 + 主动 Nudge 实现 Agent "越用越懂你"。
- **技能系统进化（Skill Evolution）**：通过 CAPO 算法驱动 SKILL.md / SOUL.md 的自动化进化，实现 "越用越会做"。
- **强化学习训练闭环（RL Training Loop）**：基于 Atropos 框架，以 DAPO 为默认 RL 算法解决熵崩溃，以 PAPO 提供过程级奖励，实现 "从经验到本能" 的跃迁。
- **Rust Burn 推理层**：提供类型安全、跨平台、低延迟的本地/边缘推理能力。

### 1.2 范围
- **适用层级**：主要作用于 Layer 2 Social Brain（社交大脑）与 Layer 3 Autonomous Agents（自主多智能体）。
- **不适用**：Layer 1 Gateway 的应用入口逻辑、Layer 4 Blockchain 的共识机制（但涉及链上记忆存证接口）。
- **关联文档**：需与 BeeBotOS 核心架构文档、智能合约接口规范、DID/A2A 商业交易协议协同阅读。

### 1.3 术语表

| 术语 | 说明 |
|------|------|
| **SLEE** | Self-Learning Evolution Engine，自主学习进化引擎 |
| **CAPO** | Cost-Aware Prompt Optimization，成本感知提示词优化算法 |
| **DAPO** | Dynamic Sampling Policy Optimization，动态采样策略优化 |
| **PAPO** | Process-Aware Policy Optimization，过程感知策略优化 |
| **Atropos** | Nous Research 开源的异步 RL 环境微服务框架 |
| **Burn** | Rust 原生深度学习框架，后端无关（WGPU/CUDA/LibTorch） |
| **Nudge** | 主动提醒引擎，驱动记忆沉淀与技能复盘 |
| **Skill Genealogy** | 技能谱系追踪，记录技能的父子关系与进化路径 |
| **NEAT** | NeuroEvolution of Augmenting Topologies，神经进化算法 |
| **PAD** | Pleasure-Arousal-Dominance，情感模型 |
| **OCEAN** | Openness-Conscientiousness-Extraversion-Agreeableness-Neuroticism，人格模型 |
| **A2A** | Agent-to-Agent，智能体间协作与商业交易协议 |
| **DID** | Decentralized Identifier，去中心化身份标识 |

---

## 2. 总体架构设计

### 2.1 架构原则
1. **Rust-first, Python-auxiliary**：核心推理引擎、记忆存储、技能执行环境使用 Rust；CAPO 进化、RL 训练管线、Atropos 环境服务使用 Python。
2. **异步解耦**：进化过程不得阻塞主 Agent 的任务执行（后台 fork / 微服务化）。
3. **成本感知**：所有进化操作需显式预算控制（Token 上限、API 调用次数上限、GPU 时长配额）。
4. **链上可审计**：关键进化事件（Skill 创建、记忆存证、策略更新）生成哈希并锚定到 Monad 链。
5. **安全边界**：进化产物（记忆、Skill、策略权重）进入系统提示词前必须通过安全扫描与自动回滚机制。

### 2.2 系统架构图

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Layer 2: Social Brain (Rust)                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │
│  │   Hot Memory │  │  Project Mem │  │  User Profile│  │  Soul / Person│  │
│  │  (Session)   │  │  (MEMORY.md) │  │  (USER.md)   │  │  (SOUL.md)   │   │
│  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘   │
│         │                 │                 │                 │             │
│         └─────────────────┴─────────────────┴─────────────────┘             │
│                                   │                                         │
│                    ┌──────────────▼──────────────┐                         │
│                    │    Nudge Engine (Rust)      │                         │
│                    │  - Memory Nudge (10 turns)  │                         │
│                    │  - Skill Nudge (5+ calls)   │                         │
│                    │  - Policy Nudge (RL trigger)│                         │
│                    └──────────────┬──────────────┘                         │
└───────────────────────────────────┼─────────────────────────────────────────┘
                                    │ gRPC / HTTP
┌───────────────────────────────────▼─────────────────────────────────────────┐
│                      Layer 3: Autonomous Agents (Hybrid)                    │
│                                                                             │
│  ┌─────────────────────────────┐    ┌─────────────────────────────────────┐│
│  │   Skill Evolution Service   │    │   RL Training Loop (Python)         ││
│  │   (Python / CAPO Engine)    │    │                                   ││
│  │                             │    │  ┌─────────┐  ┌─────────┐         ││
│  │  ┌─────────────────────┐   │    │  │  DAPO   │  │  PAPO   │         ││
│  │  │ CAPO Optimizer      │   │    │  │ (默认)  │  │(过程奖励)│         ││
│  │  │ - Population        │   │    │  └────┬────┘  └────┬────┘         ││
│  │  │ - Racing Selection  │   │    │       └────┬─────┘                ││
│  │  │ - Cross-over/Mutate │   │    │            │                      ││
│  │  └─────────────────────┘   │    │  ┌─────────▼─────────┐             ││
│  │           │                │    │  │ Atropos Framework │             ││
│  │  ┌────────▼────────┐       │    │  │ - Async Env       │             ││
│  │  │ Skill Registry  │       │    │  │ - Trajectory Std  │             ││
│  │  │ (SQLite + FTS5) │       │    │  │ - Rollout Coord   │             ││
│  │  │ - Genealogy Tree│       │    │  └─────────┬─────────┘             ││
│  │  │ - NFT Manifest  │       │    │            │                      ││
│  │  └─────────────────┘       │    └────────────┼──────────────────────┘│
│  └─────────────────────────────┘                 │                       │
│                                                  │                       │
│  ┌───────────────────────────────────────────────▼───────────────────────┐│
│  │                     Rust Burn Inference Layer                        ││
│  │                                                                      ││
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐               ││
│  │  │  WGPU Backend│  │ CUDA Backend │  │ CPU Backend  │               ││
│  │  │ (Edge/AMD)   │  │ (Cloud GPU)  │  │ (Fallback)   │               ││
│  │  └──────────────┘  └──────────────┘  └──────────────┘               ││
│  │                                                                      ││
│  │  ┌──────────────────────────────────────────────────────────────┐   ││
│  │  │  Model Zoo:                                                   │   ││
│  │  │  - Base LLM (推理主模型)                                     │   ││
│  │  │  - Policy Head (DAPO/PAPO 策略网络，轻量)                     │   ││
│  │  │  - Value Head (过程价值估计，配合 PAPO)                       │   ││
│  │  │  - Skill Embedder (技能嵌入编码器)                            │   ││
│  │  └──────────────────────────────────────────────────────────────┘   ││
│  └──────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
┌───────────────────────────────────▼─────────────────────────────────────────┐
│                      Layer 4: Blockchain (Monad)                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐  │
│  │  - DID Registry (身份锚定)                                              │  │
│  │  - Memory Attestation (记忆存证哈希)                                     │  │
│  │  - Skill NFT (技能资产化与交易)                                          │  │
│  │  - A2A Commercial Settlement (智能体间商业结算)                          │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.3 核心数据流

```
[用户交互] → [Burn 推理层] → [任务执行] → [Atropos 环境验证]
                │                                    │
                │ ① 实时反馈                         │ ② 轨迹 + 奖励
                ▼                                    ▼
        [Nudge Engine]                      [RL Training Loop]
        (热记忆压缩)                         (DAPO + PAPO 训练)
                │                                    │
                │ ③ 触发条件满足                      │ ④ 策略更新
                ▼                                    ▼
        [CAPO Skill Evolution]              [Burn 模型热加载]
        (SKILL.md / SOUL.md)                 (Policy/Value Head)
                │                                    │
                └──────────── ⑤ 链上存证 ────────────┘
```

---

## 3. 记忆系统进化设计

### 3.1 设计目标
- **分层持久化**：实现从毫秒级热记忆到永久链上记忆的多级存储。
- **主动 Nudge**：Agent 主动评估信息价值，而非被动等待用户指令。
- **质量约束**：单条记忆 ≤ 2200 字符，确保注入系统提示词时的信噪比。
- **情感-人格关联**：结合 PAD 情感模型与 OCEAN 人格模型，对记忆进行情感标签化与人格权重排序。

### 3.2 记忆分层架构

| 层级 | 存储介质 | 生命周期 | 触发写入 | 读取方式 | Rust 模块 |
|------|---------|---------|---------|---------|----------|
| **L0 热记忆** | 内存 (DashMap) | 单 Session | 每轮交互自动 | 零拷贝引用 | `memory::hot` |
| **L1 项目记忆** | SQLite + 内存缓存 | 项目存续期 | Nudge (10 turns) | FTS5 全文检索 | `memory::project` |
| **L2 用户画像** | SQLite + 本地文件 (USER.md) | 永久 | Nudge + 显式反馈 | 结构化查询 | `memory::profile` |
| **L3 灵魂定义** | 本地文件 (SOUL.md) + IPFS | 永久 | CAPO 进化 + 用户编辑 | 系统提示词注入 | `memory::soul` |
| **L4 链上存证** | Monad 链 + Arweave | 永久 | 关键记忆哈希锚定 | DID 授权查询 | `memory::chain` |

### 3.3 Nudge Engine 详细设计

#### 3.3.1 触发器类型

```rust
pub enum NudgeTrigger {
    /// 每 10 个用户回合触发一次记忆复盘
    MemoryReview { turn_threshold: u32 },
    /// 连续 5 次以上工具调用触发 Skill 审查
    SkillReview { call_threshold: u32 },
    /// 任务错误并成功恢复后触发经验提取
    RecoveryReview,
    /// 用户显式反馈（修正/表扬）触发即时学习
    ExplicitFeedback(FeedbackKind),
    /// 周期性 Cron 触发（如每日/每周复盘）
    CronReview(CronExpr),
    /// 心跳触发（低负载时后台进化）
    HeartbeatReview,
}
```

#### 3.3.2 Memory Nudge 执行流程
1. **计数器检查**：当前会话回合数达到 10，且 Agent 未在本轮主动调用记忆工具（若已调用则重置计数器，避免重复）。
2. **快照捕获**：冻结当前会话上下文（系统提示词会话内不变，保护前缀缓存）。
3. **价值评估**：后台 fork 的审查 Agent（轻量模型或 Burn 本地推理）评估对话历史，提取：
   - 用户偏好（如"偏好简洁输出"、"使用 Rust 而非 Python"）
   - 环境事实（如"项目使用 monorepo 结构"、"API 密钥存储在 ~/.config/"）
   - 踩坑记录（如"上次部署因镜像未 push 失败"）
4. **去重与合并**：通过向量相似度 + FTS 检索现有记忆，LLM 判断置信度：
   - ≥ 0.7：更新现有记忆（Patch 优先，局部修复）
   - < 0.3：创建新记忆
   - 中间值：人工（或用户）仲裁
5. **写入与通知**：写入 SQLite + 可选链上存证，向主 Agent 发送轻量通知（不阻塞主任务）。

#### 3.3.3 记忆质量评分体系
每条记忆在写入前需通过 0–10 分质量评分：
- **相关性** (0–3)：与当前项目/用户目标的关联度
- **可复用性** (0–3)：在未来任务中被引用的概率
- **准确性** (0–2)：信息来源可靠度（用户确认 > 推理得出 > 猜测）
- **简洁性** (0–2)：字符数效率（越短分越高，但需保留完整语义）

≥ 6 分写入 L1/L2，≥ 8 分触发链上存证建议。

### 3.4 记忆-情感-人格耦合

```rust
pub struct MemoryEntry {
    pub id: Uuid,
    pub content: String,          // ≤ 2200 chars
    pub layer: MemoryLayer,
    pub pad_tags: PADEmotion,     // Pleasure, Arousal, Dominance
    pub ocean_relevance: OCEANScore, // 五大人格维度关联权重
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub attestation_hash: Option<String>, // Monad 链上哈希
    pub genealogy_id: Option<Uuid>,       // 关联 Skill 谱系
}
```

- **PAD 情感标签**：在记忆写入时，由 Burn 推理层的轻量分类器（或规则引擎）标注情感极性。高 Arousal（唤醒度）记忆在检索时获得更高权重。
- **OCEAN 人格权重**：若用户画像显示高 Conscientiousness（尽责性），则"流程规范类"记忆排序靠前；高 Openness（开放性）则"创新尝试类"记忆优先。

---

## 4. 技能系统进化设计

### 4.1 设计目标
- **自动提炼**：从任务执行轨迹中自动识别高频/高价值操作模式，生成 SKILL.md。
- **CAPO 驱动**：使用 Cost-Aware Prompt Optimization 算法持续进化 Skill 的指令与 few-shot 示例。
- **渐进披露**：三级 Skill 读取策略，节省 Token。
- **谱系追踪**：记录 Skill 的父子关系、版本历史、NFT 化凭证，支持链上交易与 A2A 共享。

### 4.2 Skill 生命周期

```
[任务执行] → [触发条件检测] → [Nudge SkillReview]
                                    │
                                    ▼
                    [CAPO 初始生成 / 进化优化]
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
              [创建新 Skill]   [更新现有 Skill]   [合并/废弃 Skill]
                    │               │               │
                    └───────────────┴───────────────┘
                                    │
                    [质量评分 >= 6] → [写入 Skill Registry]
                                    │
                    [质量评分 >= 8] → [生成 Skill NFT 凭证]
                                    │
                    [链上存证] → [Monad 链锚定哈希]
```

### 4.3 自动触发条件

| 条件 | 阈值 | 说明 |
|------|------|------|
| **高频工具调用** | 同一任务内 >= 5 次工具调用 | 表明存在可固化的复杂流程 |
| **错误恢复** | 任务执行中遇到错误并成功恢复 | 踩坑经验具有高度复用价值 |
| **用户修正** | 用户显式纠正 Agent 行为 | 直接反馈，信噪比极高 |
| **跨任务复用** | 同一操作模式在 3 个不同任务中出现 | 泛化信号，触发通用 Skill 创建 |
| **Cron 复盘** | 每周自动扫描高频轨迹 | 兜底机制，防止遗漏 |

### 4.4 CAPO 驱动的 Skill 进化引擎

#### 4.4.1 搜索空间定义
每个 Skill 个体由以下基因型构成：
- **指令基因（Instruction Gene）**：任务描述、适用场景、前置条件、输出格式要求。
- **示例基因（Few-shot Gene）**：0–5 个典型执行案例（输入 → 思考过程 → 输出），数量由 CAPO 动态优化。
- **元数据基因（Meta Gene）**：版本号、父 Skill ID、适用权限层级、Burn 推理后端提示词。

#### 4.4.2 进化算子

```python
class CAPOSkillOptimizer:
    def crossover(self, parent_a: SkillGenome, parent_b: SkillGenome) -> SkillGenome:
        # 1. 指令交叉：Meta-LLM 读取两个父代指令，生成融合两者优势的子代指令。
        # 2. 示例并集：子代示例从父代示例池的并集中采样，数量取父代平均。
        pass

    def mutate(self, offspring: SkillGenome) -> SkillGenome:
        # 1. 指令变异：Meta-LLM 对指令进行语义改写（保持功能等价，优化表达）。
        # 2. 示例变异（三选一，等概率）：
        #    - Add：从轨迹库中挖掘新示例加入（不超过上限）。
        #    - Remove：随机删除一个示例（防止过拟合）。
        #    - Shuffle：随机打乱示例顺序（促进局部探索）。
        pass

    def racing_selection(self, population: List[SkillGenome]) -> List[SkillGenome]:
        # 不等待完整评估，逐步淘汰统计上已落后的个体。
        # Fitness = w1*成功率 + w2*Token节省率 + w3*用户满意度 - w4*API成本
        pass
```

#### 4.4.3 Fitness 函数设计

```rust
pub struct SkillFitness {
    /// 任务成功率（在 Atropos 模拟环境中验证）
    pub success_rate: f64,
    /// Token 节省率：使用 Skill 前后的平均 Token 消耗比
    pub token_efficiency: f64,
    /// 用户满意度：显式反馈 + 隐式信号（如是否手动修正）
    pub user_satisfaction: f64,
    /// 进化成本：本次 CAPO 迭代消耗的 API Token + GPU 时长
    pub evolution_cost: f64,
    /// 复用广度：跨项目/跨 Agent 的引用次数
    pub reuse_breadth: u32,
}

impl SkillFitness {
    pub fn score(&self) -> f64 {
        0.4 * self.success_rate 
        + 0.2 * self.token_efficiency 
        + 0.2 * self.user_satisfaction 
        + 0.1 * (1.0 / (1.0 + self.evolution_cost)) 
        + 0.1 * f64::min(self.reuse_breadth as f64 / 10.0, 1.0)
    }
}
```

#### 4.4.4 长度惩罚与预算控制
- **单条 Skill ≤ 2200 字符**：与记忆系统保持一致，避免系统提示词膨胀。
- **CAPO 种群规模**：默认 8–16 个个体，每代最多评估 4 个完整轨迹（Racing 提前淘汰）。
- **预算上限**：单次 Skill 进化消耗 ≤ $2（API 成本）或等值本地 GPU 时长。

### 4.5 渐进披露（Progressive Disclosure）

```rust
pub enum SkillDisclosureLevel {
    /// Level 0: 仅获取技能列表（~3K tokens）
    /// 用于快速判断是否存在相关技能
    List,
    /// Level 1: 获取完整 Skill 内容与元数据
    /// 在确认需要时调用
    Full { skill_id: Uuid },
    /// Level 2: 获取特定引用文件或深度参考材料
    /// 用于复杂任务的细粒度指导
    Deep { skill_id: Uuid, path: String },
}
```

- **调用策略**：Burn 推理层在生成工具调用计划时，先执行 `skills_list()`，仅当置信度 > 0.8 时执行 `skill_view()`。
- **缓存机制**：Level 1 内容在会话内缓存，避免重复读取；Level 2 按需加载。

### 4.6 技能谱系追踪（Skill Genealogy）

```rust
pub struct SkillNode {
    pub id: Uuid,
    pub name: String,
    pub version: SemVer,
    pub parent_ids: Vec<Uuid>,          // 支持多父代融合
    pub child_ids: Vec<Uuid>,
    pub created_by: DID,                // 创建者 DID（支持 A2A 溯源）
    pub evolution_method: EvolutionMethod, // CAPO / Manual / A2A-Import
    pub nft_manifest: Option<NFTManifest>, // 技能 NFT 化凭证
    pub attestation_hash: String,       // Monad 链上存证
    pub deprecated: bool,
    pub replacement_id: Option<Uuid>,
}
```

- **谱系树可视化**：通过 `crates/social-brain` 模块提供谱系查询接口，支持"查看此 Skill 的所有祖先/后代"。
- **A2A 共享**：高复用 Skill 可打包为 Skill NFT，通过 Monad 链进行 A2A 商业交易（买方支付 Token，卖方获得收益，链上自动分账）。
- **自动回滚**：若新版本的 Skill 在 3 次独立任务中成功率下降 > 20%，自动回滚到上一个稳定版本，并标记该变异为"有害突变"。

---

## 5. 强化学习训练闭环设计

### 5.1 设计目标
- **解决熵崩溃**：DAPO 的动态采样与解耦裁剪防止策略过早收敛到单一模式。
- **过程级奖励**：PAPO 的解耦优势归一化将工具调用验证器的反馈转化为过程奖励，消除奖励黑客。
- **异步可扩展**：Atropos 承担环境微服务化，支持分布式 Rollout 收集。
- **Burn 推理集成**：Rust 端执行策略网络的前向传播，Python 端负责梯度更新与轨迹协调。

### 5.2 Atropos 框架适配

#### 5.2.1 环境服务化
将 BeeBotOS 的各类验证场景封装为 Atropos 环境微服务：

| 环境服务 | 验证内容 | 奖励信号 | 过程奖励来源 |
|---------|---------|---------|-------------|
| `code_exec_env` | 代码执行结果（单元测试、类型检查） | 通过/失败 | 编译错误类型、测试覆盖率 |
| `chain_tx_env` | 链上交易模拟与回执验证 | 交易成功 | Gas 优化度、安全扫描结果 |
| `tool_call_env` | 工具调用参数合法性、执行结果 | 调用成功 | 参数完整性、返回格式合规性 |
| `a2a_negotiate_env` | 智能体间协商协议合规性 | 协议达成 | 协商轮次效率、条款公平性 |
| `webhook_env` | Webhook 响应合规性 | 200 OK + 格式正确 | 响应延迟、字段完整性 |

#### 5.2.2 轨迹标准化
所有环境统一输出 **Atropos Trajectory Format v2**：

```json
{
  "trajectory_id": "uuid",
  "agent_did": "did:monad:...",
  "session_context_hash": "sha256",
  "turns": [
    {
      "turn_id": 0,
      "observation": "用户请求 + 环境状态",
      "reasoning_chain": "Burn 推理层的思维链输出",
      "action": "工具调用 / 文本回复",
      "action_params": {},
      "process_reward": 0.3,
      "environment_feedback": "验证器详细输出",
      "burn_logits_snapshot": "base64_encoded_tensor",
      "burn_value_estimate": 0.5
    }
  ],
  "outcome_reward": 1.0,
  "total_turns": 6,
  "skill_ids_used": ["uuid-a", "uuid-b"],
  "memory_ids_accessed": ["uuid-c"],
  "timestamp": "2026-05-05T12:00:00Z"
}
```

### 5.3 DAPO 算法实现

#### 5.3.1 动态采样（Dynamic Sampling）
```python
class DAPOSampler:
    def filter_batch(self, trajectories: List[Trajectory]) -> List[Trajectory]:
        # 过滤掉模型已'掌握'的样本：
        # - 若某类任务连续 3 次成功率 > 95%，则从训练批次中降低其采样权重至 10%。
        # - 保留困难样本（成功率 < 60%）和边界样本（成功率 60-80%）。
        pass
```

#### 5.3.2 解耦裁剪（Decoupled Clip）
```python
# 标准 GRPO: ratio_clip = 0.2 (对称)
# DAPO: 非对称裁剪，鼓励探索
policy_loss = -min(
    ratio * advantage,
    torch.clamp(ratio, 1.0 - eps_low, 1.0 + eps_high) * advantage
)
# 其中 eps_low = 0.1 (收缩保守侧), eps_high = 0.3 (放宽探索侧)
```

#### 5.3.3 与 Burn 的交互
- **Burn 端**：提供 `PolicyHead` 和 `ValueHead` 的推理接口，输出 logits 和 value estimate。
- **Python 端**：接收 Burn 的 logits snapshot，计算 ratio 和 advantage，执行梯度更新。
- **通信协议**：gRPC streaming，支持批量传输轨迹和增量权重同步。

### 5.4 PAPO 算法实现

#### 5.4.1 过程奖励模型（PRM）接入
PAPO 的核心是将 Atropos 环境验证器的反馈转化为**过程级优势**，避免 GRPO 对所有正确响应分配相同优势导致的奖励黑客。

```python
class PAPOTrainer:
    def compute_process_advantage(self, trajectory: Trajectory) -> List[float]:
        # 1. 结果奖励 R_outcome in {0, 1}
        # 2. 过程奖励 R_process[t] in [-1, 1]，来自：
        #    - code_exec_env: 编译错误类型权重、测试通过数
        #    - chain_tx_env: 安全扫描分数、Gas 效率
        #    - tool_call_env: 参数 schema 匹配度、返回字段完整性
        # 3. 解耦归一化：
        #    - 结果优势 A_outcome = (R_outcome - mu_outcome) / sigma_outcome
        #    - 过程优势 A_process[t] = (R_process[t] - mu_process) / sigma_process
        #    - 总优势 A_total = alpha * A_outcome + (1-alpha) * A_process[t]
        pass
```

#### 5.4.2 解决奖励黑客
- **冗长生成长惩罚**：若过程奖励显示 Agent 在"重复思考/冗余验证"上消耗过多 Token，PAPO 的过程优势会自然抑制该行为（过程奖励中的效率维度为负）。
- **猜对 vs 推导区分**：PAPO 通过过程奖励区分"蒙对答案"和"严格推导"——前者过程奖励低（中间步骤混乱），后者过程奖励高（逻辑连贯），即使两者结果奖励相同。

### 5.5 训练闭环时序

```
T0: Agent 执行任务 -> Burn 推理层生成动作
T1: Atropos 环境验证 -> 返回过程奖励 + 结果奖励
T2: 轨迹写入 SQLite + 异步上传至 RL 训练队列
T3: DAPO 采样器过滤已掌握样本，构造训练批次
T4: PAPO 计算过程优势，生成梯度更新信号
T5: Python 端更新 Policy/Value Head 权重
T6: 增量权重 diff 通过 gRPC 同步至 Burn 推理层
T7: Burn 热加载新权重（无需重启服务）
T8: 关键事件（策略更新、高价值轨迹）锚定到 Monad 链
```

---

## 6. Rust Burn 推理层设计

### 6.1 设计目标
- **类型安全**：利用 Rust 类型系统在编译期捕获张量维度错误。
- **后端无关**：同一套模型代码可在 WGPU（边缘/AMD）、CUDA（云端）、CPU（降级）上运行。
- **低延迟推理**：本地执行，避免网络延迟，支持心跳级实时响应。
- **与 Python RL 训练协同**：Burn 负责前向传播，Python 负责反向传播与权重更新。

### 6.2 模块架构

```
crates/
└── inference/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs
    │   ├── backend/          # 后端抽象与初始化
    │   │   ├── mod.rs
    │   │   ├── wgpu.rs       # WebGPU / WGPU 后端（跨平台 GPU）
    │   │   ├── cuda.rs       # CUDA 后端（NVIDIA GPU）
    │   │   └── cpu.rs        # NdArray 回退后端
    │   ├── model/            # 模型定义
    │   │   ├── mod.rs
    │   │   ├── base_llm.rs   # 基础 LLM 推理（加载量化权重）
    │   │   ├── policy_head.rs # DAPO/PAPO 策略头（轻量，~100M 参数）
    │   │   ├── value_head.rs  # 价值估计头（过程价值 + 结果价值）
    │   │   └── skill_embedder.rs # Skill 嵌入编码器（用于检索与谱系）
    │   ├── serving/          # 服务化接口
    │   │   ├── mod.rs
    │   │   ├── grpc_server.rs # gRPC 服务（与 Python RL 端通信）
    │   │   └── local_api.rs   # 本地 HTTP API（与 Social Brain 通信）
    │   └── quantization/     # 量化支持
    │       ├── mod.rs
    │       └── gguf_loader.rs # GGUF 格式加载（兼容 llama.cpp 生态）
```

### 6.3 核心接口定义

```rust
/// Burn 推理后端 trait，抽象不同硬件后端
pub trait InferenceBackend {
    type Device: Backend;

    fn init(config: BackendConfig) -> Result<Self, BurnError>;
    fn load_model(&self, path: &str) -> Result<ModelHandle, BurnError>;
    fn infer(&self, handle: &ModelHandle, input: Tensor) -> Result<Tensor, BurnError>;
}

/// 策略网络输出
pub struct PolicyOutput {
    pub logits: Tensor,           // 动作空间 logits
    pub value_process: f64,       // PAPO 过程价值估计
    pub value_outcome: f64,       // 结果价值估计
    pub entropy: f64,             // 策略熵（用于 DAPO 监控）
}

/// Skill 嵌入输出
pub struct SkillEmbedding {
    pub embedding: Vec<f32>,      // 768-dim 或 1024-dim
    pub skill_id: Uuid,
    pub similarity_score: f64,    // 与当前任务的余弦相似度
}
```

### 6.4 与 Python RL 端的通信协议

| 方向 | 协议 | 内容 | 频率 |
|------|------|------|------|
| Rust -> Python | gRPC streaming | 轨迹（obs, action, logits, value） | 每 turn |
| Python -> Rust | gRPC unary | 增量权重 diff（LoRA 或全量） | 每训练步 / 批量同步 |
| Rust -> Python | gRPC unary | 心跳状态（负载、缓存命中率） | 每 30s |
| Python -> Rust | HTTP POST | 紧急策略回滚指令 | 按需 |

### 6.5 热加载与版本控制
- **权重版本化**：每次策略更新生成版本号（如 `policy-v1.2.3`），Burn 端同时保留最新 2 个版本。
- **原子切换**：新权重加载至 shadow 内存，验证通过（连续 3 次推理无 panic）后原子指针切换。
- **自动回滚**：若新版本在 60s 内错误率 > 阈值，自动切回旧版本并告警。

---

## 7. 数据流与交互流程

### 7.1 典型交互：用户请求 -> 任务执行 -> 进化沉淀

```
用户 -> Gateway: 发送任务请求
Gateway -> SocialBrain: 路由至活跃 Agent
SocialBrain -> Burn: 加载用户画像 + 相关 Skill
Burn -> Burn: 本地推理生成动作计划
Burn -> AtroposEnv: 执行工具调用（如代码执行）
AtroposEnv -> AtroposEnv: 验证结果，生成过程奖励
AtroposEnv --> Burn: 返回观察 + 奖励
Burn --> SocialBrain: 返回执行结果
SocialBrain --> Gateway: 返回给用户

Note over SocialBrain: Nudge Engine 检测
SocialBrain -> SocialBrain: 回合数+1，检查触发条件

alt 触发 Memory Nudge (10 turns)
    SocialBrain -> MemoryService: 复盘对话，提取记忆
    MemoryService -> SQLite: 写入 L1/L2 记忆
    MemoryService -> MonadChain: 锚定哈希（高价值记忆）
end

alt 触发 Skill Nudge (5+ tool calls)
    SocialBrain -> CAPOEngine: 提交执行轨迹
    CAPOEngine -> CAPOEngine: 种群进化（交叉/变异/Racing）
    CAPOEngine -> SkillRegistry: 写入/更新 SKILL.md
    SkillRegistry -> SQLite: 更新 Skill 谱系树
    SkillRegistry -> MonadChain: 生成/更新 Skill NFT
end

alt 触发 RL Training
    AtroposEnv -> RLQueue: 提交标准化轨迹
    RLQueue -> DAPOTrainer: 动态采样构造批次
    DAPOTrainer -> PAPOTrainer: 计算过程优势
    PAPOTrainer -> PythonBackend: 梯度更新 Policy/Value
    PythonBackend -> Burn: 同步增量权重
    Burn -> Burn: 热加载新策略
end
```

### 7.2 跨智能体（A2A）进化共享

```
AgentA -> AgentB: A2A 协商请求（携带 DID + Skill NFT 凭证）
AgentB -> MonadChain: 验证 Skill NFT 所有权与完整性
MonadChain --> AgentB: 验证通过
AgentB -> AgentA: 授权访问特定 Skill
AgentA -> SkillRegistry: 导入 Skill（标记为 A2A-Import 来源）
AgentA -> CAPOEngine: 本地化适配进化（保留父代引用）
CAPOEngine -> SkillRegistry: 写入本地化子代 Skill
SkillRegistry -> MonadChain: 记录谱系分支 + 商业结算
```

---

## 8. 安全、成本与治理约束

### 8.1 安全机制

| 层级 | 机制 | 实现 |
|------|------|------|
| **输入安全** | Prompt 注入扫描 | 所有进化产物（记忆、Skill）进入系统提示词前，通过规则引擎 + 轻量模型扫描恶意指令 |
| **执行安全** | 沙箱隔离 | 代码执行环境（Atropos `code_exec_env`）运行在 Firecracker MicroVM / gVisor 中 |
| **链上安全** | 存证不可篡改 | 记忆哈希与 Skill NFT 锚定至 Monad 链，任何篡改可通过哈希校验发现 |
| **策略安全** | 自动回滚 | 新策略版本在 shadow 模式验证通过后才上线，错误率超阈值自动回滚 |
| **权限安全** | 10 层权限栈 | 进化操作需匹配当前权限层级（如 L7 以上才能修改 SOUL.md，L9 以上才能发布 Skill NFT） |

### 8.2 成本控制

```rust
pub struct EvolutionBudget {
    /// 单次 CAPO 迭代的最大 API 调用成本（USD）
    pub capo_max_cost: f64,          // default: 2.0
    /// 单次 CAPO 迭代的最大 LLM 调用次数
    pub capo_max_calls: u32,         // default: 20
    /// 每日 RL 训练的最大 GPU 时长（小时）
    pub rl_daily_gpu_hours: f64,     // default: 4.0
    /// 单条记忆/Skill 的最大字符数
    pub max_content_length: usize,   // default: 2200
    /// 系统提示词总长度软上限（Token）
    pub system_prompt_token_limit: usize, // default: 8192
    /// 上下文压缩目标（降至窗口的 50% 以下）
    pub context_compression_target: f64, // default: 0.5
}
```

- **Token 估算**：采用 4 字符 ≈ 1 Token 的粗略估算，在 Rust 端实时计算。
- **前缀缓存保护**：系统提示词在会话内冻结，避免每轮 API 重新计费（针对云端 fallback 场景）。
- **后台 fork 隔离**：进化过程在独立进程/容器中运行，资源占用不影响主 Agent 的 SLA。

### 8.3 治理与审计
- **进化日志**：所有 Nudge 触发、CAPO 迭代、RL 权重更新写入不可篡改的审计日志（WAL + 链上锚定）。
- **人工仲裁接口**：对于置信度中间值（0.3–0.7）的记忆/Skill 变更，提供用户仲裁 UI（Web/CLI）。
- **遗忘权**：用户可随时请求删除特定记忆或 Skill，系统执行级联删除并更新链上状态为"已撤销"。

---

## 9. 实施路线图

### Phase 1: 基础设施（Week 1–4）
- [ ] 搭建 Burn 推理层（WGPU/CUDA 后端初始化，基础 LLM 加载）
- [ ] 实现记忆分层存储（SQLite + FTS5，L0–L2）
- [ ] 实现 Nudge Engine 核心触发器（Memory Nudge, Skill Nudge）
- [ ] Atropos 框架集成（环境服务化，轨迹标准化）

### Phase 2: 进化引擎（Week 5–8）
- [ ] CAPO 算法实现（种群管理、Racing Selection、Fitness 评估）
- [ ] Skill Registry 与谱系追踪系统（SQLite 关系模型 + 图查询）
- [ ] 渐进披露接口（Level 0/1/2）
- [ ] DAPO 算法接入 Atropos（动态采样、解耦裁剪）

### Phase 3: 过程奖励与强化（Week 9–12）
- [ ] PAPO 算法实现（过程奖励模型接入、解耦优势归一化）
- [ ] 工具调用验证器环境（code_exec_env, chain_tx_env, tool_call_env）
- [ ] Burn Policy/Value Head 训练与热加载管线
- [ ] Rust <-> Python gRPC 权重同步协议

### Phase 4: 链上与生态（Week 13–16）
- [ ] Monad 链记忆存证合约（Solidity）
- [ ] Skill NFT 合约（铸造、转移、A2A 商业结算）
- [ ] DID 与权限栈的链上验证
- [ ] A2A 进化共享协议（跨 Agent Skill 导入与谱系记录）

### Phase 5: 优化与治理（Week 17–20）
- [ ] 成本监控仪表盘（CAPO 成本、RL GPU 时长、Token 消耗）
- [ ] 安全扫描自动化（Prompt 注入、策略回滚测试）
- [ ] 用户仲裁界面
- [ ] 性能基准测试（对比 GEPA+GRPO 基线）

---

## 10. 附录

### A. 参考实现与依赖

| 组件 | 技术栈 | 版本约束 | 说明 |
|------|--------|---------|------|
| Burn | Rust | >= 0.16 | 深度学习推理与轻量训练 |
| Atropos | Python | >= 0.4 | 异步 RL 环境框架 |
| DAPO/PAPO | Python (PyTorch) | >= 2.2 | RL 算法实现 |
| CAPO | Python | 自定义 | Prompt 进化引擎 |
| SQLite | C/Rust (rusqlite) | >= 0.32 | 记忆与 Skill 持久化 |
| FTS5 | SQLite 内置 | — | 全文检索 |
| gRPC | tonic (Rust) / grpcio (Python) | >= 0.12 | 跨语言通信 |
| Monad SDK | Rust / Solidity | 最新测试网 | 链上存证与结算 |

### B. 与 BeeBotOS 现有模块的对接点

| 现有模块 | 对接方式 | 进化系统消费/产出 |
|---------|---------|------------------|
| `crates/core` | 引入 `crates/inference` | 产出：策略权重、Skill 嵌入 |
| `crates/social-brain` | 调用 Nudge Engine API | 消费：记忆查询；产出：新记忆、情感标签 |
| `crates/agents` | 通过 A2A 协议共享 Skill | 消费：Skill 列表；产出：执行轨迹 |
| `crates/chain` | 链上存证接口 | 消费：记忆哈希、Skill NFT；产出：存证凭证 |
| `contracts/` | Solidity 合约升级 | 新增：MemoryAttestation, SkillNFT, A2ASettlement |
| `skills/` | 目录结构由 Skill Registry 管理 | 产出：自动生成的 SKILL.md / SOUL.md |

### C. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| CAPO 进化成本超支 | 预算失控 | Racing 机制 + 硬预算上限（$2/次） |
| DAPO 动态采样偏差 | 遗忘已掌握技能 | 保留 10% 已掌握样本作为复习 |
| PAPO 过程奖励设计错误 | 奖励黑客 | 多验证器交叉验证 + 人工审计样本 |
| Burn 训练生态不成熟 | 无法大规模 RL | Python 端承担训练，Burn 仅负责推理 |
| Skill 谱系膨胀 | 检索效率下降 | 自动归档低频 Skill，谱系树定期剪枝 |
| 链上存证延迟 | 用户体验受损 | 异步锚定，本地先确认，链上最终一致性 |

---

**文档结束**

*本文档为 BeeBotOS 自主学习进化机制的技术需求基线，后续迭代需根据 Phase 1 原型反馈进行修订。*
