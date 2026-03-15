//! Codex API 模块
//!
//! 提供 OpenAI Codex (Responses API) 的反代服务，包括：
//! - 请求/响应透传（不做协议转换）
//! - 号池管理（使用现有 OAuth 账号）
//! - 请求日志记录（内存 + 持久化）
//! - Token 使用统计

pub mod archive;
pub mod archive_storage;
pub mod commands;
pub mod executor;
pub mod logger;
pub mod models;
pub mod pool;
pub mod relay;
pub mod server;
pub mod storage;
pub mod team_profiles;

pub use archive_storage::{ArchiveSessionRow, ArchiveTurnRecord, CodexArchiveStorage};
pub use executor::CodexExecutor;
pub use logger::RequestLogger;
pub use models::*;
pub use pool::CodexPool;
pub use server::CodexServer;
pub use storage::CodexLogStorage;
