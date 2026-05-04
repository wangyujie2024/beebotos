//! Internationalization (i18n) module for BeeBotOS Web
//!
//! Provides multi-language support with Chinese (zh-CN) as default

use leptos::prelude::*;
use std::collections::HashMap;

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
    let mut translations: HashMap<&'static str, HashMap<&'static str, &'static str>> = HashMap::new();

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
    zh.insert("nav-workflows", "工作流");
    zh.insert("nav-llm-settings", "大模型");
    zh.insert("nav-channels", "频道管理");
    zh.insert("nav-settings", "设置");
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
    zh.insert("hero-subtitle", "构建、部署和管理具备内置治理功能的智能代理");
    zh.insert("hero-cta-primary", "开始使用");
    zh.insert("hero-cta-secondary", "浏览技能");
    zh.insert("features-title", "核心功能");
    zh.insert("feature-agents-title", "自主智能体");
    zh.insert("feature-agents-desc", "部署具备内置安全控制的独立运行 AI 智能体");
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
    zh.insert("wechat-login-hint", "使用微信扫描二维码登录，获取 Bot Token");
    zh.insert("qr-expires-in", "二维码过期时间");
    zh.insert("action-get-qr", "获取二维码");
    zh.insert("action-refresh-qr", "刷新二维码");
    zh.insert("action-test", "测试连接");
    zh.insert("config-base-url", "Base URL");
    zh.insert("config-bot-token", "Bot Token");
    zh.insert("config-auto-reconnect", "自动重连");

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
    translations.insert("zh-CN", zh);

    // English translations
    let mut en = HashMap::new();
    en.insert("app-title", "BeeBotOS - Web4.0 Autonomous Agent Operating System");
    en.insert("app-description", "The Operating System for Autonomous AI Agents");
    en.insert("nav-home", "Home");
    en.insert("nav-agents", "Agents");
    en.insert("nav-dao", "DAO");
    en.insert("nav-treasury", "Treasury");
    en.insert("nav-skills", "Skills");
    en.insert("nav-skill-instances", "Instances");
    en.insert("nav-workflows", "Workflows");
    en.insert("nav-llm-settings", "LLM Model");
    en.insert("nav-channels", "Channels");
    en.insert("nav-settings", "Settings");
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
    en.insert("login-demo-hint", "Demo mode: Enter any username and password to login");
    // Register page
    en.insert("register-title", "Create Account");
    en.insert("register-subtitle", "Register a BeeBotOS account to get started");
    en.insert("register-username", "Username");
    en.insert("register-username-placeholder", "Enter your username");
    en.insert("register-email", "Email");
    en.insert("register-email-placeholder", "Enter your email (optional)");
    en.insert("register-password", "Password");
    en.insert("register-password-placeholder", "Enter password (at least 6 characters)");
    en.insert("register-confirm-password", "Confirm Password");
    en.insert("register-confirm-password-placeholder", "Enter password again");
    en.insert("register-error-empty", "Username and password cannot be empty");
    en.insert("register-error-password-mismatch", "Passwords do not match");
    en.insert("register-error-password-short", "Password must be at least 6 characters");
    en.insert("register-error-failed", "Registration failed");
    en.insert("register-or", "OR");
    en.insert("register-demo-button", "Demo Register");
    en.insert("register-have-account", "Already have an account?");
    en.insert("register-login-link", "Login now");
    en.insert("hero-title", "The Operating System for Autonomous AI Agents");
    en.insert("hero-subtitle", "Build, deploy, and manage intelligent agents with built-in governance");
    en.insert("hero-cta-primary", "Get Started");
    en.insert("hero-cta-secondary", "Browse Skills");
    en.insert("features-title", "Core Features");
    en.insert("feature-agents-title", "Autonomous Agents");
    en.insert("feature-agents-desc", "Deploy AI agents that operate independently with built-in safety controls");
    en.insert("feature-dao-title", "DAO Governance");
    en.insert("feature-dao-desc", "Community-driven decision making with transparent voting mechanisms");
    en.insert("feature-treasury-title", "Secure Treasury");
    en.insert("feature-treasury-desc", "Multi-sig treasury management with on-chain transparency");
    en.insert("feature-skills-title", "Skill Marketplace");
    en.insert("feature-skills-desc", "Extend agent capabilities with community-built skills");
    en.insert("feature-wasm-title", "WebAssembly Runtime");
    en.insert("feature-wasm-desc", "High-performance, sandboxed execution environment");
    en.insert("feature-analytics-title", "Real-time Analytics");
    en.insert("feature-analytics-desc", "Monitor agent performance and system health in real-time");
    en.insert("quick-actions-title", "Quick Actions");
    en.insert("quick-action-create-agent-title", "Create Agent");
    en.insert("quick-action-create-agent-desc", "Set up a new autonomous agent");
    en.insert("quick-action-view-proposals-title", "View Proposals");
    en.insert("quick-action-view-proposals-desc", "Participate in DAO governance");
    en.insert("quick-action-install-skills-title", "Install Skills");
    en.insert("quick-action-install-skills-desc", "Add capabilities to your agents");
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
    en.insert("channels-subtitle", "Configure and manage message channel connections");
    en.insert("channel-status", "Channel Status");
    en.insert("channel-config", "Channel Configuration");
    en.insert("status-enabled", "Enabled");
    en.insert("status-disabled", "Disabled");
    en.insert("wechat-login", "WeChat Login");
    en.insert("wechat-login-hint", "Scan QR code with WeChat to get Bot Token");
    en.insert("qr-expires-in", "QR expires in");
    en.insert("action-get-qr", "Get QR Code");
    en.insert("action-refresh-qr", "Refresh QR Code");
    en.insert("action-test", "Test Connection");
    en.insert("config-base-url", "Base URL");
    en.insert("config-bot-token", "Bot Token");
    en.insert("config-auto-reconnect", "Auto Reconnect");

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
    en.insert("error-404-description", "The page you're looking for doesn't exist or has been moved.");
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
    en.insert("quick-action-start-chat-desc", "Start a conversation with AI agents");
    en.insert("nav-section-chat", "Chat");
    en.insert("nav-section-control", "Control");
    en.insert("nav-section-agents", "Agents");
    en.insert("nav-section-settings", "Settings");
    en.insert("footer-product", "Product");
    en.insert("footer-resources", "Resources");
    en.insert("footer-community", "Community");
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
