//! 浏览器自动化组件
//!
//! 提供浏览器控制相关的 UI 组件

pub mod browser_viewport;
pub mod debug_console;
pub mod profile_card;
pub mod profile_list;
pub mod sandbox_card;
pub mod sandbox_list;

pub use browser_viewport::BrowserViewport;
pub use debug_console::DebugConsole;
pub use profile_card::ProfileCard;
pub use profile_list::ProfileList;
pub use sandbox_card::SandboxCard;
pub use sandbox_list::SandboxList;
