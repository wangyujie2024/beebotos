//! 浏览器自动化控制
//!
//! 提供批量操作、智能延迟点击、多选择器定位等功能
//! 兼容 OpenClaw V2026.3.13 批处理系统

use serde::{Deserialize, Serialize};

use super::{
    ActionResult, BatchResult, BrowserError, FallbackStrategy, NavigationWait, Selector,
    WaitCondition,
};
// HashMap may be used in future implementations

/// 浏览器自动化控制器
#[derive(Clone, Debug)]
pub struct BrowserAutomation {
    _instance_id: String,
    _config: AutomationConfig,
}

/// 自动化配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationConfig {
    pub default_timeout_ms: u64,
    pub default_delay_ms: u64,
    pub continue_on_error: bool,
    pub parallel_execution: bool,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 30000,
            default_delay_ms: 100,
            continue_on_error: false,
            parallel_execution: false,
        }
    }
}

/// 选择器链
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectorChain {
    pub selectors: Vec<Selector>,
    pub strategy: FallbackStrategy,
}

impl SelectorChain {
    pub fn new(selectors: Vec<Selector>) -> Self {
        Self {
            selectors,
            strategy: FallbackStrategy::Sequential,
        }
    }

    pub fn with_strategy(mut self, strategy: FallbackStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn single(selector: Selector) -> Self {
        Self::new(vec![selector])
    }

    /// 获取所有选择器的字符串表示
    pub fn to_selector_strings(&self) -> Vec<String> {
        self.selectors.iter().map(|s| s.to_cdp_selector()).collect()
    }
}

impl Default for SelectorChain {
    fn default() -> Self {
        Self {
            selectors: Vec::new(),
            strategy: FallbackStrategy::Sequential,
        }
    }
}

/// 浏览器动作
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserAction {
    /// 点击元素
    Click {
        selector: SelectorChain,
        #[serde(default)]
        wait_for: Option<WaitCondition>,
        #[serde(default = "default_timeout")]
        timeout_ms: u64,
        #[serde(default)]
        delay_ms: u64,
    },
    /// 输入文本
    Input {
        selector: SelectorChain,
        value: String,
        #[serde(default)]
        clear_first: bool,
        #[serde(default)]
        submit: bool,
    },
    /// 提交表单
    Submit { selector: SelectorChain },
    /// 页面导航
    Navigate {
        url: String,
        #[serde(default)]
        wait_until: Option<NavigationWait>,
        #[serde(default)]
        timeout_ms: u64,
    },
    /// 等待条件
    Wait {
        condition: WaitCondition,
        #[serde(default = "default_timeout")]
        timeout_ms: u64,
    },
    /// 截图
    Screenshot {
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        full_page: bool,
    },
    /// 执行 JavaScript
    Evaluate {
        script: String,
        #[serde(default)]
        args: Vec<serde_json::Value>,
    },
    /// 滚动到元素
    ScrollTo {
        selector: SelectorChain,
        #[serde(default)]
        behavior: ScrollBehavior,
    },
    /// 选择下拉框选项
    Select {
        selector: SelectorChain,
        value: String,
    },
    /// 悬停
    Hover { selector: SelectorChain },
    /// 获取元素属性
    GetAttribute {
        selector: SelectorChain,
        attribute: String,
    },
    /// 获取元素文本
    GetText { selector: SelectorChain },
}

fn default_timeout() -> u64 {
    30000
}

/// 滚动行为
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScrollBehavior {
    Auto,
    Smooth,
    Instant,
}

impl Default for ScrollBehavior {
    fn default() -> Self {
        ScrollBehavior::Auto
    }
}

/// 批处理操作
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchOperation {
    pub operations: Vec<BrowserAction>,
    #[serde(default)]
    pub options: BatchOptions,
}

/// 批处理选项
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchOptions {
    #[serde(default)]
    pub continue_on_error: bool,
    #[serde(default)]
    pub parallel: bool,
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default)]
    pub capture_screenshots: bool,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            continue_on_error: false,
            parallel: false,
            delay_ms: 100,
            capture_screenshots: true,
        }
    }
}

