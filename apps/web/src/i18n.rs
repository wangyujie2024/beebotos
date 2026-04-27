//! Internationalization (i18n) module for BeeBotOS Web
//!
//! Provides multi-language support with Chinese (zh-CN) as default

use std::collections::HashMap;

use leptos::prelude::*;

/// Supported locales
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Locale {
    ZhCN,
    En,
}

impl Default for Locale {
    fn default() -> Self {
        Locale::ZhCN
    }
}

impl Locale {
    pub fn as_str(&self) -> &'static str {
        match self {
            Locale::ZhCN => "zh-CN",
            Locale::En => "en",
        }
    }
}

/// I18n context holding current locale and translations
#[derive(Clone)]
pub struct I18nContext {
    locale: RwSignal<Locale>,
    translations: HashMap<&'static str, HashMap<&'static str, &'static str>>,
}

impl I18nContext {
    /// Get current locale
    pub fn get_locale(&self) -> Locale {
        self.locale.get()
    }

    /// Set locale
    pub fn set_locale(&self, locale: Locale) {
        self.locale.set(locale);
    }

    /// Translate a key
    pub fn t(&self, key: &str) -> String {
        let locale_str = self.get_locale().as_str();
        self.translations
            .get(locale_str)
            .and_then(|t| t.get(key))
            .copied()
            .unwrap_or(key)
            .to_string()
    }
}

