//! CmdDeck 全局错误类型。
//!
//! 所有 Tauri 命令统一返回 `Result<T, AppError>`，前端 `invoke` 会拿到一个普通对象：
//! `{ kind: "NotFound", message: "预设不存在" }`，据此弹出中文友好提示。

use serde::{Serialize, Serializer};

/// 应用统一错误。
///
/// `kind` 供前端做分支处理（例如 `NotFound` 关闭弹窗、`Conflict` 提示重名），
/// `message` 直接展示给用户，必须是中文且说明"哪里错了、怎么改"。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 找不到指定对象（预设、分组、工作流……）
    #[error("{0}")]
    NotFound(String),

    /// 参数校验失败
    #[error("{0}")]
    Validation(String),

    /// 数据冲突（重名等）
    #[error("{0}")]
    Conflict(String),

    /// 数据库 / 文件系统错误
    #[error("数据操作失败：{0}")]
    Io(String),

    /// 命令执行失败
    #[error("命令执行失败：{0}")]
    Exec(String),

    /// 命中安全黑名单
    #[error("命令被安全策略拦截：{0}")]
    Blocked(String),

    /// 权限不足（例如非管理员却要求提权）
    #[error("权限不足：{0}")]
    Permission(String),

    /// 其它未分类错误
    #[error("{0}")]
    Other(String),
}

impl AppError {
    // ---- 便捷构造 ----
    pub fn not_found(msg: impl Into<String>) -> Self {
        AppError::NotFound(msg.into())
    }
    pub fn validation(msg: impl Into<String>) -> Self {
        AppError::Validation(msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        AppError::Conflict(msg.into())
    }
    pub fn io(msg: impl Into<String>) -> Self {
        AppError::Io(msg.into())
    }
    pub fn exec(msg: impl Into<String>) -> Self {
        AppError::Exec(msg.into())
    }
    pub fn blocked(msg: impl Into<String>) -> Self {
        AppError::Blocked(msg.into())
    }
    pub fn permission(msg: impl Into<String>) -> Self {
        AppError::Permission(msg.into())
    }
    pub fn other(msg: impl Into<String>) -> Self {
        AppError::Other(msg.into())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Io(format!("JSON 解析失败：{e}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<anyhow_shim::Error> for AppError {
    fn from(e: anyhow_shim::Error) -> Self {
        AppError::Exec(e.to_string())
    }
}

/// 轻量 anyhow 兼容层。
///
/// `portable-pty` 的接口大量返回 `anyhow::Result`。这里不直接引入 anyhow 依赖，
/// 而是统一收敛到 `Box<dyn std::error::Error>`，避免两套错误类型互相污染。
pub mod anyhow_shim {
    /// PTY 相关错误统一别名
    pub type Error = Box<dyn std::error::Error + Send + Sync>;
    /// PTY 相关结果统一别名
    pub type Result<T> = std::result::Result<T, Error>;

    /// 把任意错误对象装箱成统一错误类型
    pub fn boxed<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
        Box::new(e)
    }
}

/// 把错误序列化成前端可读的对象。
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let kind = match self {
            AppError::NotFound(_) => "NotFound",
            AppError::Validation(_) => "Validation",
            AppError::Conflict(_) => "Conflict",
            AppError::Io(_) => "Io",
            AppError::Exec(_) => "Exec",
            AppError::Blocked(_) => "Blocked",
            AppError::Permission(_) => "Permission",
            AppError::Other(_) => "Other",
        };
        let mut st = serializer.serialize_struct("AppError", 2)?;
        st.serialize_field("kind", kind)?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

/// 命令返回类型别名，减少 `Result<..., AppError>` 的重复书写
pub type AppResult<T> = Result<T, AppError>;