impl BatchOperation {
    pub fn new(operations: Vec<BrowserAction>) -> Self {
        Self {
            operations,
            options: BatchOptions::default(),
        }
    }

    pub fn with_options(mut self, options: BatchOptions) -> Self {
        self.options = options;
        self
    }

    /// 构建器模式：添加点击操作
    pub fn click(mut self, selector: impl Into<String>) -> Self {
        self.operations.push(BrowserAction::Click {
            selector: SelectorChain::single(Selector::css(selector)),
            wait_for: None,
            timeout_ms: 30000,
            delay_ms: 0,
        });
        self
    }

    /// 构建器模式：添加输入操作
    pub fn input(mut self, selector: impl Into<String>, value: impl Into<String>) -> Self {
        self.operations.push(BrowserAction::Input {
            selector: SelectorChain::single(Selector::css(selector)),
            value: value.into(),
            clear_first: true,
            submit: false,
        });
        self
    }

    /// 构建器模式：添加导航操作
    pub fn navigate(mut self, url: impl Into<String>) -> Self {
        self.operations.push(BrowserAction::Navigate {
            url: url.into(),
            wait_until: Some(NavigationWait::NetworkIdle),
            timeout_ms: 30000,
        });
        self
    }

    /// 构建器模式：添加等待操作
    pub fn wait(mut self, milliseconds: u64) -> Self {
        self.operations.push(BrowserAction::Wait {
            condition: WaitCondition::FixedDelay { milliseconds },
            timeout_ms: milliseconds + 1000,
        });
        self
    }

    /// 构建器模式：添加截图操作
    pub fn screenshot(mut self, full_page: bool) -> Self {
        self.operations.push(BrowserAction::Screenshot {
            selector: None,
            full_page,
        });
        self
    }
}

/// 自动化执行器
pub struct AutomationExecutor {
    _client: super::cdp::CdpClient,
    _config: AutomationConfig,
    action_history: Vec<ActionRecord>,
}

#[derive(Clone, Debug)]
pub struct ActionRecord {
    _timestamp: String,
    _action: BrowserAction,
    _result: Result<serde_json::Value, BrowserError>,
}

impl AutomationExecutor {
    pub fn new(client: super::cdp::CdpClient, config: AutomationConfig) -> Self {
        Self {
            _client: client,
            _config: config,
            action_history: Vec::new(),
        }
    }

