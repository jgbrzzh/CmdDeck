//! Tauri 命令层。
//!
//! 每个子模块对应前端 `src/api/index.ts` 里的一个 api 分组：
//! * [`preset_cmds`]  预设指令的增删改查、排序、占位符
//! * [`group_cmds`]   分组
//! * [`terminal_cmds`] / [`batch_cmds`]  终端会话与批量执行（backend-runtime）
//! * [`schedule_cmds`] / [`workflow_cmds`] 定时任务与工作流（backend-automation）
//! * [`security_cmds`] 黑名单与危险命令检查（backend-automation）
//! * [`audit_cmds`]  审计日志与终端历史
//! * [`system_cmds`] 设置、导入导出、系统集成
//! * [`seed_cmds`]   首次启动写入示例数据
//!
//! ## 分层约定
//!
//! 命令层只做三件事：参数校验、业务编排、广播事件；
//! 真正的 SQL 一律在 `crate::db::*` 里，方便单元测试覆盖。

pub mod audit_cmds;
pub mod batch_cmds;
pub mod environment_cmds;
pub mod group_cmds;
pub mod preset_cmds;
pub mod productivity_cmds;
pub mod schedule_cmds;
pub mod security_cmds;
pub mod seed_cmds;
pub mod system_cmds;
pub mod terminal_cmds;
pub mod workflow_cmds;
