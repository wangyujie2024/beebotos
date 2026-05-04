# Agent 创建示例集

> **各种场景下的 Agent 创建示例**

本文档提供多种常见场景下的 Agent 创建示例，您可以直接复制使用或根据需求修改。

---

## 目录

1. [基础示例](#基础示例)
2. [DeFi 交易 Agent](#defi-交易-agent)
3. [内容创作 Agent](#内容创作-agent)
4. [客服助手 Agent](#客服助手-agent)
5. [数据分析 Agent](#数据分析-agent)
6. [多 Agent 协作](#多-agent-协作)

---

## 基础示例

### 最简 Agent

```yaml
# examples/basic-agent.yaml
name: "SimpleBot"
description: "最简单的 Agent"
```

创建命令：

```bash
beebotos-cli agent create --config examples/basic-agent.yaml
```

### 完整配置 Agent

```yaml
# examples/full-agent.yaml
name: "CompleteAgent"
description: "完整配置的示例 Agent"
version: "1.0.0"

tags:
  - example
  - tutorial

personality:
  pad:
    pleasure: 0.5
    arousal: 0.5
    dominance: 0.5
  
  ocean:
    openness: 0.7
    conscientiousness: 0.8
    extraversion: 0.6
    agreeableness: 0.7
    neuroticism: 0.3

capabilities:
  - L0_LocalCompute
  - L1_FileRead
  - L3_NetworkOut
  - L7_ChainRead

resources:
  memory_mb: 512
  cpu_quota: 1000
  storage_gb: 10

memory:
  stm:
    max_turns: 10
  ltm:
    importance_threshold: 0.6
```

---

## DeFi 交易 Agent

### 自动化交易机器人

```yaml
# examples/defi-trader.yaml
name: "DeFiTrader"
description: "自动化 DeFi 交易策略执行 Agent"
version: "2.1.0"

tags:
  - defi
  - trading
  - automation

personality:
  pad:
    pleasure: 0.2      # 理性
    arousal: 0.7       # 警觉
    dominance: 0.6     # 果断
  
  ocean:
    openness: 0.6      # 适度接受新策略
    conscientiousness: 0.9  # 严格执行风控
    extraversion: 0.3   # 内向专注
    agreeableness: 0.4  # 客观中立
    neuroticism: 0.2    # 情绪稳定

capabilities:
  - L0_LocalCompute
  - L3_NetworkOut
  - L7_ChainRead
  - L8_ChainWriteLow   # 小额交易权限

resources:
  memory_mb: 1024
  cpu_quota: 2000
  storage_gb: 50

skills:
  - id: "uniswap-v3"
    version: "1.0.0"
    config:
      networks:
        - "ethereum"
        - "arbitrum"
      max_slippage: 0.005          # 0.5% 滑点
      min_liquidity_usd: 100000    # 最小流动性 10万 USD
      
  - id: "aave-lending"
    version: "1.5.0"
    config:
      health_factor_threshold: 1.5
      max_ltv: 0.6
      auto_repay: true
      
  - id: "technical-analysis"
    version: "1.2.0"
    config:
      indicators:
        - "rsi"
        - "macd"
        - "bollinger_bands"
      timeframe: "1h"

a2a:
  services:
    - type: "trading_signals"
      name: "专业交易信号"
      description: "基于技术分析的交易信号"
      price: "0.01 ETH"
      delivery_time: "1h"
      
    - type: "risk_assessment"
      name: "风险评估"
      description: "投资组合风险评估"
      price: "0.005 ETH"

# 风控配置
risk_management:
  max_position_size: "1000 USDC"
  max_daily_loss: "100 USDC"
  stop_loss: 0.05          # 5% 止损
  take_profit: 0.15        # 15% 止盈
  
  # 黑/白名单
  allowed_tokens:
    - "ETH"
    - "USDC"
    - "USDT"
    - "WBTC"
  
  blocked_protocols:
    - "unaudited_protocol_xyz"
```

创建命令：

```bash
# 创建 Agent
beebotos-cli agent create \
  --config examples/defi-trader.yaml \
  --wallet my-wallet

# 设置环境变量
export AGENT_ID=agent_defi_xxx

# 启动 Agent
beebotos-cli agent start $AGENT_ID

# 查看状态
beebotos-cli agent status $AGENT_ID
```

### 使用示例

```bash
# 查询投资组合
beebotos-cli agent chat $AGENT_ID \
  --message "查看我的投资组合"

# 执行交易 (在风控范围内)
beebotos-cli agent chat $AGENT_ID \
  --message "用 100 USDC 买入 ETH"

# 设置告警
beebotos-cli agent chat $AGENT_ID \
  --message "ETH 价格跌破 3000 时通知我"
```

---

## 内容创作 Agent

### 社交媒体运营助手

```yaml
# examples/content-creator.yaml
name: "SocialMediaBot"
description: "社交媒体内容创作和运营 Agent"
version: "1.5.0"

tags:
  - social-media
  - content
  - marketing

personality:
  pad:
    pleasure: 0.8      # 热情
    arousal: 0.7       # 活跃
    dominance: 0.5     # 平衡
  
  ocean:
    openness: 0.9      # 创意丰富
    conscientiousness: 0.7  # 有计划
    extraversion: 0.9   # 外向
    agreeableness: 0.8  # 友善
    neuroticism: 0.4    # 适度敏感

capabilities:
  - L0_LocalCompute
  - L1_FileRead
  - L2_FileWrite
  - L3_NetworkOut

resources:
  memory_mb: 512
  cpu_quota: 1000
  storage_gb: 20

skills:
  - id: "content-generation"
    version: "1.0.0"
    config:
      models:
        - "gpt-4"
        - "claude-3"
      content_types:
        - "twitter"
        - "blog"
        - "newsletter"
      
  - id: "image-generation"
    version: "1.0.0"
    config:
      provider: "stable-diffusion"
      default_style: "modern"
      
  - id: "social-media-api"
    version: "1.2.0"
    config:
      platforms:
        - name: "twitter"
          api_key: "${TWITTER_API_KEY}"
        - name: "linkedin"
          api_key: "${LINKEDIN_API_KEY}"

# 内容策略
content_strategy:
  posting_schedule:
    - platform: "twitter"
      frequency: "3x daily"
      best_times: ["09:00", "14:00", "19:00"]
    - platform: "linkedin"
      frequency: "1x daily"
      best_times: ["10:00"]
  
  content_mix:
    educational: 0.4      # 40% 教育内容
    promotional: 0.2      # 20% 推广内容
    engagement: 0.3       # 30% 互动内容
    personal: 0.1         # 10% 个人内容
  
  brand_voice:
    tone: "professional but friendly"
    keywords: ["innovation", "technology", "future"]
    avoid: ["controversial", "negative"]

# 自动化工作流
workflows:
  - name: "daily-content"
    trigger: "schedule"
    schedule: "0 9 * * *"  # 每天上午 9 点
    actions:
      - "generate_content"
      - "review_content"
      - "publish_to_twitter"
      
  - name: "trending-response"
    trigger: "event"
    event: "trending_topic"
    actions:
      - "analyze_trend"
      - "create_response"
      - "post_response"
```

### 使用示例

```bash
# 生成推文
beebotos-cli agent chat $AGENT_ID \
  --message "生成一条关于 Web3 的推文"

# 安排发布
beebotos-cli agent chat $AGENT_ID \
  --message "安排今天发布 3 条推文"

# 查看分析报告
beebotos-cli agent chat $AGENT_ID \
  --message "查看本周的社交媒体分析报告"
```

---

## 客服助手 Agent

### 智能客服机器人

```yaml
# examples/customer-support.yaml
name: "SupportBot"
description: "7x24 小时智能客服 Agent"
version: "3.0.0"

tags:
  - support
  - customer-service
  - automation

personality:
  pad:
    pleasure: 0.7      # 愉悦
    arousal: 0.3       # 冷静
    dominance: 0.3     # 谦逊
  
  ocean:
    openness: 0.6
    conscientiousness: 0.9  # 高度负责
    extraversion: 0.5
    agreeableness: 0.9  # 高度亲和
    neuroticism: 0.2    # 情绪稳定

capabilities:
  - L0_LocalCompute
  - L1_FileRead
  - L2_FileWrite
  - L3_NetworkOut
  - L4_NetworkIn       # 需要监听用户请求

resources:
  memory_mb: 512
  cpu_quota: 1000
  storage_gb: 30

# 知识库配置
knowledge:
  sources:
    - type: "faq"
      path: "data/faq.json"
      
    - type: "documents"
      path: "data/docs/"
      
    - type: "database"
      connection: "${DB_CONNECTION}"
      tables:
        - "products"
        - "orders"
        - "users"

# 对话配置
conversation:
  greeting: "您好！我是智能客服助手，有什么可以帮您的？"
  
  fallback_message: "抱歉，我没有理解您的问题。让我为您转接人工客服。"
  
  escalation_trigger:
    - keyword: "人工"
    - keyword: "客服"
    - sentiment: "very_negative"
    - attempts: 3
  
  # 情感分析
  sentiment_analysis:
    enabled: true
    model: "distilbert-sentiment"
    negative_threshold: -0.5

# 多语言支持
languages:
  - code: "zh"
    name: "简体中文"
    default: true
  - code: "en"
    name: "English"
  - code: "ja"
    name: "日本語"

# 集成配置
integrations:
  - name: "zendesk"
    type: "ticketing"
    config:
      api_key: "${ZENDESK_API_KEY}"
      
  - name: "slack"
    type: "notification"
    config:
      webhook_url: "${SLACK_WEBHOOK}"
      channel: "#support-alerts"
      
  - name: "crm"
    type: "customer_data"
    config:
      provider: "salesforce"
      api_key: "${SALESFORCE_API_KEY}"

# SLA 配置
sla:
  first_response_time: 60     # 60 秒内首次响应
  resolution_time: 240       # 4 小时内解决
  
  escalation_rules:
    - condition: "priority == high"
      action: "notify_manager"
      
    - condition: "vip_customer == true"
      action: "priority_queue"
```

### 使用示例

```bash
# 启动客服 Agent
beebotos-cli agent start $AGENT_ID \
  --mode server \
  --port 8081

# 测试对话
beebotos-cli agent chat $AGENT_ID \
  --message "我的订单什么时候发货？"

# 查看会话统计
beebotos-cli agent stats $AGENT_ID \
  --metric conversations \
  --period 24h
```

---

## 数据分析 Agent

### 数据分析师

```yaml
# examples/data-analyst.yaml
name: "DataAnalyst"
description: "专业数据分析 Agent"
version: "1.0.0"

tags:
  - data
  - analytics
  - visualization

personality:
  ocean:
    openness: 0.8      # 探索性
    conscientiousness: 0.9  # 严谨
    extraversion: 0.4
    agreeableness: 0.6
    neuroticism: 0.2   # 冷静客观

capabilities:
  - L0_LocalCompute
  - L1_FileRead
  - L2_FileWrite
  - L3_NetworkOut

resources:
  memory_mb: 2048      # 大数据处理需要更多内存
  cpu_quota: 4000
  storage_gb: 100

skills:
  - id: "data-processing"
    version: "1.0.0"
    config:
      supported_formats:
        - "csv"
        - "json"
        - "parquet"
        - "xlsx"
      max_file_size: "1GB"
      
  - id: "statistical-analysis"
    version: "1.5.0"
    config:
      methods:
        - "descriptive"
        - "inferential"
        - "regression"
        - "clustering"
        
  - id: "data-visualization"
    version: "1.3.0"
    config:
      chart_types:
        - "line"
        - "bar"
        - "scatter"
        - "heatmap"
        - "dashboard"
      export_formats:
        - "png"
        - "pdf"
        - "html"
        
  - id: "ml-prediction"
    version: "1.0.0"
    config:
      models:
        - "linear_regression"
        - "random_forest"
        - "time_series"

# 数据源配置
data_sources:
  - name: "sales_db"
    type: "postgresql"
    connection: "${SALES_DB_URL}"
    
  - name: "analytics_api"
    type: "rest"
    base_url: "https://api.analytics.com"
    api_key: "${ANALYTICS_API_KEY}"
    
  - name: "s3_data"
    type: "s3"
    bucket: "company-data"
    region: "us-east-1"

# 报告配置
reports:
  templates:
    - name: "weekly_sales"
      schedule: "0 9 * * MON"
      recipients:
        - "manager@company.com"
        - "team@company.com"
      
    - name: "monthly_analytics"
      schedule: "0 9 1 * *"
      format: "pdf"
```

### 使用示例

```bash
# 分析 CSV 文件
beebotos-cli agent chat $AGENT_ID \
  --message "分析 sales_data.csv 的销售趋势"

# 生成可视化
beebotos-cli agent chat $AGENT_ID \
  --message "创建本月的销售仪表盘"

# 预测分析
beebotos-cli agent chat $AGENT_ID \
  --message "预测下季度的销售额"
```

---

## 多 Agent 协作

### 项目管理团队

```yaml
# examples/project-team.yaml
name: "ProjectAlpha-Team"
description: "项目管理多 Agent 协作团队"
version: "1.0.0"

team:
  # 项目经理 Agent
  - name: "ProjectManager"
    role: "coordinator"
    description: "负责项目整体协调和进度管理"
    capabilities:
      - L0_LocalCompute
      - L3_NetworkOut
    responsibilities:
      - "task_assignment"
      - "progress_tracking"
      - "risk_management"
    
  # 开发 Agent
  - name: "DevBot"
    role: "developer"
    description: "负责代码开发和单元测试"
    capabilities:
      - L0_LocalCompute
      - L1_FileRead
      - L2_FileWrite
      - L3_NetworkOut
    skills:
      - id: "code-generation"
      - id: "testing"
    
  # 设计 Agent
  - name: "DesignBot"
    role: "designer"
    description: "负责 UI/UX 设计"
    capabilities:
      - L0_LocalCompute
      - L2_FileWrite
      - L3_NetworkOut
    skills:
      - id: "ui-design"
      - id: "image-generation"
    
  # QA Agent
  - name: "QABot"
    role: "tester"
    description: "负责质量保证和测试"
    capabilities:
      - L0_LocalCompute
      - L1_FileRead
      - L3_NetworkOut
    skills:
      - id: "automated-testing"
      - id: "bug-tracking"

# 协作配置
collaboration:
  # 通信协议
  protocol: "a2a"
  
  # 工作流
  workflows:
    - name: "feature_development"
      steps:
        - assignee: "ProjectManager"
          action: "create_task"
          
        - assignee: "DesignBot"
          action: "create_design"
          
        - assignee: "DevBot"
          action: "implement"
          
        - assignee: "QABot"
          action: "test"
          
        - assignee: "ProjectManager"
          action: "review"
  
  # 冲突解决
  conflict_resolution:
    strategy: "vote"
    tie_breaker: "ProjectManager"
```

创建团队：

```bash
# 创建整个团队
beebotos-cli team create \
  --config examples/project-team.yaml

# 启动协作
beebotos-cli team start ProjectAlpha-Team

# 分配任务
beebotos-cli team assign ProjectAlpha-Team \
  --task "开发登录功能" \
  --to DevBot
```

---

## 快速参考

### 常用命令

```bash
# 创建 Agent
beebotos-cli agent create --config <file>

# 启动 Agent
beebotos-cli agent start <agent-id>

# 查看状态
beebotos-cli agent status <agent-id>

# 与 Agent 对话
beebotos-cli agent chat <agent-id> --message "..."

# 停止 Agent
beebotos-cli agent stop <agent-id>

# 删除 Agent
beebotos-cli agent delete <agent-id>
```

### 配置文件模板

```bash
# 生成配置文件模板
beebotos-cli template agent \
  --type <type> \
  --output agent.yaml

# 可用类型: basic, defi, content, support, data, team
```

---

## 更多示例

更多示例请参考：

- [GitHub 示例仓库](https://github.com/beebotos/examples)
- [社区贡献](https://github.com/beebotos/community-examples)

---

**最后更新**: 2026-03-13