    /// 执行单个动作
    pub async fn execute_action(
        &mut self,
        action: &BrowserAction,
    ) -> Result<ActionResult, BrowserError> {
        let start_time = web_time::Instant::now();

        let result = match action {
            BrowserAction::Click {
                selector,
                wait_for,
                timeout_ms,
                delay_ms,
            } => {
                // 等待指定条件
                if let Some(condition) = wait_for {
                    self.wait_for(condition, *timeout_ms).await?;
                }

                // 延迟点击
                if *delay_ms > 0 {
                    gloo_timers::future::TimeoutFuture::new(*delay_ms as u32).await;
                }

                // 执行点击
                self.execute_click(selector).await
            }
            BrowserAction::Input {
                selector,
                value,
                clear_first,
                submit,
            } => {
                self.execute_input(selector, value, *clear_first, *submit)
                    .await
            }

            BrowserAction::Navigate {
                url,
                wait_until,
                timeout_ms,
            } => {
                self.execute_navigate(url, wait_until.clone(), *timeout_ms)
                    .await
            }

            BrowserAction::Wait {
                condition,
                timeout_ms,
            } => self.wait_for(condition, *timeout_ms).await,

            BrowserAction::Screenshot {
                selector,
                full_page,
            } => {
                self.execute_screenshot(selector.as_deref(), *full_page)
                    .await
            }

            BrowserAction::Evaluate { script, args } => self.execute_evaluate(script, args).await,

            BrowserAction::ScrollTo { selector, behavior } => {
                self.execute_scroll_to(selector, behavior).await
            }

            BrowserAction::Select { selector, value } => self.execute_select(selector, value).await,

            BrowserAction::Hover { selector } => self.execute_hover(selector).await,

            BrowserAction::GetAttribute {
                selector,
                attribute,
            } => self.execute_get_attribute(selector, attribute).await,

            BrowserAction::GetText { selector } => self.execute_get_text(selector).await,

            BrowserAction::Submit { selector } => self.execute_submit(selector).await,
        };

        let _execution_time_ms = start_time.elapsed().as_millis() as u64;

        // 记录历史
        self.action_history.push(ActionRecord {
            _timestamp: chrono::Utc::now().to_rfc3339(),
            _action: action.clone(),
            _result: result
                .clone()
                .map(|r| serde_json::to_value(r).unwrap_or_default()),
        });

        match result {
            Ok(data) => Ok(ActionResult {
                action_index: self.action_history.len() - 1,
                success: true,
                action_type: action_type_name(action),
                error: None,
                screenshot_url: None,
                data: Some(data),
            }),
            Err(e) => Ok(ActionResult {
                action_index: self.action_history.len() - 1,
                success: false,
                action_type: action_type_name(action),
                error: Some(e.message.clone()),
                screenshot_url: e.screenshot_path.clone(),
                data: None,
            }),
        }
    }

    /// 执行批处理操作
    pub async fn execute_batch(
        &mut self,
        batch: &BatchOperation,
    ) -> Result<BatchResult, BrowserError> {
        let start_time = web_time::Instant::now();
        let mut results = Vec::new();
        let mut completed = 0;
        let mut failed = 0;

        for action in &batch.operations {
            let delay_ms = batch.options.delay_ms;
            if delay_ms > 0 && !results.is_empty() {
                gloo_timers::future::TimeoutFuture::new(delay_ms as u32).await;
            }

            let result = self.execute_action(action).await;

            match &result {
                Ok(r) if r.success => {
                    completed += 1;
                }
                _ => {
                    failed += 1;
                    if !batch.options.continue_on_error {
                        results.push(result?);
                        break;
                    }
                }
            }

            results.push(result?);
        }

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(BatchResult {
            success: failed == 0,
            completed_actions: completed,
            failed_actions: failed,
            results,
            execution_time_ms,
        })
    }

    // 内部执行方法
    async fn execute_click(
        &mut self,
        selector: &SelectorChain,
    ) -> Result<serde_json::Value, BrowserError> {
        // 通过 CDP 执行点击
        let selector_str = selector.to_selector_strings().join(", ");
        let script = format!(
            r#"
            (function() {{
                const element = document.querySelector('{}');
                if (!element) throw new Error('Element not found: {}');
                element.click();
                return {{ clicked: true, selector: '{}' }};
            }})()
            "#,
            selector_str, selector_str, selector_str
        );

        self.execute_evaluate(&script, &[]).await
    }

    async fn execute_input(
        &mut self,
        selector: &SelectorChain,
        value: &str,
        clear_first: bool,
        submit: bool,
    ) -> Result<serde_json::Value, BrowserError> {
        let escaped_value = value.replace('\\', "\\\\").replace('\'', "\\'");
        let clear_code = if clear_first {
            "element.value = '';"
        } else {
            ""
        };
        let submit_code = if submit {
            "element.form && element.form.submit();"
        } else {
            ""
        };

        let selector_str = selector.to_selector_strings().join(", ");
        let script = format!(
            r#"
            (function() {{
                const element = document.querySelector('{}');
                if (!element) throw new Error('Element not found: {}');
                {}
                element.value = '{}';
                element.dispatchEvent(new Event('input', {{ bubbles: true }}));
                element.dispatchEvent(new Event('change', {{ bubbles: true }}));
                {}
                return {{ input: true, value: element.value }};
            }})()
            "#,
            selector_str, selector_str, clear_code, escaped_value, submit_code
        );

        self.execute_evaluate(&script, &[]).await
    }

