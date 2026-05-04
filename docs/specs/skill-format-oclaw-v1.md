
OpenClaw 的 Skills 是标准化的能力扩展单元，其定义规则以 SKILL.md 为核心，通过 YAML 元数据 + Markdown 执行逻辑 描述任务触发条件、执行流程与安全边界。所有 Skills 必须遵循目录化结构、精准触发描述、权限最小化三大原则，否则无法被智能体正确识别或执行。以下为基于 2026 年 4 月最新版 OpenClaw 2.3+ 的官方定义规则：

一、核心文件结构规范
强制目录结构
每个 Skill 必须以 独立文件夹 形式存在，SKILL.md 为唯一必选文件，其他为可选：
skill-name/                  # 目录名需小写字母+连字符（≤64字符）
├── SKILL.md                 # 必需：YAML 元数据 + 执行逻辑
├── scripts/                 # 可选：可执行脚本（Python/Bash/TS）
├── references/              # 可选：参考文档（API 规范/示例）
└── assets/                  # 可选：资源文件（模板/图标）

目录名与 name 字段必须一致，否则智能体无法加载。
SKILL.md 位置不可变更，必须位于 Skill 根目录。

SKILL.md 格式要求
（1）YAML Frontmatter（必填字段）

name: "唯一技能标识"          # 强制：小写字母+连字符（如 file-report-skill）
description: "触发条件说明"    # 强制：明确何时调用（非功能描述！）
version: "1.0.0"             # 推荐：语义化版本
author: "开发者"              # 可选
permissions: []              # 强制：声明所需权限（见下文）
triggers: []                 # 强制：触发条件（keywords/intent）

description 是触发关键：必须写 具体场景（如“当用户要求合并 PDF 时触发”），而非“处理 PDF” 等模糊表述。
permissions 必须最小化：仅申请必要权限（如 file.read 而非 file.write）。

（2）Markdown 正文（核心逻辑）
需包含 3 个强制部分：
工作流程：分步骤说明执行逻辑（如“1. 获取文件路径 → 2. 验证格式 → 3. 生成报告”）。
输入/输出规范：明确参数类型、格式及返回结构。
异常处理：定义错误场景与恢复策略（如“路径不存在时返回错误码 404”）。

二、触发条件定义规则
精准触发声明
必须在 YAML 或正文中明确 触发条件，支持两种方式：
triggers:
  keywords: ["合并", "pdf"]       # 匹配用户输入关键词
  intent: "merge_pdf"            # 匹配语义意图（需 Skill 自定义）

禁止模糊描述：如 description: "处理文件" 应改为“当用户要求合并、拆分或旋转 PDF 时触发”。
多条件优先级：keywords 优先于 intent，冲突时按 YAML 顺序匹配。

环境门控（Gating）
通过 gates 字段声明 运行时依赖，不满足则 Skill 不可见：
gates:
  os: ["linux", "darwin"]          # 支持的操作系统
  binary: "chromium"               # 依赖的二进制工具
  env: "BROWSER_API_KEY"           # 必需的环境变量

智能体在加载时自动检查门控条件，缺失依赖的 Skill 不会进入能力列表。

三、执行逻辑规范
执行方式分类
类型               适用场景                       定义方式
纯语言 Skill    文本处理/内容生成             直接在 SKILL.md 描述逻辑

工具型 Skill    调用 API/二进制工具           声明 execution.type: shell

代码执行型      复杂计算/自动化流程           提供 scripts/ 下的可执行脚本

关键规则
脚本必须独立执行：  
  scripts/ 中的代码 不可直接加载到上下文，需通过 SKILL.md 声明调用方式（如 scripts/merge_pdf.py）。
禁止硬编码敏感信息：  
  API 密钥等必须通过 env 变量注入，不得写入 SKILL.md 或脚本。
输入/输出需结构化：  
  明确定义参数类型（如 city: string）和返回格式（如 result: string）。

四、安全与权限规则
权限声明强制项
permissions:
  filesystem: read            # 文件系统权限（read/write/none）
  network: true             # 是否需要联网
  env_vars: ["API_KEY"]     # 所需环境变量

未声明的权限一律拒绝：如 Skill 未声明 network: true，则无法发起 HTTP 请求。
高风险操作需二次确认：涉及数据删除、外部写入等操作时，必须要求用户手动确认。

安全红线
禁止 curl | bash 类指令：安装依赖必须显式列出命令，不可隐藏执行逻辑。
第三方 Skill 需沙箱运行：来自 ClawHub 的 Skill 默认视为不可信代码，需在隔离环境中执行。

五、开发与验证最佳实践
开发原则
只写 AI 不知道的内容：  
  避免重复基础常识（如“Python 用 def 定义函数”），聚焦 实战经验（如“中文 PDF 用 pdfplumber 避免乱码”）。
保持精简（≤500 行）：  
  详细文档放入 references/，SKILL.md 仅保留核心流程。

验证命令
语法校验（不执行）
openclaw skill validate ./skill-name

模拟触发测试
openclaw skill test "帮我合并两个PDF" --skill ./skill-name

验证失败时会提示 具体错误位置（如 triggers.keywords: 缺少必要关键词）。

六、加载优先级与冲突解决
加载顺序（高→低）
工作区 Skill：/skills/（项目级，优先级最高）。
本地托管 Skill：~/.openclaw/skills/（用户级）。
内置 Skill：OpenClaw 安装包自带（优先级最低）。

冲突处理
同名 Skill 按优先级覆盖：工作区 Skill 会覆盖本地/内置版本。
禁用 Skill：在 openclaw.json 中设置 "enabled": false 即可全局停用。

💡 关键提示：Skills 的 核心价值在于将经验沉淀为可复用 SOP，而非简单封装工具。定义时需聚焦 “如何稳定完成任务”，而非仅描述功能。生产环境建议通过 clawhub publish 发布 Skill，并附带清晰的 references/examples.md 供用户验证。



