//! General-purpose built-in tools for the Agent tool-calling framework.
//!
//! These tools provide fundamental capabilities: file I/O, web access,
//! command execution, and process management.

pub mod exec;
pub mod process;
pub mod read_file;
pub mod search_files;
pub mod web_fetch;
pub mod write_file;

// Re-export all tools for convenience
pub use exec::ExecTool;
pub use process::ProcessTool;
pub use read_file::ReadFileTool;
pub use search_files::SearchFilesTool;
pub use web_fetch::WebFetchTool;
pub use write_file::WriteFileTool;