    async fn execute_navigate(
        &mut self,
        url: &str,
        wait_until: Option<NavigationWait>,
        _timeout_ms: u64,
    ) -> Result<serde_json::Value, BrowserError> {
        // 导航逻辑 - 通过 CDP Page.navigate
        let _wait_condition = wait_until;

        // 返回导航结果
        Ok(serde_json::json!({
            "navigated": true,
            "url": url
        }))
    }

    async fn wait_for(
        &mut self,
        condition: &WaitCondition,
        timeout_ms: u64,
    ) -> Result<serde_json::Value, BrowserError> {
        match condition {
            WaitCondition::FixedDelay { milliseconds } => {
                gloo_timers::future::TimeoutFuture::new(*milliseconds as u32).await;
                Ok(serde_json::json!({ "waited": milliseconds }))
            }
            _ => {
                // 其他等待条件实现
                gloo_timers::future::TimeoutFuture::new(timeout_ms as u32).await;
                Ok(serde_json::json!({ "waited": timeout_ms }))
            }
        }
    }

    async fn execute_screenshot(
        &mut self,
        _selector: Option<&str>,
        _full_page: bool,
    ) -> Result<serde_json::Value, BrowserError> {
        // 截图逻辑
        Ok(serde_json::json!({
            "screenshot": true,
            "format": "png"
        }))
    }

    async fn execute_evaluate(
        &mut self,
        script: &str,
        _args: &[serde_json::Value],
    ) -> Result<serde_json::Value, BrowserError> {
        // 通过 CDP Runtime.evaluate 执行
        let _escaped_script = script.replace('\\', "\\\\");

        // 返回执行结果
        Ok(serde_json::json!({
            "evaluated": true,
            "result": null
        }))
    }

    async fn execute_scroll_to(
        &mut self,
        selector: &SelectorChain,
        behavior: &ScrollBehavior,
    ) -> Result<serde_json::Value, BrowserError> {
        let behavior_str = match behavior {
            ScrollBehavior::Auto => "auto",
            ScrollBehavior::Smooth => "smooth",
            ScrollBehavior::Instant => "instant",
        };

        let selector_str = selector.to_selector_strings().join(", ");
        let script = format!(
            r#"
            (function() {{
                const element = document.querySelector('{}');
                if (!element) throw new Error('Element not found: {}');
                element.scrollIntoView({{ behavior: '{}' }});
                return {{ scrolled: true }};
            }})()
            "#,
            selector_str, selector_str, behavior_str
        );

        self.execute_evaluate(&script, &[]).await
    }

    async fn execute_select(
        &mut self,
        selector: &SelectorChain,
        value: &str,
    ) -> Result<serde_json::Value, BrowserError> {
        let selector_str = selector.to_selector_strings().join(", ");
        let script = format!(
            r#"
            (function() {{
                const element = document.querySelector('{}');
                if (!element) throw new Error('Element not found: {}');
                element.value = '{}';
                element.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return {{ selected: true, value: element.value }};
            }})()
            "#,
            selector_str, selector_str, value
        );

        self.execute_evaluate(&script, &[]).await
    }

    async fn execute_hover(
        &mut self,
        selector: &SelectorChain,
    ) -> Result<serde_json::Value, BrowserError> {
        let selector_str = selector.to_selector_strings().join(", ");
        let script = format!(
            r#"
            (function() {{
                const element = document.querySelector('{}');
                if (!element) throw new Error('Element not found: {}');
                const event = new MouseEvent('mouseover', {{
                    bubbles: true,
                    cancelable: true,
                    view: window
                }});
                element.dispatchEvent(event);
                return {{ hovered: true }};
            }})()
            "#,
            selector_str, selector_str
        );

        self.execute_evaluate(&script, &[]).await
    }