/// Initialize i18n context
pub fn init_i18n() -> I18nContext {
    let mut translations: HashMap<&'static str, HashMap<&'static str, &'static str>> =
        HashMap::new();

    // Chinese translations
    let mut zh = HashMap::new();
    zh.insert("app-title", "BeeBotOS - Web4.0 自主智能体操作系统");
    zh.insert("app-description", "自主 AI 智能体的操作系统");
    zh.insert("nav-home", "首页");
    zh.insert("nav-agents", "智能体");
    zh.insert("nav-dao", "DAO 治理");
    zh.insert("nav-treasury", "金库");
    zh.insert("nav-skills", "技能市场");
    zh.insert("nav-skill-instances", "实例管理");
    zh.insert("nav-channels", "频道管理");
    zh.insert("nav-settings", "设置");
    zh.insert("nav-models", "模型");
    zh.insert("nav-llm-config", "LLM 配置");
    zh.insert("nav-chat", "聊天");
    zh.insert("nav-browser", "浏览器");
    zh.insert("action-get-started", "开始使用");
    zh.insert("action-browse-skills", "浏览技能");
    zh.insert("action-create", "创建");
    zh.insert("action-view", "查看");
    zh.insert("action-browse", "浏览");
    zh.insert("action-save", "保存");
    zh.insert("action-cancel", "取消");
    zh.insert("action-delete", "删除");
    zh.insert("action-edit", "编辑");
    zh.insert("action-submit", "提交");
    zh.insert("action-refresh", "刷新");
    zh.insert("action-loading", "加载中...");
    zh.insert("action-back", "返回");
    zh.insert("action-close", "关闭");
    zh.insert("action-search", "搜索");
    zh.insert("action-filter", "筛选");
    zh.insert("action-install", "安装");
    zh.insert("action-uninstall", "卸载");
    zh.insert("action-enable", "启用");
    zh.insert("action-disable", "禁用");
    zh.insert("action-login", "登录");
    zh.insert("action-logout", "退出登录");
    zh.insert("action-register", "注册");
    // Login page
    zh.insert("login-title", "欢迎回来");
    zh.insert("login-subtitle", "登录到您的 BeeBotOS 账户");
    zh.insert("login-username", "用户名");
    zh.insert("login-username-placeholder", "请输入用户名");
    zh.insert("login-password", "密码");
    zh.insert("login-password-placeholder", "请输入密码");
    zh.insert("login-error-empty", "用户名和密码不能为空");
    zh.insert("login-error-failed", "登录失败");
    zh.insert("login-or", "或");
    zh.insert("login-demo-button", "演示登录");
    zh.insert("login-no-account", "还没有账户？");
    zh.insert("login-register-link", "立即注册");
    // Register page
    zh.insert("register-title", "创建账户");
    zh.insert("register-subtitle", "注册 BeeBotOS 账户开始使用");
    zh.insert("register-username", "用户名");
    zh.insert("register-username-placeholder", "请输入用户名");
    zh.insert("register-email", "邮箱");
    zh.insert("register-email-placeholder", "请输入邮箱（可选）");
    zh.insert("register-password", "密码");
    zh.insert("register-password-placeholder", "请输入密码（至少6位）");
    zh.insert("register-confirm-password", "确认密码");
    zh.insert("register-confirm-password-placeholder", "请再次输入密码");
    zh.insert("register-error-empty", "用户名和密码不能为空");
    zh.insert("register-error-password-mismatch", "两次输入的密码不一致");
    zh.insert("register-error-password-short", "密码长度至少6位");
    zh.insert("register-error-failed", "注册失败");
    zh.insert("register-or", "或");
    zh.insert("register-demo-button", "演示注册");
    zh.insert("register-have-account", "已有账户？");
    zh.insert("register-login-link", "立即登录");
    zh.insert("hero-title", "自主 AI 智能体的操作系统");
    zh.insert(
        "hero-subtitle",
        "构建、部署和管理具备内置治理功能的智能代理",
    );
    zh.insert("hero-cta-primary", "开始使用");
    zh.insert("hero-cta-secondary", "浏览技能");
    zh.insert("features-title", "核心功能");
    zh.insert("feature-agents-title", "自主智能体");
    zh.insert(
        "feature-agents-desc",
        "部署具备内置安全控制的独立运行 AI 智能体",
    );
    zh.insert("feature-dao-title", "DAO 治理");
    zh.insert("feature-dao-desc", "通过透明投票机制实现社区驱动决策");
    zh.insert("feature-treasury-title", "安全金库");
    zh.insert("feature-treasury-desc", "多签金库管理，链上透明可追溯");
    zh.insert("feature-skills-title", "技能市场");
    zh.insert("feature-skills-desc", "通过社区构建的技能扩展智能体能力");
    zh.insert("feature-wasm-title", "WebAssembly 运行时");
    zh.insert("feature-wasm-desc", "高性能、沙盒化执行环境");
    zh.insert("feature-analytics-title", "实时分析");
    zh.insert("feature-analytics-desc", "实时监控智能体性能和系统健康状况");
    zh.insert("quick-actions-title", "快速操作");
    zh.insert("quick-action-create-agent-title", "创建智能体");
    zh.insert("quick-action-create-agent-desc", "设置新的自主智能体");
    zh.insert("quick-action-view-proposals-title", "查看提案");
    zh.insert("quick-action-view-proposals-desc", "参与 DAO 治理投票");
    zh.insert("quick-action-install-skills-title", "安装技能");
    zh.insert("quick-action-install-skills-desc", "为智能体添加新能力");
    zh.insert("agents-title", "智能体管理");
    zh.insert("agents-subtitle", "管理您的自主 AI 智能体");
    zh.insert("agents-create-new", "创建新智能体");
    zh.insert("agents-no-agents", "暂无智能体");
    zh.insert("agents-loading", "加载中...");
    zh.insert("agents-error", "加载失败");
    zh.insert("status-active", "运行中");
    zh.insert("status-idle", "空闲");
    zh.insert("status-paused", "已暂停");
    zh.insert("status-error", "错误");
    zh.insert("status-offline", "离线");
    zh.insert("status-running", "运行中");
    zh.insert("status-completed", "已完成");
    zh.insert("status-pending", "待处理");
    // Channels
    zh.insert("channels-title", "频道管理");
    zh.insert("channels-subtitle", "配置和管理各消息频道的连接");
    zh.insert("channel-status", "频道状态");
    zh.insert("channel-config", "频道配置");
    zh.insert("status-enabled", "已启用");
    zh.insert("status-disabled", "未启用");
    zh.insert("wechat-login", "微信登录");
    zh.insert(
        "wechat-login-hint",
        "使用微信扫描二维码登录，获取 Bot Token",
    );
    zh.insert("qr-expires-in", "二维码过期时间");
    zh.insert("action-get-qr", "获取二维码");
    zh.insert("action-refresh-qr", "刷新二维码");
    zh.insert("action-test", "测试连接");
    zh.insert("config-base-url", "Base URL");
    zh.insert("config-bot-token", "Bot Token");
    zh.insert("config-auto-reconnect", "自动重连");
    zh.insert("scan-qr", "扫码");
    zh.insert("click-open-wechat-scan", "点击打开微信扫码页面");
    zh.insert("wechat-scan-hint", "请使用微信扫描页面中的二维码");
    zh.insert("qr-code-label", "二维码");
    zh.insert("qr-status-confirmed", "扫码成功，登录完成");
    zh.insert("qr-status-scanned", "已扫码，等待确认");
    zh.insert("qr-status-expired", "二维码已过期，请重新获取");
    zh.insert("qr-status-waiting", "等待扫码...");
    zh.insert("poll-qr-failed", "轮询二维码状态失败");
    zh.insert("get-qr-failed", "获取二维码失败");
    zh.insert("config-enabled", "已启用");
    zh.insert("channel-enabled-msg", "频道已启用");
    zh.insert("channel-disabled-msg", "频道已禁用");
    zh.insert("connection-test-passed", "连接测试通过");
    zh.insert("connection-test-failed", "连接测试失败");
    zh.insert("test-failed", "测试失败");
    zh.insert("config-saved", "配置已保存");
    zh.insert("save-failed", "保存失败");

    zh.insert("dao-title", "DAO 治理");
    zh.insert("dao-subtitle", "参与社区决策");
    zh.insert("dao-active-proposals", "活跃提案");
    zh.insert("dao-completed-proposals", "已完成提案");
    zh.insert("dao-create-proposal", "创建提案");
    zh.insert("dao-vote-for", "赞成");
    zh.insert("dao-vote-against", "反对");
    zh.insert("dao-votes-for", "赞成票");
    zh.insert("dao-votes-against", "反对票");
    zh.insert("dao-voting-ends", "投票截止");
    zh.insert("dao-executed", "已执行");
    zh.insert("treasury-title", "金库管理");
    zh.insert("treasury-subtitle", "管理 DAO 资产和交易");
    zh.insert("treasury-total-balance", "总资产");
    zh.insert("treasury-assets", "资产列表");
    zh.insert("treasury-transactions", "交易记录");
    zh.insert("treasury-deposit", "存入");
    zh.insert("treasury-withdraw", "提取");
    zh.insert("skills-title", "技能市场");
    zh.insert("skills-subtitle", "发现和安装智能体能力");
    zh.insert("skills-categories", "分类");
    zh.insert("skills-installed", "已安装");
    zh.insert("skills-available", "可用");
    zh.insert("skills-search-placeholder", "搜索技能...");
    zh.insert("settings-title", "系统设置");
    zh.insert("settings-subtitle", "配置您的 BeeBotOS 实例");
    zh.insert("settings-general", "常规设置");
    zh.insert("settings-appearance", "外观设置");
    zh.insert("settings-language", "语言");
    zh.insert("settings-theme", "主题");
    zh.insert("theme-light", "浅色");
    zh.insert("theme-dark", "深色");
    zh.insert("theme-system", "跟随系统");
    zh.insert("settings-notifications", "通知设置");
    zh.insert("settings-security", "安全设置");
    zh.insert("settings-wallet", "钱包设置");
    zh.insert("settings-system", "系统信息");
    zh.insert("footer-copyright", "© 2026 BeeBotOS. 保留所有权利。");
    zh.insert("footer-version", "版本");
    zh.insert("error-404-title", "404");
    zh.insert("error-404-message", "页面未找到");
    zh.insert("error-404-description", "您访问的页面不存在或已被移动。");
    zh.insert("error-go-home", "返回首页");
    zh.insert("error-generic", "出错了");
    zh.insert("error-retry", "重试");
    zh.insert("notification-success", "成功");
    zh.insert("notification-error", "错误");
    zh.insert("notification-warning", "警告");
    zh.insert("notification-info", "信息");
    zh.insert("logout-success", "您已成功退出登录");
    zh.insert("stats-tasks", "完成任务");
    zh.insert("stats-uptime", "系统正常运行时间");
    zh.insert("stats-members", "社区成员");
    zh.insert("nav-home", "首页");
    zh.insert("quick-action-start-chat-title", "开始聊天");
    zh.insert("quick-action-start-chat-desc", "与AI智能体开始对话");
    zh.insert("nav-section-chat", "聊天");
    zh.insert("nav-section-control", "控制");
    zh.insert("nav-section-agents", "智能体");
    zh.insert("nav-section-settings", "设置");
    zh.insert("footer-product", "产品");
    zh.insert("footer-resources", "资源");
    zh.insert("footer-community", "社区");

    // Components - loading
    zh.insert("loading-page", "加载中...");
    zh.insert("loading-inline", "加载中...");
    zh.insert("loading-more", "加载更多...");
    // Components - error boundary
    zh.insert("error-refresh-hint", "请刷新页面后重试。");
    zh.insert("error-async-handler", "AsyncHandler 在 CSR 模式下不受支持 - 请使用 LocalResource");
    // Components - guard
    zh.insert("auth-checking", "正在检查认证...");
    zh.insert("redirecting", "正在重定向...");
    zh.insert("error-access-denied", "访问被拒绝");
    zh.insert("error-access-denied-desc", "您没有权限访问此页面。");
    zh.insert("contact-support", "联系支持");
    // Components - footer
    zh.insert("footer-openclaw", "OpenClaw v2026.3.13");
    zh.insert("footer-web4-ready", "Web4.0 Ready");
    // Components - command palette
    zh.insert("cmd-palette-hint", "按 Ctrl+K 打开命令面板");
    zh.insert("cmd-palette-placeholder", "输入命令或搜索...");
    zh.insert("cmd-palette-empty", "未找到命令");
    zh.insert("cmd-palette-open", "打开命令面板");
    // Components - chat search
    zh.insert("search-messages-placeholder", "搜索消息...");
    zh.insert("search-results", "条结果");
    zh.insert("search-in", "在");
    zh.insert("filter-from-date", "开始日期");
    zh.insert("filter-to-date", "结束日期");
    zh.insert("filter-sender", "发送者");
    zh.insert("filter-sender-placeholder", "用户名");
    zh.insert("filter-channel", "频道");
    zh.insert("filter-channel-placeholder", "频道名称");
    zh.insert("filter-has-attachments", "仅含附件");
    zh.insert("export-messages-title", "导出消息");
    zh.insert("export-format", "导出格式");
    zh.insert("format-plain-text", "纯文本");
    zh.insert("date-range", "日期范围");
    zh.insert("range-all-messages", "全部消息");
    zh.insert("range-today", "今天");
    zh.insert("range-last-7-days", "最近7天");
    zh.insert("range-last-30-days", "最近30天");
    zh.insert("range-custom", "自定义范围");
    zh.insert("include-attachments", "包含附件");
    zh.insert("action-export", "导出");
    zh.insert("pinned-messages", "置顶消息");
    zh.insert("jump-to-message", "跳转到消息");
    zh.insert("unpin-message", "取消置顶");
    zh.insert("slash-command-placeholder", "输入 / 查看命令...");
    // Components - pagination
    zh.insert("pagination-previous", "上一页");
    zh.insert("pagination-next", "下一页");
    zh.insert("pagination-page", "第");
    zh.insert("pagination-of", "页，共");
    zh.insert("pagination-items", "条");
    zh.insert("page-size-show", "显示：");
    zh.insert("page-size-per-page", "条/页");
    // Components - sidebar
    zh.insert("user-status-online", "在线");

    // WebChat components
    zh.insert("message-input-placeholder", "输入消息...");
    zh.insert("message-input-hint-send", "按 Enter 发送，Shift+Enter 换行");
    zh.insert("message-input-hint-btw", "使用 /btw 进行侧边提问");
    zh.insert("session-list-title", "会话列表");
    zh.insert("session-list-new", "+ 新建");
    zh.insert("session-list-empty-title", "暂无会话");
    zh.insert("session-list-empty-hint", "点击「新建」开始聊天");
    zh.insert("session-messages-count", "{} 条消息");
    zh.insert("side-panel-title", "侧边提问");
    zh.insert("side-panel-empty-title", "暂无侧边提问");
    zh.insert("side-panel-empty-hint", "使用 /btw 提问");
    zh.insert("side-panel-input-placeholder", "输入侧边提问...");
    zh.insert("side-panel-ask-button", "提问");
    zh.insert("usage-panel-title", "Token 用量");
    zh.insert("usage-panel-session", "当前会话");
    zh.insert("usage-panel-daily", "今日");
    zh.insert("usage-panel-monthly", "本月");
    zh.insert("usage-panel-limits", "限额");
    zh.insert("usage-panel-daily-remaining", "今日剩余：");
    zh.insert("usage-panel-monthly-remaining", "本月剩余：");
    zh.insert("usage-panel-approaching-limit", "⚠️ 接近限额");
    zh.insert("usage-panel-prompt", "提示：{}");
    zh.insert("usage-panel-completion", "补全：{}");

    // Browser components
    zh.insert("browser-url-placeholder", "输入 URL...");
    zh.insert("browser-go", "前往");
    zh.insert("browser-loading", "加载中...");
    zh.insert("browser-not-connected", "未连接浏览器");
    zh.insert("browser-select-profile", "选择一个配置进行连接");
    zh.insert("debug-console-title", "调试控制台");
    zh.insert("debug-console-clear", "清空");
    zh.insert("debug-console-export", "导出");
    zh.insert("debug-console-empty", "暂无日志");
    zh.insert("debug-console-filter-level", "过滤级别：");
    zh.insert("debug-console-filter-all", "全部");
    zh.insert("debug-console-filter-debug", "调试");
    zh.insert("debug-console-filter-info", "信息");
    zh.insert("debug-console-filter-warning", "警告");
    zh.insert("debug-console-filter-error", "错误");
    zh.insert("debug-console-filter-critical", "严重");
    zh.insert("profile-cdp-port", "CDP 端口：");
    zh.insert("profile-status", "状态：");
    zh.insert("profile-connect", "连接");
    zh.insert("profile-disconnect", "断开");
    zh.insert("profile-edit", "编辑");
    zh.insert("sandbox-cdp-port", "CDP 端口：");
    zh.insert("sandbox-isolation", "隔离级别：");
    zh.insert("sandbox-memory", "内存限制：");
    zh.insert("sandbox-start", "启动");
    zh.insert("sandbox-stop", "停止");
    zh.insert("sandbox-delete", "删除");

    // Wizard components
    zh.insert("wizard-step-welcome", "欢迎");
    zh.insert("wizard-step-server", "服务器");
    zh.insert("wizard-step-database", "数据库");
    zh.insert("wizard-step-security", "安全");
    zh.insert("wizard-step-llm", "LLM 模型");
    zh.insert("wizard-step-channels", "频道");
    zh.insert("wizard-step-blockchain", "区块链");
    zh.insert("wizard-step-logging", "日志");
    zh.insert("wizard-step-review", "审核");
    zh.insert("wizard-step-deploy", "部署");
    zh.insert("wizard-back", "← 返回");
    zh.insert("wizard-next", "下一步 →");
    zh.insert("wizard-step-indicator", "第 {current} / {total} 步");
    zh.insert("wizard-deploying", "部署中...");
    zh.insert("wizard-save-export", "保存并导出 ▼");
    zh.insert("secret-input-placeholder", "输入密钥...");
    zh.insert("secret-input-show", "显示");
    zh.insert("secret-input-hide", "隐藏");
    zh.insert("secret-input-generate", "生成");
    zh.insert("provider-api-key", "API 密钥");
    zh.insert("provider-model", "模型");
    zh.insert("provider-base-url", "基础 URL");
    zh.insert("provider-temperature", "温度");
    zh.insert("provider-context-window", "上下文窗口");
    zh.insert("provider-default", "默认");
    zh.insert("platform-enabled", "已启用");
    zh.insert("platform-disabled", "已禁用");
    zh.insert("platform-config-hint", "在 beebotos.toml 中配置 {} 设置");

    // Agents page
    zh.insert("agents-create-first", "创建您的第一个自主智能体以开始使用");
    zh.insert("action-manage", "管理");
    zh.insert("action-start", "启动");
    zh.insert("action-stop", "停止");
    zh.insert("action-starting", "启动中...");
    zh.insert("action-stopping", "停止中...");
    zh.insert("action-creating", "创建中...");
    zh.insert("action-saving", "保存中...");
    zh.insert("action-refresh-page", "刷新页面");
    zh.insert("modal-create-agent-title", "创建新智能体");
    zh.insert("label-agent-name", "智能体名称 *");
    zh.insert("placeholder-agent-name", "输入智能体名称");
    zh.insert("label-description", "描述");
    zh.insert("placeholder-agent-description", "输入智能体描述");
    zh.insert("label-model-provider", "模型提供商");
    zh.insert("label-model-name", "模型名称");
    zh.insert("placeholder-model-name", "例如 gpt-4, claude-3-opus-20240229");
    zh.insert("notification-agent-started", "智能体已启动");
    zh.insert("notification-agent-started-desc", "智能体已启动");
    zh.insert("notification-start-failed", "启动失败");
    zh.insert("notification-start-failed-desc", "启动智能体失败");
    zh.insert("notification-agent-stopped", "智能体已停止");
    zh.insert("notification-agent-stopped-desc", "智能体已停止");
    zh.insert("notification-stop-failed", "停止失败");
    zh.insert("notification-stop-failed-desc", "停止智能体失败");

    // Agent detail page
    zh.insert("agent-detail-title", "智能体详情");
    zh.insert("agent-detail-id-required", "需要提供智能体 ID");
    zh.insert("modal-edit-agent-title", "编辑智能体");
    zh.insert("modal-configure-agent-title", "配置智能体");
    zh.insert("modal-agent-logs-title", "智能体日志");
    zh.insert("modal-confirm-delete-title", "确认删除");
    zh.insert("label-name", "名称");
    zh.insert("label-clone", "副本");
    zh.insert("notification-agent-cloned", "智能体克隆成功");
    zh.insert("notification-agent-cloned-desc", "智能体克隆成功");
    zh.insert("notification-clone-failed", "克隆失败");
    zh.insert("notification-clone-failed-desc", "克隆智能体失败");
    zh.insert("notification-update-failed", "更新失败");
    zh.insert("notification-delete-failed", "删除失败");
    zh.insert("agent-config-hint", "智能体级别的配置选项将在此处提供。全局模型设置可在 LLM 配置中进行。");
    zh.insert("action-go-to-llm-config", "前往 LLM 配置");
    zh.insert("logs-loading", "加载日志...");
    zh.insert("logs-empty", "暂无日志");
    zh.insert("delete-confirm-prefix", "确定要删除");
    zh.insert("delete-confirm-suffix", "");
    zh.insert("delete-confirm-irreversible", "此操作无法撤销");
    zh.insert("section-overview", "概览");
    zh.insert("section-capabilities", "能力");
    zh.insert("section-recent-activity", "最近活动");
    zh.insert("section-quick-stats", "快速统计");
    zh.insert("section-actions", "操作");
    zh.insert("label-agent-id", "智能体 ID");
    zh.insert("label-created", "创建时间");
    zh.insert("label-updated", "更新时间");
    zh.insert("label-tasks-completed", "已完成任务");
    zh.insert("label-uptime", "正常运行时间");
    zh.insert("label-unknown", "未知");
    zh.insert("label-status", "状态");
    zh.insert("label-tasks", "任务");
    zh.insert("capabilities-empty", "未配置能力");
    zh.insert("action-view-logs", "查看日志");
    zh.insert("action-configure", "配置");
    zh.insert("action-clone-agent", "克隆智能体");
    zh.insert("action-export-config", "导出配置");
    zh.insert("action-back-to-agents", "返回智能体列表");
    zh.insert("agent-not-found", "智能体未找到");
    zh.insert("activity-loading", "加载活动中...");
    zh.insert("activity-empty", "暂无最近活动");
    zh.insert("activity-load-error", "无法加载活动日志");

    // Browser page
    zh.insert("browser-title", "浏览器自动化");
    zh.insert("browser-subtitle", "Chrome DevTools MCP 控制 - 兼容 OpenClaw V2026.3.13");
    zh.insert("browser-profiles", "配置文件");
    zh.insert("browser-add-profile", "添加配置文件");
    zh.insert("browser-sandboxes", "沙箱");
    zh.insert("browser-create-sandbox", "创建沙箱");
    zh.insert("browser-debug-hidden", "调试面板已隐藏");
    zh.insert("browser-url-placeholder", "输入 URL...");
    zh.insert("browser-go", "前往");
    zh.insert("browser-connecting", "连接中...");
    zh.insert("browser-connection-failed", "连接失败");
    zh.insert("browser-no-connection", "未连接浏览器");
    zh.insert("browser-select-profile", "选择一个配置文件进行连接");
    zh.insert("browser-debug-console", "调试控制台");
    zh.insert("browser-clear", "清空");
    zh.insert("browser-debug-placeholder", "调试日志将显示在此处...");
    zh.insert("modal-add-profile-title", "添加浏览器配置文件");
    zh.insert("label-profile-name", "配置文件名称");
    zh.insert("placeholder-profile-name", "例如：工作配置文件");
    zh.insert("label-cdp-port", "CDP 端口");
    zh.insert("modal-create-sandbox-title", "创建沙箱");
    zh.insert("label-sandbox-name", "沙箱名称");
    zh.insert("placeholder-sandbox-name", "例如：测试沙箱");
    zh.insert("label-base-profile", "基础配置文件");

    // DAO page
    zh.insert("dao-view-treasury", "查看金库 →");
    zh.insert("dao-governance-proposals", "治理提案");
    zh.insert("dao-members", "DAO 成员");
    zh.insert("dao-your-voting-power", "您的投票权");
    zh.insert("dao-your-balance", "您的余额");
    zh.insert("dao-past-proposals", "历史提案");
    zh.insert("dao-create-failed", "创建提案失败");
    zh.insert("dao-load-proposals-failed", "加载提案失败");
    zh.insert("modal-create-proposal-title", "创建提案");
    zh.insert("label-title", "标题");
    zh.insert("placeholder-proposal-title", "提案标题");
    zh.insert("placeholder-proposal-desc", "描述您的提案...");
    zh.insert("label-type", "类型");
    zh.insert("proposal-type-general", "普通");
    zh.insert("proposal-type-funding", "资金");
    zh.insert("proposal-type-upgrade", "升级");
    zh.insert("proposal-type-parameter", "参数");
    zh.insert("proposal-by", "提案人：");
    zh.insert("proposals-empty-title", "暂无提案");
    zh.insert("proposals-empty-desc", "成为第一个创建治理提案的人");
    zh.insert("voted-for", "您投了赞成票");
    zh.insert("voted-against", "您投了反对票");
    zh.insert("action-voting", "投票中...");
    zh.insert("notification-vote-submitted", "投票已提交");
    zh.insert("notification-vote-submitted-desc", "您的投票已成功记录");
    zh.insert("notification-vote-failed", "投票失败");
    zh.insert("notification-vote-failed-desc", "提交投票失败");

    // LLM Config page
    zh.insert("llm-config-title", "LLM 配置");
    zh.insert("llm-config-subtitle", "全局 LLM 设置和实时监控");
    zh.insert("llm-global-config", "全局配置");
    zh.insert("llm-default-provider", "默认提供商");
    zh.insert("llm-max-tokens", "最大 Token 数");
    zh.insert("llm-request-timeout", "请求超时");
    zh.insert("llm-cost-optimization", "成本优化");
    zh.insert("llm-enabled", "已启用");
    zh.insert("llm-disabled", "已禁用");
    zh.insert("llm-fallback-chain", "回退链");
    zh.insert("llm-system-prompt", "系统提示词");
    zh.insert("llm-providers", "提供商");
    zh.insert("llm-healthy", "健康");
    zh.insert("llm-failures", "次失败");
    zh.insert("llm-model", "模型");
    zh.insert("llm-base-url", "基础 URL");
    zh.insert("llm-api-key", "API 密钥");
    zh.insert("llm-temperature", "温度");
    zh.insert("llm-context-window", "上下文窗口");
    zh.insert("llm-default", "默认");
    zh.insert("llm-realtime-metrics", "实时监控");
    zh.insert("llm-last-updated", "最后更新");
    zh.insert("llm-total-requests", "总请求数");
    zh.insert("llm-success-rate", "成功率");
    zh.insert("llm-successful", "成功");
    zh.insert("llm-failed", "失败");
    zh.insert("llm-total-tokens", "总 Token 数");
    zh.insert("llm-input", "输入");
    zh.insert("llm-output", "输出");
    zh.insert("llm-latency", "延迟");
    zh.insert("llm-avg", "平均");
    zh.insert("llm-p50", "P50");
    zh.insert("llm-p95", "P95");
    zh.insert("llm-p99", "P99");
    zh.insert("llm-visual-overview", "可视化概览");
    zh.insert("llm-request-distribution", "请求分布");
    zh.insert("llm-token-usage", "Token 使用");
    zh.insert("llm-latency-percentiles", "延迟百分位 (ms)");
    zh.insert("llm-success", "成功");

    // ==================== settings.rs ====================
    zh.insert("settings-page-title", "设置 - BeeBotOS");
    zh.insert("settings-heading", "设置");
    zh.insert("settings-description", "管理您的偏好设置和系统配置");
    zh.insert("settings-loading", "正在加载设置...");
    zh.insert("settings-save-success", "设置保存成功");
    zh.insert("settings-save-local", "已保存到本地");
    zh.insert("settings-appearance", "外观");
    zh.insert("settings-theme", "主题");
    zh.insert("settings-language", "语言");
    zh.insert("lang-en", "English");
    zh.insert("lang-zh", "中文");
    zh.insert("lang-ja", "日本語");
    zh.insert("lang-ko", "한국어");
    zh.insert("settings-notifications", "通知");
    zh.insert("settings-enable-notifications", "启用通知");
    zh.insert("settings-notifications-help", "接收关于智能体状态和 DAO 治理的提醒");
    zh.insert("settings-auto-update", "自动更新");
    zh.insert("settings-auto-update-help", "自动更新到最新版本");
    zh.insert("settings-network", "网络");
    zh.insert("settings-api-endpoint", "API 端点");
    zh.insert("settings-api-endpoint-help", "自定义 API 端点（留空使用默认）");
    zh.insert("settings-wallet", "钱包");
    zh.insert("settings-wallet-address", "钱包地址");
    zh.insert("settings-wallet-help", "用于参与 DAO 的钱包地址");
    zh.insert("settings-connect-wallet", "连接钱包");
    zh.insert("settings-disconnect-wallet", "断开连接");
    zh.insert("settings-ai-config", "AI 配置");
    zh.insert("settings-ai-config-help", "查看全局 LLM 提供商设置和指标");
    zh.insert("settings-open-llm-config", "打开 LLM 配置 →");
    zh.insert("settings-gateway-setup", "网关设置");
    zh.insert("settings-gateway-setup-help", "运行配置向导来设置或重新配置网关");
    zh.insert("settings-config-wizard", "配置向导 →");
    zh.insert("settings-system", "系统");
    zh.insert("settings-version", "版本");
    zh.insert("settings-build", "构建");
    zh.insert("settings-platform", "平台");
    zh.insert("settings-check-updates", "检查更新");
    zh.insert("settings-reload-config", "重新加载配置");
    zh.insert("settings-reload-failed", "重新加载失败");
    zh.insert("settings-reset-defaults", "恢复默认");
    zh.insert("settings-saving", "保存中...");
    zh.insert("settings-save-changes", "保存更改");

    // ==================== setup.rs ====================
    zh.insert("setup-page-title", "网关设置 - BeeBotOS");
    zh.insert("setup-welcome-title", "BeeBotOS 网关设置");
    zh.insert("setup-welcome-subtitle", "通过几个简单步骤配置您的网关");
    zh.insert("setup-mode-fresh", "从头开始");
    zh.insert("setup-mode-fresh-desc", "从零开始创建新配置");
    zh.insert("setup-mode-minimal", "最小化");
    zh.insert("setup-mode-minimal-desc", "SQLite + Kimi + WebChat — 用于本地测试");
    zh.insert("setup-mode-standard", "标准");
    zh.insert("setup-mode-standard-desc", "多提供商 + 5 个频道 — 用于生产环境");
    zh.insert("setup-mode-enterprise", "企业版");
    zh.insert("setup-mode-enterprise-desc", "Postgres + TLS + OTLP — 完整堆栈");
    zh.insert("setup-server-title", "服务器配置");
    zh.insert("setup-server-desc", "配置 HTTP/gRPC 服务器设置");
    zh.insert("setup-server-host", "主机");
    zh.insert("setup-server-http-port", "HTTP 端口");
    zh.insert("setup-server-grpc-port", "gRPC 端口");
    zh.insert("setup-server-timeout", "请求超时（秒）");
    zh.insert("setup-server-max-body", "最大请求体（MB）");
    zh.insert("setup-server-cors", "CORS 来源（逗号分隔）");
    zh.insert("setup-server-advanced-tls", "高级 TLS 选项");
    zh.insert("setup-server-enable-tls", "启用 TLS");
    zh.insert("setup-server-tls-cert", "TLS 证书路径");
    zh.insert("setup-server-tls-key", "TLS 密钥路径");
    zh.insert("setup-server-enable-mtls", "启用 mTLS");
    zh.insert("setup-db-title", "数据库配置");
    zh.insert("setup-db-desc", "选择您的数据库引擎");
    zh.insert("setup-db-type", "数据库类型");
    zh.insert("setup-db-sqlite", "SQLite");
    zh.insert("setup-db-postgres", "PostgreSQL");
    zh.insert("setup-db-sqlite-path", "SQLite 路径");
    zh.insert("setup-db-postgres-url", "PostgreSQL URL");
    zh.insert("setup-db-max-conn", "最大连接数");
    zh.insert("setup-db-min-conn", "最小连接数");
    zh.insert("setup-db-connect-timeout", "连接超时（秒）");
    zh.insert("setup-db-auto-migrate", "启动时自动迁移");
    zh.insert("setup-security-title", "JWT 与安全");
    zh.insert("setup-security-desc", "配置认证和速率限制");
    zh.insert("setup-security-jwt-secret", "JWT 密钥");
    zh.insert("setup-security-jwt-placeholder", "最少 32 个字符");
    zh.insert("setup-security-jwt-help", "用于签署 JWT 令牌。请妥善保管！");
    zh.insert("setup-security-token-expiry", "令牌过期时间（秒）");
    zh.insert("setup-security-refresh-expiry", "刷新令牌过期时间（秒）");
    zh.insert("setup-security-enable-rate-limit", "启用速率限制");
    zh.insert("setup-security-qps-limit", "QPS 限制");
    zh.insert("setup-security-burst-limit", "突发限制");
    zh.insert("setup-llm-title", "LLM 模型配置");
    zh.insert("setup-llm-desc", "配置 AI 提供商和回退链");
    zh.insert("setup-llm-default-provider", "默认提供商");
    zh.insert("setup-llm-max-tokens", "最大令牌数");
    zh.insert("setup-llm-timeout", "请求超时（秒）");
    zh.insert("setup-llm-cost-opt", "启用成本优化");
    zh.insert("setup-llm-system-prompt", "系统提示");
    zh.insert("setup-llm-providers", "提供商");
    zh.insert("setup-llm-api-key", "API 密钥");
    zh.insert("setup-llm-model", "模型");
    zh.insert("setup-llm-base-url", "基础 URL");
    zh.insert("setup-llm-temperature", "温度");
    zh.insert("setup-llm-context-window", "上下文窗口");
    zh.insert("setup-llm-provider-placeholder", "提供商名称（例如 kimi, openai）");
    zh.insert("setup-llm-add-provider", "+ 添加提供商");
    zh.insert("setup-channels-title", "频道配置");
    zh.insert("setup-channels-desc", "启用和配置通信平台");
    zh.insert("setup-channels-context-window", "上下文窗口（消息数）");
    zh.insert("setup-channels-max-file", "最大文件大小（MB）");
    zh.insert("setup-channels-default-agent", "默认智能体 ID");
    zh.insert("setup-channels-auto-download", "自动下载媒体");
    zh.insert("setup-channels-auto-reply", "自动回复");
    zh.insert("setup-channels-platforms", "平台");
    zh.insert("setup-blockchain-title", "区块链配置");
    zh.insert("setup-blockchain-desc", "可选的区块链集成");
    zh.insert("setup-blockchain-enable", "启用区块链");
    zh.insert("setup-blockchain-chain-id", "链 ID");
    zh.insert("setup-blockchain-rpc", "RPC URL");
    zh.insert("setup-blockchain-mnemonic", "钱包助记词");
    zh.insert("setup-blockchain-mnemonic-placeholder", "12 或 24 字助记词");
    zh.insert("setup-logging-title", "日志与可观测性");
    zh.insert("setup-logging-desc", "配置日志、指标和追踪");
    zh.insert("setup-logging-level", "日志级别");
    zh.insert("setup-logging-format", "日志格式");
    zh.insert("setup-logging-file-path", "日志文件路径");
    zh.insert("setup-logging-rotation", "日志轮转");
    zh.insert("setup-logging-enable-metrics", "启用指标（Prometheus）");
    zh.insert("setup-logging-metrics-port", "指标端口");
    zh.insert("setup-logging-enable-tracing", "启用 OpenTelemetry 追踪");
    zh.insert("setup-logging-otlp", "OTLP 端点");
    zh.insert("setup-logging-sampling", "追踪采样率");
    zh.insert("log-trace", "追踪");
    zh.insert("log-debug", "调试");
    zh.insert("log-info", "信息");
    zh.insert("log-warn", "警告");
    zh.insert("log-error", "错误");
    zh.insert("log-json", "JSON");
    zh.insert("log-pretty", "美化");
    zh.insert("log-compact", "紧凑");
    zh.insert("log-minutely", "每分钟");
    zh.insert("log-hourly", "每小时");
    zh.insert("log-daily", "每天");
    zh.insert("log-never", "从不");
    zh.insert("setup-review-title", "审核配置");
    zh.insert("setup-review-desc", "在部署前验证您的设置");
    zh.insert("setup-review-warnings", "⚠️ 验证警告");
    zh.insert("setup-review-success", "✅ 所有必填字段已填写");
    zh.insert("setup-deploy-title", "部署配置");
    zh.insert("setup-deploy-desc", "导出并应用您的配置");
    zh.insert("setup-deploy-toml-desc", "下载 beebotos.toml 并将其放置在 config/ 目录中");
    zh.insert("setup-deploy-toml-btn", "下载 beebotos.toml");
    zh.insert("setup-deploy-env-desc", "导出为 .env 文件，用于 Docker 或 CI/CD");
    zh.insert("setup-deploy-env-btn", "下载 .env");
    zh.insert("setup-deploy-docker-desc", "使用您的设置生成 docker-compose.yml");
    zh.insert("setup-deploy-docker-btn", "下载 docker-compose.yml");
    zh.insert("setup-deploy-k8s-desc", "生成 K8s Deployment 和 Service 清单");
    zh.insert("setup-deploy-k8s-btn", "下载 K8s 清单");
    zh.insert("setup-deploy-instructions-title", "部署说明");
    zh.insert("setup-deploy-step1", "下载您首选的配置格式");
    zh.insert("setup-deploy-step2", "上传到您的服务器");
    zh.insert("setup-deploy-step3", "放置在 config/ 目录中（对于 TOML）或 source .env 文件");
    zh.insert("setup-deploy-step4", "重启网关服务");
    zh.insert("setup-deploy-go-settings", "前往设置");

    // ==================== skills.rs ====================
    zh.insert("skills-page-title", "技能 - BeeBotOS");
    zh.insert("skills-source", "来源：");
    zh.insert("skills-source-local", "本地");
    zh.insert("skills-source-clawhub", "ClawHub");
    zh.insert("skills-source-beehub", "BeeHub");
    zh.insert("skills-search-btn", "🔍 搜索");
    zh.insert("skills-cat-all", "全部");
    zh.insert("skills-cat-trading", "交易");
    zh.insert("skills-cat-data", "数据");
    zh.insert("skills-cat-social", "社交");
    zh.insert("skills-cat-automation", "自动化");
    zh.insert("skills-cat-analysis", "分析");
    zh.insert("skills-installed-badge", "✓ 已安装");
    zh.insert("skills-btn-details", "详情");
    zh.insert("skills-btn-install", "安装");
    zh.insert("skills-btn-uninstall", "卸载");
    zh.insert("skills-btn-view-hub", "在 Hub 上查看");
    zh.insert("skills-installing", "安装中...");
    zh.insert("skills-removing", "移除中...");
    zh.insert("skills-install-success-title", "技能已安装");
    zh.insert("skills-install-success-msg", "安装成功");
    zh.insert("skills-install-fail-title", "安装失败");
    zh.insert("skills-install-fail-msg", "安装失败");
    zh.insert("skills-uninstall-success-title", "技能已卸载");
    zh.insert("skills-uninstall-success-msg", "移除成功");
    zh.insert("skills-uninstall-fail-title", "卸载失败");
    zh.insert("skills-uninstall-fail-msg", "卸载失败");
    zh.insert("skills-detail-version", "版本：");
    zh.insert("skills-detail-author", "作者：");
    zh.insert("skills-detail-license", "许可证：");
    zh.insert("skills-detail-downloads", "下载：");
    zh.insert("skills-detail-rating", "评分：");
    zh.insert("skills-detail-description", "描述：");
    zh.insert("skills-detail-capabilities", "能力：");
    zh.insert("skills-detail-tags", "标签：");
    zh.insert("skills-detail-none", "未列出");
    zh.insert("skills-empty-search", "搜索 {}");
    zh.insert("skills-empty-search-desc", "在上方的搜索框中输入关键词以在此 Hub 上搜索技能。");
    zh.insert("skills-empty-noresults", "在 {} 上没有结果");
    zh.insert("skills-empty-noresults-desc", "尝试不同的搜索词或切换到本地技能。");
    zh.insert("skills-empty-none", "未找到技能");
    zh.insert("skills-empty-none-desc", "尝试调整您的搜索或筛选条件");
    zh.insert("skills-error-title", "加载技能失败");
    zh.insert("skills-error-unavailable", "技能 Hub 当前无法访问。");
    zh.insert("skills-error-unavailable-hint", "请切换到本地技能或检查网关网络配置。");
    zh.insert("skills-error-retry", "重试");

    // ==================== skill_instances.rs ====================
    zh.insert("instances-page-title", "技能实例 - BeeBotOS");
    zh.insert("instances-title", "技能实例");
    zh.insert("instances-subtitle", "管理与您的智能体绑定的技能实例");
    zh.insert("instances-cancel", "✕ 取消");
    zh.insert("instances-new", "+ 新建实例");
    zh.insert("instances-create-title", "创建实例");
    zh.insert("instances-skill-id", "技能 ID");
    zh.insert("instances-skill-id-placeholder", "例如 echo-skill");
    zh.insert("instances-agent-id", "智能体 ID");
    zh.insert("instances-agent-id-placeholder", "例如 agent-001");
    zh.insert("instances-creating", "创建中...");
    zh.insert("instances-create-btn", "创建实例");
    zh.insert("instances-missing-fields-title", "缺少字段");
    zh.insert("instances-missing-fields-msg", "请填写技能 ID 和智能体 ID");
    zh.insert("instances-create-success-title", "实例已创建");
    zh.insert("instances-create-success-msg", "创建成功");
    zh.insert("instances-create-fail-title", "创建失败");
    zh.insert("instances-create-fail-msg", "创建失败");
    zh.insert("instances-delete-success-title", "实例已删除");
    zh.insert("instances-delete-success-msg", "已删除");
    zh.insert("instances-delete-fail-title", "删除失败");
    zh.insert("instances-delete-fail-msg", "删除失败");
    zh.insert("instances-exec-result-title", "执行结果");
    zh.insert("instances-exec-completed", "执行完成，耗时");
    zh.insert("instances-exec-failed", "执行失败");
    zh.insert("instances-exec-fail-title", "执行失败");
    zh.insert("instances-exec-fail-msg", "执行失败");
    zh.insert("instances-col-id", "实例 ID");
    zh.insert("instances-col-skill", "技能");
    zh.insert("instances-col-agent", "智能体");
    zh.insert("instances-col-status", "状态");
    zh.insert("instances-col-usage", "用量");
    zh.insert("instances-col-actions", "操作");
    zh.insert("instances-calls", "次调用");
    zh.insert("instances-avg", "平均");
    zh.insert("instances-running", "运行中...");
    zh.insert("instances-run", "▶ 运行");
    zh.insert("instances-delete", "删除");
    zh.insert("instances-empty-title", "暂无实例");
    zh.insert("instances-empty-desc", "创建一个新实例以将技能绑定到智能体");
    zh.insert("instances-error-title", "加载实例失败");

    // ==================== treasury.rs ====================
    zh.insert("treasury-page-title", "金库 - BeeBotOS");
    zh.insert("treasury-tx-page-title", "金库交易 - BeeBotOS");
    zh.insert("treasury-transfer-title", "转账");
    zh.insert("treasury-transfer-to", "接收地址");
    zh.insert("treasury-transfer-amount", "金额（wei）");
    zh.insert("treasury-transfer-submitting", "提交中...");
    zh.insert("treasury-transfer-submit", "提交转账");
    zh.insert("treasury-transfer-required", "地址和金额必填");
    zh.insert("treasury-transfer-submitted", "转账已提交");
    zh.insert("treasury-transfer-failed", "转账失败");
    zh.insert("treasury-total-balance", "金库总余额");
    zh.insert("treasury-assets", "资产");
    zh.insert("treasury-tokens", "个代币");
    zh.insert("treasury-transactions", "最近交易");
    zh.insert("treasury-view-all", "查看全部 →");
    zh.insert("treasury-deposit", "存入");
    zh.insert("treasury-withdraw", "提取");
    zh.insert("treasury-transfer", "转账");
    zh.insert("treasury-about-title", "关于金库");
    zh.insert("treasury-about-multisig", "多签保护");
    zh.insert("treasury-about-multisig-desc", "所有提款都需要 DAO 理事会成员的多重签名");
    zh.insert("treasury-about-transparent", "透明");
    zh.insert("treasury-about-transparent-desc", "所有交易都记录在链上，可公开验证");
    zh.insert("treasury-about-governance", "治理控制");
    zh.insert("treasury-about-governance-desc", "重大分配需要通过 DAO 提案的社区投票");
    zh.insert("treasury-no-assets", "金库中暂无资产");
    zh.insert("treasury-first-deposit", "首次存入");
    zh.insert("treasury-no-transactions", "暂无最近交易");
    zh.insert("treasury-error-title", "加载金库失败");
    zh.insert("treasury-error-retry", "重试");
    zh.insert("treasury-tx-title", "交易历史");
    zh.insert("treasury-tx-desc", "所有金库交易都记录在链上");
    zh.insert("treasury-all-transactions", "所有交易");
    zh.insert("treasury-tx-total", "总计");

    // ==================== webchat.rs ====================
    zh.insert("webchat-page-title", "聊天 - BeeBotOS");
    zh.insert("webchat-new-session", "新会话");
    zh.insert("webchat-default-title", "聊天会话");
    zh.insert("webchat-load-messages-failed", "加载消息失败");
    zh.insert("webchat-create-session-failed", "创建会话失败");
    zh.insert("webchat-load-sessions-failed", "加载会话失败");
    zh.insert("webchat-ws-error", "WebSocket 连接错误");
    zh.insert("webchat-send-failed", "发送失败");
    zh.insert("webchat-input-placeholder", "输入消息...（使用 /btw 进行侧边提问）");
    zh.insert("webchat-sessions-title", "会话");
    zh.insert("webchat-new-chat", "+ 新建聊天");
    zh.insert("webchat-search-sessions", "搜索会话...");

    translations.insert("zh-CN", zh);

    // English translations
    let mut en = HashMap::new();
    en.insert(
        "app-title",
        "BeeBotOS - Web4.0 Autonomous Agent Operating System",
    );
    en.insert(
        "app-description",
        "The Operating System for Autonomous AI Agents",
    );
    en.insert("nav-home", "Home");
    en.insert("nav-agents", "Agents");
    en.insert("nav-dao", "DAO");
    en.insert("nav-treasury", "Treasury");
    en.insert("nav-skills", "Skills");
    en.insert("nav-skill-instances", "Instances");
    en.insert("nav-channels", "Channels");
    en.insert("nav-settings", "Settings");
    en.insert("nav-models", "Models");
    en.insert("nav-llm-config", "LLM Config");
    en.insert("nav-chat", "Chat");
    en.insert("nav-browser", "Browser");
    en.insert("action-get-started", "Get Started");
    en.insert("action-browse-skills", "Browse Skills");
    en.insert("action-create", "Create");
    en.insert("action-view", "View");
    en.insert("action-browse", "Browse");
    en.insert("action-save", "Save");
    en.insert("action-cancel", "Cancel");
    en.insert("action-delete", "Delete");
    en.insert("action-edit", "Edit");
    en.insert("action-submit", "Submit");
    en.insert("action-refresh", "Refresh");
    en.insert("action-loading", "Loading...");
    en.insert("action-back", "Back");
    en.insert("action-close", "Close");
    en.insert("action-search", "Search");
    en.insert("action-filter", "Filter");
    en.insert("action-install", "Install");
    en.insert("action-uninstall", "Uninstall");
    en.insert("action-enable", "Enable");
    en.insert("action-disable", "Disable");
    en.insert("action-login", "Login");
    en.insert("action-logout", "Logout");
    en.insert("action-register", "Register");
    // Login page
    en.insert("login-title", "Welcome Back");
    en.insert("login-subtitle", "Sign in to your BeeBotOS account");
    en.insert("login-username", "Username");
    en.insert("login-username-placeholder", "Enter your username");
    en.insert("login-password", "Password");
    en.insert("login-password-placeholder", "Enter your password");
    en.insert("login-error-empty", "Username and password cannot be empty");
    en.insert("login-error-failed", "Login failed");
    en.insert("login-or", "OR");
    en.insert("login-demo-button", "Demo Login");
    en.insert("login-no-account", "Don't have an account?");
    en.insert("login-register-link", "Register now");
    en.insert(
        "login-demo-hint",
        "Demo mode: Enter any username and password to login",
    );
    // Register page
    en.insert("register-title", "Create Account");
    en.insert(
        "register-subtitle",
        "Register a BeeBotOS account to get started",
    );
    en.insert("register-username", "Username");
    en.insert("register-username-placeholder", "Enter your username");
    en.insert("register-email", "Email");
    en.insert("register-email-placeholder", "Enter your email (optional)");
    en.insert("register-password", "Password");
    en.insert(
        "register-password-placeholder",
        "Enter password (at least 6 characters)",
    );
    en.insert("register-confirm-password", "Confirm Password");
    en.insert(
        "register-confirm-password-placeholder",
        "Enter password again",
    );
    en.insert(
        "register-error-empty",
        "Username and password cannot be empty",
    );
    en.insert("register-error-password-mismatch", "Passwords do not match");
    en.insert(
        "register-error-password-short",
        "Password must be at least 6 characters",
    );
    en.insert("register-error-failed", "Registration failed");
    en.insert("register-or", "OR");
    en.insert("register-demo-button", "Demo Register");
    en.insert("register-have-account", "Already have an account?");
    en.insert("register-login-link", "Login now");
    en.insert(
        "hero-title",
        "The Operating System for Autonomous AI Agents",
    );
    en.insert(
        "hero-subtitle",
        "Build, deploy, and manage intelligent agents with built-in governance",
    );
    en.insert("hero-cta-primary", "Get Started");
    en.insert("hero-cta-secondary", "Browse Skills");
    en.insert("features-title", "Core Features");
    en.insert("feature-agents-title", "Autonomous Agents");
    en.insert(
        "feature-agents-desc",
        "Deploy AI agents that operate independently with built-in safety controls",
    );
    en.insert("feature-dao-title", "DAO Governance");
    en.insert(
        "feature-dao-desc",
        "Community-driven decision making with transparent voting mechanisms",
    );
    en.insert("feature-treasury-title", "Secure Treasury");
    en.insert(
        "feature-treasury-desc",
        "Multi-sig treasury management with on-chain transparency",
    );
    en.insert("feature-skills-title", "Skill Marketplace");
    en.insert(
        "feature-skills-desc",
        "Extend agent capabilities with community-built skills",
    );
    en.insert("feature-wasm-title", "WebAssembly Runtime");
    en.insert(
        "feature-wasm-desc",
        "High-performance, sandboxed execution environment",
    );
    en.insert("feature-analytics-title", "Real-time Analytics");
    en.insert(
        "feature-analytics-desc",
        "Monitor agent performance and system health in real-time",
    );
    en.insert("quick-actions-title", "Quick Actions");
    en.insert("quick-action-create-agent-title", "Create Agent");
    en.insert(
        "quick-action-create-agent-desc",
        "Set up a new autonomous agent",
    );
    en.insert("quick-action-view-proposals-title", "View Proposals");
    en.insert(
        "quick-action-view-proposals-desc",
        "Participate in DAO governance",
    );
    en.insert("quick-action-install-skills-title", "Install Skills");
    en.insert(
        "quick-action-install-skills-desc",
        "Add capabilities to your agents",
    );
    en.insert("agents-title", "Agents");
    en.insert("agents-subtitle", "Manage your autonomous AI agents");
    en.insert("agents-create-new", "Create New Agent");
    en.insert("agents-no-agents", "No agents found");
    en.insert("agents-loading", "Loading agents...");
    en.insert("agents-error", "Failed to load agents");
    en.insert("status-active", "Active");
    en.insert("status-idle", "Idle");
    en.insert("status-paused", "Paused");
    en.insert("status-error", "Error");
    en.insert("status-offline", "Offline");
    en.insert("status-running", "Running");
    en.insert("status-completed", "Completed");
    en.insert("status-pending", "Pending");
    // Channels
    en.insert("channels-title", "Channel Management");
    en.insert(
        "channels-subtitle",
        "Configure and manage message channel connections",
    );
    en.insert("channel-status", "Channel Status");
    en.insert("channel-config", "Channel Configuration");
    en.insert("status-enabled", "Enabled");
    en.insert("status-disabled", "Disabled");
    en.insert("wechat-login", "WeChat Login");
    en.insert(
        "wechat-login-hint",
        "Scan QR code with WeChat to get Bot Token",
    );
    en.insert("qr-expires-in", "QR expires in");
    en.insert("action-get-qr", "Get QR Code");
    en.insert("action-refresh-qr", "Refresh QR Code");
    en.insert("action-test", "Test Connection");
    en.insert("config-base-url", "Base URL");
    en.insert("config-bot-token", "Bot Token");
    en.insert("config-auto-reconnect", "Auto Reconnect");
    en.insert("scan-qr", "Scan QR");
    en.insert("click-open-wechat-scan", "Click to open WeChat scan page");
    en.insert("wechat-scan-hint", "Please scan the QR code in the page with WeChat");
    en.insert("qr-code-label", "QR Code");
    en.insert("qr-status-confirmed", "Scan successful, login complete");
    en.insert("qr-status-scanned", "Scanned, waiting for confirmation");
    en.insert("qr-status-expired", "QR code expired, please refresh");
    en.insert("qr-status-waiting", "Waiting for scan...");
    en.insert("poll-qr-failed", "Failed to poll QR code status");
    en.insert("get-qr-failed", "Failed to get QR code");
    en.insert("config-enabled", "Enabled");
    en.insert("channel-enabled-msg", "Channel enabled");
    en.insert("channel-disabled-msg", "Channel disabled");
    en.insert("connection-test-passed", "Connection test passed");
    en.insert("connection-test-failed", "Connection test failed");
    en.insert("test-failed", "Test failed");
    en.insert("config-saved", "Configuration saved");
    en.insert("save-failed", "Save failed");

    en.insert("dao-title", "DAO Governance");
    en.insert("dao-subtitle", "Participate in community decision-making");
    en.insert("dao-active-proposals", "Active Proposals");
    en.insert("dao-completed-proposals", "Completed Proposals");
    en.insert("dao-create-proposal", "Create Proposal");
    en.insert("dao-vote-for", "Vote For");
    en.insert("dao-vote-against", "Vote Against");
    en.insert("dao-votes-for", "For");
    en.insert("dao-votes-against", "Against");
    en.insert("dao-voting-ends", "Voting ends");
    en.insert("dao-executed", "Executed");
    en.insert("treasury-title", "Treasury");
    en.insert("treasury-subtitle", "Manage DAO assets and transactions");
    en.insert("treasury-total-balance", "Total Balance");
    en.insert("treasury-assets", "Assets");
    en.insert("treasury-transactions", "Transactions");
    en.insert("treasury-deposit", "Deposit");
    en.insert("treasury-withdraw", "Withdraw");
    en.insert("skills-title", "Skill Marketplace");
    en.insert("skills-subtitle", "Discover and install agent capabilities");
    en.insert("skills-categories", "Categories");
    en.insert("skills-installed", "Installed");
    en.insert("skills-available", "Available");
    en.insert("skills-search-placeholder", "Search skills...");
    en.insert("settings-title", "Settings");
    en.insert("settings-subtitle", "Configure your BeeBotOS instance");
    en.insert("settings-general", "General");
    en.insert("settings-appearance", "Appearance");
    en.insert("settings-language", "Language");
    en.insert("settings-theme", "Theme");
    en.insert("theme-light", "Light");
    en.insert("theme-dark", "Dark");
    en.insert("theme-system", "System");
    en.insert("settings-notifications", "Notifications");
    en.insert("settings-security", "Security");
    en.insert("settings-wallet", "Wallet");
    en.insert("settings-system", "System Info");
    en.insert("footer-copyright", "© 2026 BeeBotOS. All rights reserved.");
    en.insert("footer-version", "Version");
    en.insert("error-404-title", "404");
    en.insert("error-404-message", "Page not found");
    en.insert(
        "error-404-description",
        "The page you're looking for doesn't exist or has been moved.",
    );
    en.insert("error-go-home", "Go Home");
    en.insert("error-generic", "Something went wrong");
    en.insert("error-retry", "Try Again");
    en.insert("notification-success", "Success");
    en.insert("notification-error", "Error");
    en.insert("notification-warning", "Warning");
    en.insert("notification-info", "Info");
    en.insert("logout-success", "You have been successfully logged out");
    en.insert("stats-tasks", "Tasks Completed");
    en.insert("stats-uptime", "System Uptime");
    en.insert("stats-members", "Community Members");
    en.insert("quick-action-start-chat-title", "Start Chat");
    en.insert(
        "quick-action-start-chat-desc",
        "Start a conversation with AI agents",
    );
    en.insert("nav-section-chat", "Chat");
    en.insert("nav-section-control", "Control");
    en.insert("nav-section-agents", "Agents");
    en.insert("nav-section-settings", "Settings");
    en.insert("footer-product", "Product");
    en.insert("footer-resources", "Resources");
    en.insert("footer-community", "Community");

    // Components - loading
    en.insert("loading-page", "Loading...");
    en.insert("loading-inline", "Loading...");
    en.insert("loading-more", "Loading more...");
    // Components - error boundary
    en.insert("error-refresh-hint", "Please refresh the page and try again.");
    en.insert("error-async-handler", "AsyncHandler not supported in CSR mode - use LocalResource");
    // Components - guard
    en.insert("auth-checking", "Checking authentication...");
    en.insert("redirecting", "Redirecting...");
    en.insert("error-access-denied", "Access Denied");
    en.insert("error-access-denied-desc", "You don't have permission to access this page.");
    en.insert("contact-support", "Contact Support");
    // Components - footer
    en.insert("footer-openclaw", "OpenClaw v2026.3.13");
    en.insert("footer-web4-ready", "Web4.0 Ready");
    // Components - command palette
    en.insert("cmd-palette-hint", "Press Ctrl+K for commands");
    en.insert("cmd-palette-placeholder", "Type a command or search...");
    en.insert("cmd-palette-empty", "No commands found");
    en.insert("cmd-palette-open", "Open Command Palette");
    // Components - chat search
    en.insert("search-messages-placeholder", "Search messages...");
    en.insert("search-results", "results");
    en.insert("search-in", "in");
    en.insert("filter-from-date", "From Date");
    en.insert("filter-to-date", "To Date");
    en.insert("filter-sender", "Sender");
    en.insert("filter-sender-placeholder", "Username");
    en.insert("filter-channel", "Channel");
    en.insert("filter-channel-placeholder", "Channel name");
    en.insert("filter-has-attachments", "Has attachments only");
    en.insert("export-messages-title", "Export Messages");
    en.insert("export-format", "Export Format");
    en.insert("format-plain-text", "Plain Text");
    en.insert("date-range", "Date Range");
    en.insert("range-all-messages", "All messages");
    en.insert("range-today", "Today");
    en.insert("range-last-7-days", "Last 7 days");
    en.insert("range-last-30-days", "Last 30 days");
    en.insert("range-custom", "Custom range");
    en.insert("include-attachments", "Include attachments");
    en.insert("action-export", "Export");
    en.insert("pinned-messages", "Pinned Messages");
    en.insert("jump-to-message", "Jump to message");
    en.insert("unpin-message", "Unpin");
    en.insert("slash-command-placeholder", "Type / for commands...");
    // Components - pagination
    en.insert("pagination-previous", "Previous");
    en.insert("pagination-next", "Next");
    en.insert("pagination-page", "Page");
    en.insert("pagination-of", "of");
    en.insert("pagination-items", "items");
    en.insert("page-size-show", "Show:");
    en.insert("page-size-per-page", "per page");
    // Components - sidebar
    en.insert("user-status-online", "Online");

    // WebChat components
    en.insert("message-input-placeholder", "Type a message...");
    en.insert("message-input-hint-send", "Press Enter to send, Shift+Enter for new line");
    en.insert("message-input-hint-btw", "Use /btw for side question");
    en.insert("session-list-title", "Sessions");
    en.insert("session-list-new", "+ New");
    en.insert("session-list-empty-title", "No sessions yet");
    en.insert("session-list-empty-hint", "Click 'New' to start chatting");
    en.insert("session-messages-count", "{} messages");
    en.insert("side-panel-title", "Side Questions");
    en.insert("side-panel-empty-title", "No side questions yet");
    en.insert("side-panel-empty-hint", "Use /btw to ask a side question");
    en.insert("side-panel-input-placeholder", "Ask a side question...");
    en.insert("side-panel-ask-button", "Ask");
    en.insert("usage-panel-title", "Token Usage");
    en.insert("usage-panel-session", "Session");
    en.insert("usage-panel-daily", "Daily");
    en.insert("usage-panel-monthly", "Monthly");
    en.insert("usage-panel-limits", "Limits");
    en.insert("usage-panel-daily-remaining", "Daily Remaining:");
    en.insert("usage-panel-monthly-remaining", "Monthly Remaining:");
    en.insert("usage-panel-approaching-limit", "⚠️ Approaching limit");
    en.insert("usage-panel-prompt", "Prompt: {}");
    en.insert("usage-panel-completion", "Completion: {}");

    // Browser components
    en.insert("browser-url-placeholder", "Enter URL...");
    en.insert("browser-go", "Go");
    en.insert("browser-loading", "Loading...");
    en.insert("browser-not-connected", "No Browser Connected");
    en.insert("browser-select-profile", "Select a profile to connect");
    en.insert("debug-console-title", "Debug Console");
    en.insert("debug-console-clear", "Clear");
    en.insert("debug-console-export", "Export");
    en.insert("debug-console-empty", "No logs to display");
    en.insert("debug-console-filter-level", "Filter Level:");
    en.insert("debug-console-filter-all", "All");
    en.insert("debug-console-filter-debug", "Debug");
    en.insert("debug-console-filter-info", "Info");
    en.insert("debug-console-filter-warning", "Warning");
    en.insert("debug-console-filter-error", "Error");
    en.insert("debug-console-filter-critical", "Critical");
    en.insert("profile-cdp-port", "CDP Port:");
    en.insert("profile-status", "Status:");
    en.insert("profile-connect", "Connect");
    en.insert("profile-disconnect", "Disconnect");
    en.insert("profile-edit", "Edit");
    en.insert("sandbox-cdp-port", "CDP Port:");
    en.insert("sandbox-isolation", "Isolation:");
    en.insert("sandbox-memory", "Memory:");
    en.insert("sandbox-start", "Start");
    en.insert("sandbox-stop", "Stop");
    en.insert("sandbox-delete", "Delete");

    // Wizard components
    en.insert("wizard-step-welcome", "Welcome");
    en.insert("wizard-step-server", "Server");
    en.insert("wizard-step-database", "Database");
    en.insert("wizard-step-security", "Security");
    en.insert("wizard-step-llm", "LLM Models");
    en.insert("wizard-step-channels", "Channels");
    en.insert("wizard-step-blockchain", "Blockchain");
    en.insert("wizard-step-logging", "Logging");
    en.insert("wizard-step-review", "Review");
    en.insert("wizard-step-deploy", "Deploy");
    en.insert("wizard-back", "← Back");
    en.insert("wizard-next", "Next →");
    en.insert("wizard-step-indicator", "Step {current} of {total}");
    en.insert("wizard-deploying", "Deploying...");
    en.insert("wizard-save-export", "Save & Export ▼");
    en.insert("secret-input-placeholder", "Enter secret...");
    en.insert("secret-input-show", "Show");
    en.insert("secret-input-hide", "Hide");
    en.insert("secret-input-generate", "Generate");
    en.insert("provider-api-key", "API Key");
    en.insert("provider-model", "Model");
    en.insert("provider-base-url", "Base URL");
    en.insert("provider-temperature", "Temperature");
    en.insert("provider-context-window", "Context Window");
    en.insert("provider-default", "Default");
    en.insert("platform-enabled", "Enabled");
    en.insert("platform-disabled", "Disabled");
    en.insert("platform-config-hint", "Configure {} settings in beebotos.toml");

    // Agents page
    en.insert("agents-create-first", "Create your first autonomous agent to get started");
    en.insert("action-manage", "Manage");
    en.insert("action-start", "Start");
    en.insert("action-stop", "Stop");
    en.insert("action-starting", "Starting...");
    en.insert("action-stopping", "Stopping...");
    en.insert("action-creating", "Creating...");
    en.insert("action-saving", "Saving...");
    en.insert("action-refresh-page", "Refresh Page");
    en.insert("modal-create-agent-title", "Create New Agent");
    en.insert("label-agent-name", "Agent Name *");
    en.insert("placeholder-agent-name", "Enter agent name");
    en.insert("label-description", "Description");
    en.insert("placeholder-agent-description", "Enter agent description");
    en.insert("label-model-provider", "Model Provider");
    en.insert("label-model-name", "Model Name");
    en.insert("placeholder-model-name", "e.g. gpt-4, claude-3-opus-20240229");
    en.insert("notification-agent-started", "Agent Started");
    en.insert("notification-agent-started-desc", "Agent has been started");
    en.insert("notification-start-failed", "Start Failed");
    en.insert("notification-start-failed-desc", "Failed to start agent");
    en.insert("notification-agent-stopped", "Agent Stopped");
    en.insert("notification-agent-stopped-desc", "Agent has been stopped");
    en.insert("notification-stop-failed", "Stop Failed");
    en.insert("notification-stop-failed-desc", "Failed to stop agent");

    // Agent detail page
    en.insert("agent-detail-title", "Agent Details");
    en.insert("agent-detail-id-required", "Agent ID is required");
    en.insert("modal-edit-agent-title", "Edit Agent");
    en.insert("modal-configure-agent-title", "Configure Agent");
    en.insert("modal-agent-logs-title", "Agent Logs");
    en.insert("modal-confirm-delete-title", "Confirm Delete");
    en.insert("label-name", "Name");
    en.insert("label-clone", "Clone");
    en.insert("notification-agent-cloned", "Agent Cloned");
    en.insert("notification-agent-cloned-desc", "Agent cloned successfully");
    en.insert("notification-clone-failed", "Clone Failed");
    en.insert("notification-clone-failed-desc", "Failed to clone agent");
    en.insert("notification-update-failed", "Update failed");
    en.insert("notification-delete-failed", "Delete failed");
    en.insert("agent-config-hint", "Agent-level configuration options will be available here. Global model settings can be configured in LLM Configuration.");
    en.insert("action-go-to-llm-config", "Go to LLM Config");
    en.insert("logs-loading", "Loading logs...");
    en.insert("logs-empty", "No logs available");
    en.insert("delete-confirm-prefix", "Are you sure you want to delete '");
    en.insert("delete-confirm-suffix", "'");
    en.insert("delete-confirm-irreversible", "This action cannot be undone.");
    en.insert("section-overview", "Overview");
    en.insert("section-capabilities", "Capabilities");
    en.insert("section-recent-activity", "Recent Activity");
    en.insert("section-quick-stats", "Quick Stats");
    en.insert("section-actions", "Actions");
    en.insert("label-agent-id", "Agent ID");
    en.insert("label-created", "Created");
    en.insert("label-updated", "Updated");
    en.insert("label-tasks-completed", "Tasks Completed");
    en.insert("label-uptime", "Uptime");
    en.insert("label-unknown", "Unknown");
    en.insert("label-status", "Status");
    en.insert("label-tasks", "Tasks");
    en.insert("capabilities-empty", "No capabilities configured");
    en.insert("action-view-logs", "View Logs");
    en.insert("action-configure", "Configure");
    en.insert("action-clone-agent", "Clone Agent");
    en.insert("action-export-config", "Export Config");
    en.insert("action-back-to-agents", "Back to Agents");
    en.insert("agent-not-found", "Agent Not Found");
    en.insert("activity-loading", "Loading activity...");
    en.insert("activity-empty", "No recent activity");
    en.insert("activity-load-error", "Unable to load activity logs");

    // Browser page
    en.insert("browser-title", "Browser Automation");
    en.insert("browser-subtitle", "Chrome DevTools MCP Control - Compatible with OpenClaw V2026.3.13");
    en.insert("browser-profiles", "Profiles");
    en.insert("browser-add-profile", "Add Profile");
    en.insert("browser-sandboxes", "Sandboxes");
    en.insert("browser-create-sandbox", "Create Sandbox");
    en.insert("browser-debug-hidden", "Debug panel hidden");
    en.insert("browser-url-placeholder", "Enter URL...");
    en.insert("browser-go", "Go");
    en.insert("browser-connecting", "Connecting...");
    en.insert("browser-connection-failed", "Connection failed");
    en.insert("browser-no-connection", "No browser connected");
    en.insert("browser-select-profile", "Select a profile to connect");
    en.insert("browser-debug-console", "Debug Console");
    en.insert("browser-clear", "Clear");
    en.insert("browser-debug-placeholder", "Debug logs will appear here...");
    en.insert("modal-add-profile-title", "Add Browser Profile");
    en.insert("label-profile-name", "Profile Name");
    en.insert("placeholder-profile-name", "e.g. Work Profile");
    en.insert("label-cdp-port", "CDP Port");
    en.insert("modal-create-sandbox-title", "Create Sandbox");
    en.insert("label-sandbox-name", "Sandbox Name");
    en.insert("placeholder-sandbox-name", "e.g. Test Sandbox");
    en.insert("label-base-profile", "Base Profile");

    // DAO page
    en.insert("dao-view-treasury", "View Treasury →");
    en.insert("dao-governance-proposals", "Governance Proposals");
    en.insert("dao-members", "DAO Members");
    en.insert("dao-your-voting-power", "Your Voting Power");
    en.insert("dao-your-balance", "Your Balance");
    en.insert("dao-past-proposals", "Past Proposals");
    en.insert("dao-create-failed", "Failed to create proposal");
    en.insert("dao-load-proposals-failed", "Failed to load proposals");
    en.insert("modal-create-proposal-title", "Create Proposal");
    en.insert("label-title", "Title");
    en.insert("placeholder-proposal-title", "Proposal title");
    en.insert("placeholder-proposal-desc", "Describe your proposal...");
    en.insert("label-type", "Type");
    en.insert("proposal-type-general", "General");
    en.insert("proposal-type-funding", "Funding");
    en.insert("proposal-type-upgrade", "Upgrade");
    en.insert("proposal-type-parameter", "Parameter");
    en.insert("proposal-by", "By ");
    en.insert("proposals-empty-title", "No proposals yet");
    en.insert("proposals-empty-desc", "Be the first to create a governance proposal");
    en.insert("voted-for", "You voted For");
    en.insert("voted-against", "You voted Against");
    en.insert("action-voting", "Voting...");
    en.insert("notification-vote-submitted", "Vote Submitted");
    en.insert("notification-vote-submitted-desc", "Your vote has been recorded successfully");
    en.insert("notification-vote-failed", "Vote Failed");
    en.insert("notification-vote-failed-desc", "Failed to submit vote");

    // LLM Config page
    en.insert("llm-config-title", "LLM Configuration");
    en.insert("llm-config-subtitle", "Global LLM settings and real-time monitoring");
    en.insert("llm-global-config", "Global Configuration");
    en.insert("llm-default-provider", "Default Provider");
    en.insert("llm-max-tokens", "Max Tokens");
    en.insert("llm-request-timeout", "Request Timeout");
    en.insert("llm-cost-optimization", "Cost Optimization");
    en.insert("llm-enabled", "Enabled");
    en.insert("llm-disabled", "Disabled");
    en.insert("llm-fallback-chain", "Fallback Chain");
    en.insert("llm-system-prompt", "System Prompt");
    en.insert("llm-providers", "Providers");
    en.insert("llm-healthy", "Healthy");
    en.insert("llm-failures", "failures");
    en.insert("llm-model", "Model");
    en.insert("llm-base-url", "Base URL");
    en.insert("llm-api-key", "API Key");
    en.insert("llm-temperature", "Temperature");
    en.insert("llm-context-window", "Context Window");
    en.insert("llm-default", "Default");
    en.insert("llm-realtime-metrics", "Real-time Metrics");
    en.insert("llm-last-updated", "Last updated");
    en.insert("llm-total-requests", "Total Requests");
    en.insert("llm-success-rate", "success");
    en.insert("llm-successful", "Successful");
    en.insert("llm-failed", "Failed");
    en.insert("llm-total-tokens", "Total Tokens");
    en.insert("llm-input", "Input");
    en.insert("llm-output", "Output");
    en.insert("llm-latency", "Latency");
    en.insert("llm-avg", "Avg");
    en.insert("llm-p50", "P50");
    en.insert("llm-p95", "P95");
    en.insert("llm-p99", "P99");
    en.insert("llm-visual-overview", "Visual Overview");
    en.insert("llm-request-distribution", "Request Distribution");
    en.insert("llm-token-usage", "Token Usage");
    en.insert("llm-latency-percentiles", "Latency Percentiles (ms)");
    en.insert("llm-success", "Success");

    // ==================== settings.rs ====================
    en.insert("settings-page-title", "Settings - BeeBotOS");
    en.insert("settings-heading", "Settings");
    en.insert("settings-description", "Manage your preferences and system configuration");
    en.insert("settings-loading", "Loading settings...");
    en.insert("settings-save-success", "Settings saved successfully");
    en.insert("settings-save-local", "Saved locally");
    en.insert("settings-appearance", "Appearance");
    en.insert("settings-theme", "Theme");
    en.insert("settings-language", "Language");
    en.insert("lang-en", "English");
    en.insert("lang-zh", "中文");
    en.insert("lang-ja", "日本語");
    en.insert("lang-ko", "한국어");
    en.insert("settings-notifications", "Notifications");
    en.insert("settings-enable-notifications", "Enable notifications");
    en.insert("settings-notifications-help", "Receive alerts about agent status and DAO governance");
    en.insert("settings-auto-update", "Auto-update");
    en.insert("settings-auto-update-help", "Automatically update to the latest version");
    en.insert("settings-network", "Network");
    en.insert("settings-api-endpoint", "API Endpoint");
    en.insert("settings-api-endpoint-help", "Custom API endpoint (leave empty for default)");
    en.insert("settings-wallet", "Wallet");
    en.insert("settings-wallet-address", "Wallet Address");
    en.insert("settings-wallet-help", "Your wallet address for DAO participation");
    en.insert("settings-connect-wallet", "Connect Wallet");
    en.insert("settings-disconnect-wallet", "Disconnect");
    en.insert("settings-ai-config", "AI Configuration");
    en.insert("settings-ai-config-help", "View global LLM provider settings and metrics");
    en.insert("settings-open-llm-config", "Open LLM Configuration →");
    en.insert("settings-gateway-setup", "Gateway Setup");
    en.insert("settings-gateway-setup-help", "Run the configuration wizard to setup or reconfigure Gateway");
    en.insert("settings-config-wizard", "Configuration Wizard →");
    en.insert("settings-system", "System");
    en.insert("settings-version", "Version");
    en.insert("settings-build", "Build");
    en.insert("settings-platform", "Platform");
    en.insert("settings-check-updates", "Check for Updates");
    en.insert("settings-reload-config", "Reload Config");
    en.insert("settings-reload-failed", "Reload failed");
    en.insert("settings-reset-defaults", "Reset to Defaults");
    en.insert("settings-saving", "Saving...");
    en.insert("settings-save-changes", "Save Changes");

    // ==================== setup.rs ====================
    en.insert("setup-page-title", "Gateway Setup - BeeBotOS");
    en.insert("setup-welcome-title", "BeeBotOS Gateway Setup");
    en.insert("setup-welcome-subtitle", "Configure your Gateway in a few simple steps");
    en.insert("setup-mode-fresh", "Start Fresh");
    en.insert("setup-mode-fresh-desc", "Create a new configuration from scratch");
    en.insert("setup-mode-minimal", "Minimal");
    en.insert("setup-mode-minimal-desc", "SQLite + Kimi + WebChat — for local testing");
    en.insert("setup-mode-standard", "Standard");
    en.insert("setup-mode-standard-desc", "Multi-provider + 5 channels — for production");
    en.insert("setup-mode-enterprise", "Enterprise");
    en.insert("setup-mode-enterprise-desc", "Postgres + TLS + OTLP — full stack");
    en.insert("setup-server-title", "Server Configuration");
    en.insert("setup-server-desc", "Configure HTTP/gRPC server settings");
    en.insert("setup-server-host", "Host");
    en.insert("setup-server-http-port", "HTTP Port");
    en.insert("setup-server-grpc-port", "gRPC Port");
    en.insert("setup-server-timeout", "Request Timeout (seconds)");
    en.insert("setup-server-max-body", "Max Body Size (MB)");
    en.insert("setup-server-cors", "CORS Origins (comma-separated)");
    en.insert("setup-server-advanced-tls", "Advanced TLS Options");
    en.insert("setup-server-enable-tls", "Enable TLS");
    en.insert("setup-server-tls-cert", "TLS Cert Path");
    en.insert("setup-server-tls-key", "TLS Key Path");
    en.insert("setup-server-enable-mtls", "Enable mTLS");
    en.insert("setup-db-title", "Database Configuration");
    en.insert("setup-db-desc", "Choose your database engine");
    en.insert("setup-db-type", "Database Type");
    en.insert("setup-db-sqlite", "SQLite");
    en.insert("setup-db-postgres", "PostgreSQL");
    en.insert("setup-db-sqlite-path", "SQLite Path");
    en.insert("setup-db-postgres-url", "PostgreSQL URL");
    en.insert("setup-db-max-conn", "Max Connections");
    en.insert("setup-db-min-conn", "Min Connections");
    en.insert("setup-db-connect-timeout", "Connect Timeout (seconds)");
    en.insert("setup-db-auto-migrate", "Auto Migrate on Startup");
    en.insert("setup-security-title", "JWT & Security");
    en.insert("setup-security-desc", "Configure authentication and rate limiting");
    en.insert("setup-security-jwt-secret", "JWT Secret");
    en.insert("setup-security-jwt-placeholder", "Min 32 characters");
    en.insert("setup-security-jwt-help", "Used to sign JWT tokens. Keep this secure!");
    en.insert("setup-security-token-expiry", "Token Expiry (seconds)");
    en.insert("setup-security-refresh-expiry", "Refresh Token Expiry (seconds)");
    en.insert("setup-security-enable-rate-limit", "Enable Rate Limiting");
    en.insert("setup-security-qps-limit", "QPS Limit");
    en.insert("setup-security-burst-limit", "Burst Limit");
    en.insert("setup-llm-title", "LLM Models Configuration");
    en.insert("setup-llm-desc", "Configure AI providers and fallback chain");
    en.insert("setup-llm-default-provider", "Default Provider");
    en.insert("setup-llm-max-tokens", "Max Tokens");
    en.insert("setup-llm-timeout", "Request Timeout (seconds)");
    en.insert("setup-llm-cost-opt", "Enable Cost Optimization");
    en.insert("setup-llm-system-prompt", "System Prompt");
    en.insert("setup-llm-providers", "Providers");
    en.insert("setup-llm-api-key", "API Key");
    en.insert("setup-llm-model", "Model");
    en.insert("setup-llm-base-url", "Base URL");
    en.insert("setup-llm-temperature", "Temperature");
    en.insert("setup-llm-context-window", "Context Window");
    en.insert("setup-llm-provider-placeholder", "Provider name (e.g. kimi, openai)");
    en.insert("setup-llm-add-provider", "+ Add Provider");
    en.insert("setup-channels-title", "Channels Configuration");
    en.insert("setup-channels-desc", "Enable and configure communication platforms");
    en.insert("setup-channels-context-window", "Context Window (messages)");
    en.insert("setup-channels-max-file", "Max File Size (MB)");
    en.insert("setup-channels-default-agent", "Default Agent ID");
    en.insert("setup-channels-auto-download", "Auto Download Media");
    en.insert("setup-channels-auto-reply", "Auto Reply");
    en.insert("setup-channels-platforms", "Platforms");
    en.insert("setup-blockchain-title", "Blockchain Configuration");
    en.insert("setup-blockchain-desc", "Optional blockchain integration");
    en.insert("setup-blockchain-enable", "Enable Blockchain");
    en.insert("setup-blockchain-chain-id", "Chain ID");
    en.insert("setup-blockchain-rpc", "RPC URL");
    en.insert("setup-blockchain-mnemonic", "Wallet Mnemonic");
    en.insert("setup-blockchain-mnemonic-placeholder", "12 or 24 word mnemonic phrase");
    en.insert("setup-logging-title", "Logging & Observability");
    en.insert("setup-logging-desc", "Configure logs, metrics and tracing");
    en.insert("setup-logging-level", "Log Level");
    en.insert("setup-logging-format", "Log Format");
    en.insert("setup-logging-file-path", "Log File Path");
    en.insert("setup-logging-rotation", "Log Rotation");
    en.insert("setup-logging-enable-metrics", "Enable Metrics (Prometheus)");
    en.insert("setup-logging-metrics-port", "Metrics Port");
    en.insert("setup-logging-enable-tracing", "Enable OpenTelemetry Tracing");
    en.insert("setup-logging-otlp", "OTLP Endpoint");
    en.insert("setup-logging-sampling", "Trace Sampling Rate");
    en.insert("log-trace", "Trace");
    en.insert("log-debug", "Debug");
    en.insert("log-info", "Info");
    en.insert("log-warn", "Warn");
    en.insert("log-error", "Error");
    en.insert("log-json", "JSON");
    en.insert("log-pretty", "Pretty");
    en.insert("log-compact", "Compact");
    en.insert("log-minutely", "Minutely");
    en.insert("log-hourly", "Hourly");
    en.insert("log-daily", "Daily");
    en.insert("log-never", "Never");
    en.insert("setup-review-title", "Review Configuration");
    en.insert("setup-review-desc", "Verify your settings before deployment");
    en.insert("setup-review-warnings", "⚠️ Validation Warnings");
    en.insert("setup-review-success", "✅ All required fields filled");
    en.insert("setup-deploy-title", "Deploy Configuration");
    en.insert("setup-deploy-desc", "Export and apply your configuration");
    en.insert("setup-deploy-toml-desc", "Download beebotos.toml and place it in your config/ directory");
    en.insert("setup-deploy-toml-btn", "Download beebotos.toml");
    en.insert("setup-deploy-env-desc", "Export as .env file for Docker or CI/CD usage");
    en.insert("setup-deploy-env-btn", "Download .env");
    en.insert("setup-deploy-docker-desc", "Generate docker-compose.yml with your settings");
    en.insert("setup-deploy-docker-btn", "Download docker-compose.yml");
    en.insert("setup-deploy-k8s-desc", "Generate K8s Deployment and Service manifests");
    en.insert("setup-deploy-k8s-btn", "Download K8s Manifests");
    en.insert("setup-deploy-instructions-title", "Deployment Instructions");
    en.insert("setup-deploy-step1", "Download your preferred configuration format");
    en.insert("setup-deploy-step2", "Upload to your server");
    en.insert("setup-deploy-step3", "Place in the config/ directory (for TOML) or source the .env file");
    en.insert("setup-deploy-step4", "Restart the Gateway service");
    en.insert("setup-deploy-go-settings", "Go to Settings");

    // ==================== skills.rs ====================
    en.insert("skills-page-title", "Skills - BeeBotOS");
    en.insert("skills-source", "Source:");
    en.insert("skills-source-local", "Local");
    en.insert("skills-source-clawhub", "ClawHub");
    en.insert("skills-source-beehub", "BeeHub");
    en.insert("skills-search-btn", "🔍 Search");
    en.insert("skills-cat-all", "All");
    en.insert("skills-cat-trading", "Trading");
    en.insert("skills-cat-data", "Data");
    en.insert("skills-cat-social", "Social");
    en.insert("skills-cat-automation", "Automation");
    en.insert("skills-cat-analysis", "Analysis");
    en.insert("skills-installed-badge", "✓ Installed");
    en.insert("skills-btn-details", "Details");
    en.insert("skills-btn-install", "Install");
    en.insert("skills-btn-uninstall", "Uninstall");
    en.insert("skills-btn-view-hub", "View on Hub");
    en.insert("skills-installing", "Installing...");
    en.insert("skills-removing", "Removing...");
    en.insert("skills-install-success-title", "Skill Installed");
    en.insert("skills-install-success-msg", "installed successfully");
    en.insert("skills-install-fail-title", "Install Failed");
    en.insert("skills-install-fail-msg", "Failed to install");
    en.insert("skills-uninstall-success-title", "Skill Uninstalled");
    en.insert("skills-uninstall-success-msg", "removed successfully");
    en.insert("skills-uninstall-fail-title", "Uninstall Failed");
    en.insert("skills-uninstall-fail-msg", "Failed to uninstall");
    en.insert("skills-detail-version", "Version:");
    en.insert("skills-detail-author", "Author:");
    en.insert("skills-detail-license", "License:");
    en.insert("skills-detail-downloads", "Downloads:");
    en.insert("skills-detail-rating", "Rating:");
    en.insert("skills-detail-description", "Description:");
    en.insert("skills-detail-capabilities", "Capabilities:");
    en.insert("skills-detail-tags", "Tags:");
    en.insert("skills-detail-none", "None listed");
    en.insert("skills-empty-search", "Search {}");
    en.insert("skills-empty-search-desc", "Enter a keyword above to search for skills on this hub.");
    en.insert("skills-empty-noresults", "No results on {}");
    en.insert("skills-empty-noresults-desc", "Try a different search term or switch to Local skills.");
    en.insert("skills-empty-none", "No skills found");
    en.insert("skills-empty-none-desc", "Try adjusting your search or filters");
    en.insert("skills-error-title", "Failed to load skills");
    en.insert("skills-error-unavailable", "The skill hub is currently unreachable.");
    en.insert("skills-error-unavailable-hint", "Please switch to Local skills or check Gateway network configuration.");
    en.insert("skills-error-retry", "Retry");

    // ==================== skill_instances.rs ====================
    en.insert("instances-page-title", "Skill Instances - BeeBotOS");
    en.insert("instances-title", "Skill Instances");
    en.insert("instances-subtitle", "Manage skill instances bound to your agents");
    en.insert("instances-cancel", "✕ Cancel");
    en.insert("instances-new", "+ New Instance");
    en.insert("instances-create-title", "Create Instance");
    en.insert("instances-skill-id", "Skill ID");
    en.insert("instances-skill-id-placeholder", "e.g. echo-skill");
    en.insert("instances-agent-id", "Agent ID");
    en.insert("instances-agent-id-placeholder", "e.g. agent-001");
    en.insert("instances-creating", "Creating...");
    en.insert("instances-create-btn", "Create Instance");
    en.insert("instances-missing-fields-title", "Missing Fields");
    en.insert("instances-missing-fields-msg", "Please fill in both Skill ID and Agent ID");
    en.insert("instances-create-success-title", "Instance Created");
    en.insert("instances-create-success-msg", "created successfully");
    en.insert("instances-create-fail-title", "Creation Failed");
    en.insert("instances-create-fail-msg", "Failed to create instance");
    en.insert("instances-delete-success-title", "Instance Deleted");
    en.insert("instances-delete-success-msg", "deleted");
    en.insert("instances-delete-fail-title", "Delete Failed");
    en.insert("instances-delete-fail-msg", "Failed to delete instance");
    en.insert("instances-exec-result-title", "Execution Result");
    en.insert("instances-exec-completed", "Execution completed in");
    en.insert("instances-exec-failed", "Execution failed");
    en.insert("instances-exec-fail-title", "Execution Failed");
    en.insert("instances-exec-fail-msg", "Failed to execute instance");
    en.insert("instances-col-id", "Instance ID");
    en.insert("instances-col-skill", "Skill");
    en.insert("instances-col-agent", "Agent");
    en.insert("instances-col-status", "Status");
    en.insert("instances-col-usage", "Usage");
    en.insert("instances-col-actions", "Actions");
    en.insert("instances-calls", "calls");
    en.insert("instances-avg", "avg");
    en.insert("instances-running", "Running...");
    en.insert("instances-run", "▶ Run");
    en.insert("instances-delete", "Delete");
    en.insert("instances-empty-title", "No instances yet");
    en.insert("instances-empty-desc", "Create a new instance to bind a skill to an agent");
    en.insert("instances-error-title", "Failed to load instances");

    // ==================== treasury.rs ====================
    en.insert("treasury-page-title", "Treasury - BeeBotOS");
    en.insert("treasury-tx-page-title", "Treasury Transactions - BeeBotOS");
    en.insert("treasury-transfer-title", "Transfer");
    en.insert("treasury-transfer-to", "To Address");
    en.insert("treasury-transfer-amount", "Amount (wei)");
    en.insert("treasury-transfer-submitting", "Submitting...");
    en.insert("treasury-transfer-submit", "Submit Transfer");
    en.insert("treasury-transfer-required", "Address and amount are required");
    en.insert("treasury-transfer-submitted", "Transfer submitted");
    en.insert("treasury-transfer-failed", "Transfer failed");
    en.insert("treasury-total-balance", "Total Treasury Balance");
    en.insert("treasury-assets", "Assets");
    en.insert("treasury-tokens", "tokens");
    en.insert("treasury-transactions", "Recent Transactions");
    en.insert("treasury-view-all", "View All →");
    en.insert("treasury-deposit", "Deposit");
    en.insert("treasury-withdraw", "Withdraw");
    en.insert("treasury-transfer", "Transfer");
    en.insert("treasury-about-title", "About the Treasury");
    en.insert("treasury-about-multisig", "Multi-Sig Protected");
    en.insert("treasury-about-multisig-desc", "All withdrawals require multiple signatures from DAO council members");
    en.insert("treasury-about-transparent", "Transparent");
    en.insert("treasury-about-transparent-desc", "All transactions are recorded on-chain and publicly verifiable");
    en.insert("treasury-about-governance", "Governance Controlled");
    en.insert("treasury-about-governance-desc", "Major allocations require community vote through DAO proposals");
    en.insert("treasury-no-assets", "No assets in treasury");
    en.insert("treasury-first-deposit", "Make First Deposit");
    en.insert("treasury-no-transactions", "No recent transactions");
    en.insert("treasury-error-title", "Failed to load treasury");
    en.insert("treasury-error-retry", "Retry");
    en.insert("treasury-tx-title", "Transaction History");
    en.insert("treasury-tx-desc", "All treasury transactions are recorded on-chain");
    en.insert("treasury-all-transactions", "All Transactions");
    en.insert("treasury-tx-total", "total");

    // ==================== webchat.rs ====================
    en.insert("webchat-page-title", "Chat - BeeBotOS");
    en.insert("webchat-new-session", "New Chat");
    en.insert("webchat-default-title", "Chat Session");
    en.insert("webchat-load-messages-failed", "Failed to load messages");
    en.insert("webchat-create-session-failed", "Failed to create session");
    en.insert("webchat-load-sessions-failed", "Failed to load sessions");
    en.insert("webchat-ws-error", "WebSocket connection error");
    en.insert("webchat-send-failed", "Failed to send");
    en.insert("webchat-input-placeholder", "Type a message... (use /btw for side question)");
    en.insert("webchat-sessions-title", "Sessions");
    en.insert("webchat-new-chat", "+ New Chat");
    en.insert("webchat-search-sessions", "Search sessions...");

    translations.insert("en", en);

    let i18n = I18nContext {
        locale: RwSignal::new(Locale::ZhCN),
        translations,
    };

    provide_context(i18n.clone());
    i18n
}

/// Get the current locale
pub fn current_locale(i18n: &I18nContext) -> Locale {
    i18n.get_locale()
}

/// Set the locale
pub fn set_locale(i18n: &I18nContext, locale: Locale) {
    i18n.set_locale(locale);
}

/// Toggle between Chinese and English
pub fn toggle_locale(i18n: &I18nContext) {
    let new_locale = match i18n.get_locale() {
        Locale::ZhCN => Locale::En,
        _ => Locale::ZhCN,
    };
    i18n.set_locale(new_locale);
}