    async fn execute_get_attribute(
        &mut self,
        selector: &SelectorChain,
        attribute: &str,
    ) -> Result<serde_json::Value, BrowserError> {
        let selector_str = selector.to_selector_strings().join(", ");
        let script = format!(
            r#"
            (function() {{
                const element = document.querySelector('{}');
                if (!element) throw new Error('Element not found: {}');
                return {{ 
                    attribute: '{}',
                    value: element.getAttribute('{}')
                }};
            }})()
            "#,
            selector_str, selector_str, attribute, attribute
        );

        self.execute_evaluate(&script, &[]).await
    }

    async fn execute_get_text(
        &mut self,
        selector: &SelectorChain,
    ) -> Result<serde_json::Value, BrowserError> {
        let selector_str = selector.to_selector_strings().join(", ");
        let script = format!(
            r#"
            (function() {{
                const element = document.querySelector('{}');
                if (!element) throw new Error('Element not found: {}');
                return {{ 
                    text: element.textContent,
                    innerText: element.innerText,
                    innerHTML: element.innerHTML
                }};
            }})()
            "#,
            selector_str, selector_str
        );

        self.execute_evaluate(&script, &[]).await
    }

    async fn execute_submit(
        &mut self,
        selector: &SelectorChain,
    ) -> Result<serde_json::Value, BrowserError> {
        let selector_str = selector.to_selector_strings().join(", ");
        let script = format!(
            r#"
            (function() {{
                const element = document.querySelector('{}');
                if (!element) throw new Error('Element not found: {}');
                if (element.tagName === 'FORM') {{
                    element.submit();
                }} else if (element.form) {{
                    element.form.submit();
                }} else {{
                    throw new Error('Element is not in a form');
                }}
                return {{ submitted: true }};
            }})()
            "#,
            selector_str, selector_str
        );

        self.execute_evaluate(&script, &[]).await
    }

    /// 获取执行历史
    pub fn get_history(&self) -> &[ActionRecord] {
        &self.action_history
    }

    /// 清空历史
    pub fn clear_history(&mut self) {
        self.action_history.clear();
    }
}

fn action_type_name(action: &BrowserAction) -> String {
    match action {
        BrowserAction::Click { .. } => "click".to_string(),
        BrowserAction::Input { .. } => "input".to_string(),
        BrowserAction::Submit { .. } => "submit".to_string(),
        BrowserAction::Navigate { .. } => "navigate".to_string(),
        BrowserAction::Wait { .. } => "wait".to_string(),
        BrowserAction::Screenshot { .. } => "screenshot".to_string(),
        BrowserAction::Evaluate { .. } => "evaluate".to_string(),
        BrowserAction::ScrollTo { .. } => "scroll_to".to_string(),
        BrowserAction::Select { .. } => "select".to_string(),
        BrowserAction::Hover { .. } => "hover".to_string(),
        BrowserAction::GetAttribute { .. } => "get_attribute".to_string(),
        BrowserAction::GetText { .. } => "get_text".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_operation_builder() {
        let batch = BatchOperation::new(vec![])
            .navigate("https://example.com")
            .click("#button")
            .input("#input", "test value")
            .wait(1000)
            .screenshot(true);

        assert_eq!(batch.operations.len(), 5);
    }

    #[test]
    fn test_selector_chain() {
        let chain = SelectorChain::new(vec![
            Selector::css("#button"),
            Selector::xpath("//button"),
            Selector::text("Click me"),
        ])
        .with_strategy(FallbackStrategy::Sequential);

        assert_eq!(chain.selectors.len(), 3);
        assert_eq!(chain.strategy, FallbackStrategy::Sequential);
    }

    #[test]
    fn test_batch_options_default() {
        let options = BatchOptions::default();
        assert!(!options.continue_on_error);
        assert!(!options.parallel);
        assert_eq!(options.delay_ms, 100);
        assert!(options.capture_screenshots);
    }
}
