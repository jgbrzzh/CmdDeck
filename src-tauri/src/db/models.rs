//! 全局数据模型（契约层）。
//!
//! ⚠️ 这是前后端约定的唯一事实来源：
//!   * Rust 侧结构体字段全部 `camelCase` 序列化，与 `src/types/index.ts` 一一对应；
//!   * 数据库里复杂字段（数组、对象）统一以 JSON 文本存储，读写时用 serde 转换；
//!   * 字符串枚举（如 `kind`）使用宽松字符串，方便导入导出与向后兼容。

use serde::{Deserialize, Serialize};

// ============================================================
// 常量：字符串枚举取值
// ============================================================

/// 预设执行类型
pub mod preset_kind {
    /// 交互式 Shell（用户在终端里敲命令，不自动退出）
    pub const SHELL: &str = "shell";
    /// Windows 命令提示符
    pub const CMD: &str = "cmd";
    /// Windows PowerShell 5.1
    pub const POWERSHELL: &str = "powershell";
    /// Windows PowerShell 7+（pwsh）
    pub const PWSH: &str = "pwsh";
    /// Python 脚本
    pub const PYTHON: &str = "python";
    /// Node.js
    pub const NODE: &str = "node";
    /// 自定义 exe / 任意可执行文件
    pub const EXE: &str = "exe";
    /// 自定义（完全手工指定程序 + 参数）
    pub const CUSTOM: &str = "custom";

    /// 全部合法取值，供设置界面下拉框使用
    pub const ALL: [&str; 8] = [SHELL, CMD, POWERSHELL, PWSH, PYTHON, NODE, EXE, CUSTOM];
}

/// 运行来源（审计日志用）
pub mod run_source {
    pub const MANUAL: &str = "manual"; // 手动点击运行
    pub const BATCH: &str = "batch"; // 批量执行
    pub const SCHEDULE: &str = "schedule"; // 定时任务触发
    pub const WORKFLOW: &str = "workflow"; // 工作流内部步骤
    pub const QUICK_LAUNCH: &str = "quick"; // 快速启动面板 / 全局快捷键
    pub const SHORTCUT: &str = "shortcut"; // 预设自身绑定的快捷键
}

// ============================================================
// 基础结构
// ============================================================

/// 环境变量键值对
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

impl EnvVar {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// 运行前弹窗输入项定义（占位符 {{key}}）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Placeholder {
    /// 占位符键名，对应 `{{key}}`
    pub key: String,
    /// 弹窗中显示的中文标签
    pub label: String,
    /// 控件类型：`text` | `number` | `path` | `folder` | `file` | `select` | `password`
    #[serde(default = "default_input_type")]
    pub input_type: String,
    /// 默认值
    #[serde(default)]
    pub default_value: String,
    /// `select` 类型的候选项
    #[serde(default)]
    pub options: Vec<String>,
    /// 是否必填
    #[serde(default)]
    pub required: bool,
    /// 输入框下方的补充说明
    #[serde(default)]
    pub help: String,
}

fn default_input_type() -> String {
    "text".to_string()
}

// ============================================================
// 预设指令
// ============================================================

/// 运行环境只改变当前子进程，不切换系统全局 Python / Node。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBinding {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub manager_path: String,
}

/// 一条预设指令（CmdDeck 的核心数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    #[serde(default)]
    pub runtime: RuntimeBinding,
    /// 唯一 ID（uuid v4）
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 执行类型，取值见 [`preset_kind`]
    pub kind: String,
    /// 主程序：可以是 `python`、绝对路径 exe、或留空由 kind 推断
    #[serde(default)]
    pub program: String,
    /// 参数数组。元素中可含 `{{key}}` 占位符
    #[serde(default)]
    pub args: Vec<String>,
    /// 工作目录，空表示用用户主目录
    #[serde(default)]
    pub working_dir: String,
    /// 附加环境变量
    #[serde(default)]
    pub env: Vec<EnvVar>,
    /// 是否通过 Shell 包装（支持 `&&`、`|`、`>` 等管道重定向语法）
    #[serde(default)]
    pub use_shell: bool,
    /// 图标名（内置图标 key，见前端 Icon 组件）
    #[serde(default = "default_icon")]
    pub icon: String,
    /// 所属分组 ID，空表示"未分组"
    #[serde(default)]
    pub group_id: String,
    /// 标签列表，用于搜索过滤
    #[serde(default)]
    pub tags: Vec<String>,
    /// 运行前是否需要二次确认
    #[serde(default)]
    pub confirm: bool,
    /// 是否以管理员身份运行（UAC 提权）
    #[serde(default)]
    pub elevated: bool,
    /// 危险等级：0 安全 / 1 注意 / 2 危险。用于列表着色与自动二次确认
    #[serde(default)]
    pub danger_level: i32,
    /// 备注说明
    #[serde(default)]
    pub notes: String,
    /// 排序值，越小越靠前
    #[serde(default)]
    pub sort_order: i32,
    /// 是否收藏
    #[serde(default)]
    pub favorite: bool,
    /// 是否隐藏（隐藏后不出现在快速启动，但仍可手动打开）
    #[serde(default)]
    pub hidden: bool,
    /// 该预设绑定的全局快捷键，如 `CommandOrControl+Shift+1`
    #[serde(default)]
    pub shortcut: String,
    /// 运行前需要弹窗填写的参数
    #[serde(default)]
    pub placeholder_args: Vec<Placeholder>,
    /// 累计运行次数
    #[serde(default)]
    pub run_count: i32,
    /// 最近一次运行时间（Unix 毫秒）
    #[serde(default)]
    pub last_run_at: i64,
    /// 创建时间（Unix 毫秒）
    #[serde(default)]
    pub created_at: i64,
    /// 更新时间（Unix 毫秒）
    #[serde(default)]
    pub updated_at: i64,
}

fn default_icon() -> String {
    "terminal".to_string()
}

impl Default for Preset {
    fn default() -> Self {
        let now = now_ms();
        Self {
            runtime: RuntimeBinding::default(),
            id: new_id(),
            name: "新预设".to_string(),
            kind: preset_kind::POWERSHELL.to_string(),
            program: String::new(),
            args: Vec::new(),
            working_dir: String::new(),
            env: Vec::new(),
            use_shell: true,
            icon: default_icon(),
            group_id: String::new(),
            tags: Vec::new(),
            confirm: false,
            elevated: false,
            danger_level: 0,
            notes: String::new(),
            sort_order: 0,
            favorite: false,
            hidden: false,
            shortcut: String::new(),
            placeholder_args: Vec::new(),
            run_count: 0,
            last_run_at: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

/// 预设列表查询过滤条件
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetFilter {
    /// 关键字，匹配名称/命令/标签/备注
    #[serde(default)]
    pub keyword: String,
    /// 限定分组
    #[serde(default)]
    pub group_id: String,
    /// 只看收藏
    #[serde(default)]
    pub only_favorite: bool,
    /// 只看最近使用
    #[serde(default)]
    pub only_recent: bool,
    /// 包含隐藏项
    #[serde(default)]
    pub include_hidden: bool,
    /// 限定标签
    #[serde(default)]
    pub tag: String,
    /// 限定执行类型
    #[serde(default)]
    pub kind: String,
}

// ============================================================
// 分组
// ============================================================

/// 预设分组（左侧导航的文件夹）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: String,
    /// 自定义颜色，形如 `#ff6b6b`，空表示跟随主题
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub sort_order: i32,
    /// 是否折叠
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

impl Default for Group {
    fn default() -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            name: "新分组".to_string(),
            icon: "folder".to_string(),
            color: String::new(),
            sort_order: 0,
            collapsed: false,
            created_at: now,
            updated_at: now,
        }
    }
}

// ============================================================
// 终端会话
// ============================================================

/// 前端请求创建一个终端会话时传入的参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnOptions {
    #[serde(default)]
    pub runtime: RuntimeBinding,
    /// 关联的预设 ID（可空，例如手动开的空白终端）
    #[serde(default)]
    pub preset_id: String,
    /// 标签页标题
    #[serde(default)]
    pub title: String,
    /// 执行类型，取值见 [`preset_kind`]
    #[serde(default = "default_spawn_kind")]
    pub kind: String,
    /// 主程序
    #[serde(default)]
    pub program: String,
    /// 参数
    #[serde(default)]
    pub args: Vec<String>,
    /// 工作目录
    #[serde(default)]
    pub working_dir: String,
    /// 环境变量
    #[serde(default)]
    pub env: Vec<EnvVar>,
    /// 是否用 Shell 包装
    #[serde(default)]
    pub use_shell: bool,
    /// 是否提权运行
    #[serde(default)]
    pub elevated: bool,
    /// 交互式会话（Shell 常驻）；false 表示执行完即退出
    #[serde(default)]
    pub interactive: bool,
    /// 终端列数
    #[serde(default = "default_cols")]
    pub cols: u16,
    /// 终端行数
    #[serde(default = "default_rows")]
    pub rows: u16,
    /// 运行来源，写入审计日志
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_spawn_kind() -> String {
    preset_kind::SHELL.to_string()
}
fn default_cols() -> u16 {
    120
}
fn default_rows() -> u16 {
    30
}
fn default_source() -> String {
    run_source::MANUAL.to_string()
}

impl SpawnOptions {
    /// 按 `kind` 推断出真正要启动的程序名与完整参数
    ///
    /// 返回 `(program, args)`，交给 `CommandBuilder` 使用。
    pub fn resolve(&self) -> (String, Vec<String>) {
        let (mut program, args) = self.resolve_base();
        if self.runtime.kind == "conda" {
            let mut wrapped = vec![
                "run".into(),
                "--no-capture-output".into(),
                "-p".into(),
                self.runtime.path.clone(),
                program,
            ];
            wrapped.extend(args);
            return (self.runtime.manager_path.clone(), wrapped);
        }
        if self.kind == "python"
            && ["venv", "python"].contains(&self.runtime.kind.as_str())
            && (self.program.is_empty()
                || ["python", "python.exe"].contains(&self.program.as_str()))
        {
            program = if self.runtime.kind == "python" {
                self.runtime.path.clone()
            } else {
                std::path::Path::new(&self.runtime.path)
                    .join("Scripts/python.exe")
                    .to_string_lossy()
                    .into_owned()
            };
        } else if self.kind == "node"
            && self.runtime.kind == "node"
            && (self.program.is_empty() || ["node", "node.exe"].contains(&self.program.as_str()))
        {
            program = self.runtime.path.clone();
        }
        (program, args)
    }
    fn resolve_base(&self) -> (String, Vec<String>) {
        let kind = self.kind.as_str();
        if self.interactive {
            return (
                if self.program.is_empty() {
                    default_program_for_kind(kind)
                } else {
                    self.program.clone()
                },
                self.args.clone(),
            );
        }
        if self.use_shell && ["cmd", "shell", "powershell", "pwsh"].contains(&kind) {
            // Shell 模式的首字段就是原始脚本，不给整条脚本再套引号。
            let line = if self.program.is_empty() {
                self.args.join(" ")
            } else {
                format!("{} {}", self.program, self.args.join(" "))
                    .trim()
                    .to_string()
            };
            if kind == "cmd" || kind == "shell" {
                return (
                    default_program_for_kind("cmd"),
                    vec![
                        "/d".into(),
                        "/c".into(),
                        format!("chcp 65001 >nul & {}", line),
                    ],
                );
            }
            return (default_program_for_kind(kind), vec!["-NoLogo".into(), "-NoProfile".into(), "-Command".into(), format!("[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); {}; if (-not $?) {{ exit 1 }}", line)]);
        }
        (
            if self.program.is_empty() {
                default_program_for_kind(kind)
            } else {
                self.program.clone()
            },
            self.args.clone(),
        )
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    #[test]
    fn legacy_import_defaults_to_system_runtime() {
        let p: Preset =
            serde_json::from_str(r#"{"id":"old","name":"旧配置","kind":"python"}"#).unwrap();
        assert!(p.runtime.kind.is_empty());
    }
    #[test]
    fn selected_environment_survives_database_roundtrip() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let mut p = Preset::default();
        p.runtime = RuntimeBinding {
            kind: "venv".into(),
            path: r"C:\project with spaces\.venv".into(),
            manager_path: String::new(),
        };
        let saved = crate::db::presets::save(&db, &p).unwrap();
        let read = crate::db::presets::get(&db, &saved.id).unwrap();
        assert_eq!(read.runtime.path, p.runtime.path);
        assert_eq!(read.runtime.kind, "venv");
    }
}

/// 根据执行类型推断默认程序
pub fn default_program_for_kind(kind: &str) -> String {
    match kind {
        preset_kind::CMD => std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
        preset_kind::POWERSHELL => "powershell.exe".to_string(),
        preset_kind::PWSH => "pwsh.exe".to_string(),
        preset_kind::PYTHON => "python.exe".to_string(),
        preset_kind::NODE => "node.exe".to_string(),
        preset_kind::SHELL => std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
        _ => String::new(),
    }
}

/// 把 program + args 拼成一行可读的命令行文本
pub fn join_command_line(program: &str, args: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !program.trim().is_empty() {
        parts.push(quote_if_needed(program.trim()));
    }
    for a in args {
        parts.push(quote_if_needed(a));
    }
    parts.join(" ")
}

/// 仅在含空格或特殊字符时加双引号
fn quote_if_needed(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    if s.contains(' ') || s.contains('\t') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// 终端会话状态（返回给前端，用于渲染标签页）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub session_id: String,
    pub title: String,
    #[serde(default)]
    pub preset_id: String,
    #[serde(default)]
    pub preset_name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub cwd: String,
    pub started_at: i64,
    /// `running` / `exited` / `killed`
    pub status: String,
    /// 退出码，None 表示还在运行
    #[serde(default)]
    pub exit_code: Option<i32>,
    pub cols: u16,
    pub rows: u16,
    #[serde(default)]
    pub elevated: bool,
    /// 是否属于「工作流批量执行」产生的临时会话
    #[serde(default)]
    pub temporary: bool,
}

// ============================================================
// 安全
// ============================================================

/// 安全检查结论
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityVerdict {
    /// `safe` 安全 / `warn` 注意 / `danger` 危险 / `blocked` 已拦截
    pub level: String,
    /// 命中的风险说明，逐条展示给用户
    pub reasons: Vec<String>,
    /// 命中的黑名单关键字
    pub matched: Vec<String>,
    /// 是否强制要求二次确认
    pub requires_confirm: bool,
}

impl SecurityVerdict {
    pub fn safe() -> Self {
        Self {
            level: "safe".into(),
            reasons: Vec::new(),
            matched: Vec::new(),
            requires_confirm: false,
        }
    }
}

// ============================================================
// 审计与历史
// ============================================================

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLog {
    pub id: i64,
    /// 发生时间（Unix 毫秒）
    pub at: i64,
    #[serde(default)]
    pub preset_id: String,
    #[serde(default)]
    pub preset_name: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub cwd: String,
    /// 0 安全 / 1 注意 / 2 危险
    #[serde(default)]
    pub level: i32,
    /// `success` / `failed` / `killed` / `blocked`
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub duration_ms: i64,
    /// 运行来源，取值见 [`run_source`]
    #[serde(default)]
    pub source: String,
}

/// 审计日志查询条件
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFilter {
    #[serde(default)]
    pub keyword: String,
    #[serde(default)]
    pub level: i32,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub limit: i64,
}

/// 终端运行历史（终端历史面板展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalHistory {
    pub id: i64,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub preset_id: String,
    #[serde(default)]
    pub preset_name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub kind: String,
    pub started_at: i64,
    #[serde(default)]
    pub ended_at: i64,
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// 结尾的输出片段，方便不打开终端也能看到结果
    #[serde(default)]
    pub output_tail: String,
}

/// 系统里检测到的 Shell，用于"新建终端"下拉框
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellOption {
    /// 稳定标识：`cmd` / `powershell` / `pwsh` / `gitbash` / `wsl` / `ubuntu`
    pub id: String,
    /// 显示名称
    pub label: String,
    /// 实际可执行文件路径
    pub path: String,
    /// 该 Shell 需要的启动参数
    #[serde(default)]
    pub args: Vec<String>,
    /// 是否已在本机找到
    #[serde(default)]
    pub available: bool,
}

// ============================================================
// 批量执行
// ============================================================

/// 批量执行请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {
    /// 要执行的预设 ID 列表
    pub preset_ids: Vec<String>,
    /// 每条预设对应的参数键值（key = 占位符 key）
    #[serde(default)]
    pub args_map: std::collections::HashMap<String, String>,
    /// 同时最多跑几个
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// 某一条失败后是否继续
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
}

fn default_concurrency() -> usize {
    4
}
fn default_true() -> bool {
    true
}

/// 批量执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub run_id: String,
    /// 每条任务对应的终端会话 ID，前端据此开标签页
    pub sessions: Vec<TerminalInfo>,
    /// 被安全策略拦截的预设
    pub blocked: Vec<String>,
    pub started_at: i64,
}

// ============================================================
// 定时任务
// ============================================================

/// 定时任务
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub preset_id: String,
    /// 运行参数（占位符取值），JSON 对象
    #[serde(default)]
    pub args: std::collections::HashMap<String, String>,
    /// `interval` 间隔 / `daily` 每天 / `weekly` 每周 / `once` 仅一次
    #[serde(default = "default_schedule_mode")]
    pub mode: String,
    /// interval 模式：间隔分钟数
    #[serde(default)]
    pub interval_minutes: i64,
    /// daily/weekly 模式：触发时刻 "HH:mm"
    #[serde(default = "default_time")]
    pub time: String,
    /// weekly 模式：星期几，1=周一 … 7=周日
    #[serde(default)]
    pub weekdays: Vec<i32>,
    /// once 模式：日期 "YYYY-MM-DD"
    #[serde(default)]
    pub date: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 下次触发时间（Unix 毫秒）
    #[serde(default)]
    pub next_run_at: i64,
    #[serde(default)]
    pub last_run_at: i64,
    #[serde(default)]
    pub last_status: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

fn default_schedule_mode() -> String {
    "interval".to_string()
}
fn default_time() -> String {
    "09:00".to_string()
}

impl Default for Schedule {
    fn default() -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            name: "新定时任务".to_string(),
            preset_id: String::new(),
            args: std::collections::HashMap::new(),
            mode: default_schedule_mode(),
            interval_minutes: 60,
            time: default_time(),
            weekdays: vec![],
            date: String::new(),
            enabled: true,
            next_run_at: 0,
            last_run_at: 0,
            last_status: String::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

// ============================================================
// 工作流
// ============================================================

/// 工作流中的一个步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// 执行的预设 ID
    #[serde(default)]
    pub preset_id: String,
    /// 该步骤的占位符取值
    #[serde(default)]
    pub args: std::collections::HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 该步骤开始前的等待毫秒数
    #[serde(default)]
    pub delay_ms: i64,
    /// `always` 总是执行 / `on_success` 上一步成功才执行 / `on_fail` 上一步失败才执行
    #[serde(default = "default_step_condition")]
    pub condition: String,
}

fn default_step_condition() -> String {
    "always".to_string()
}

impl Default for WorkflowStep {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: String::new(),
            preset_id: String::new(),
            args: std::collections::HashMap::new(),
            enabled: true,
            delay_ms: 0,
            condition: default_step_condition(),
        }
    }
}

/// 工作流（多个预设按顺序串联执行）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// `serial` 串行 / `parallel` 并行
    #[serde(default = "default_run_mode")]
    pub run_mode: String,
    /// 出错是否继续
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

fn default_run_mode() -> String {
    "serial".to_string()
}

impl Default for Workflow {
    fn default() -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            name: "新工作流".to_string(),
            description: String::new(),
            run_mode: default_run_mode(),
            continue_on_error: true,
            steps: vec![],
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }
}

/// 单个步骤的执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepResult {
    pub step_id: String,
    pub step_name: String,
    pub preset_name: String,
    /// `skipped` / `running` / `success` / `failed` / `blocked`
    pub status: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub started_at: i64,
    #[serde(default)]
    pub finished_at: i64,
}

/// 一次工作流执行记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    pub id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub started_at: i64,
    #[serde(default)]
    pub finished_at: i64,
    /// `running` / `success` / `failed` / `canceled`
    pub status: String,
    #[serde(default)]
    pub steps: Vec<WorkflowStepResult>,
    #[serde(default)]
    pub source: String,
}

// ============================================================
// 设置
// ============================================================

/// 应用设置（单条 JSON 存进 settings 表的 key = "app"）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    // ---- 外观 ----
    /// `dark` / `light` / `system`
    pub theme: String,
    /// 界面语言
    pub locale: String,
    /// 终端字体
    pub font_family: String,
    pub font_size: u32,
    /// 终端配色方案名
    pub color_scheme: String,
    /// 光标闪烁
    pub cursor_blink: bool,
    /// 终端回滚行数
    pub scrollback: u32,
    /// 界面缩放 80~150
    pub ui_scale: u32,

    // ---- 终端行为 ----
    /// 默认 Shell：`cmd` / `powershell` / `pwsh`
    pub default_shell: String,
    /// 自定义 Shell 可执行文件路径
    pub shell_path: String,
    /// 默认工作目录，空表示用户主目录
    pub default_working_dir: String,
    /// 新标签页右上角提示符
    pub show_welcome_banner: bool,
    /// 编码探测：auto / utf-8 / gbk
    pub encoding: String,
    /// 同时运行的会话上限
    pub max_concurrent_sessions: u32,
    /// 命令结束后自动关闭标签页
    pub auto_close_on_exit: bool,

    // ---- 行为 ----
    /// 危险命令强制二次确认
    pub confirm_dangerous: bool,
    /// 允许运行未在白名单内的 exe
    pub allow_unknown_exe: bool,
    /// 记录审计日志
    pub audit_enabled: bool,
    /// 终端历史记录开关
    pub history_enabled: bool,
    /// 历史记录最多保留条数
    pub history_limit: u32,

    // ---- 系统集成 ----
    /// 全局快捷键，如 `CommandOrControl+Shift+Space`
    pub global_hotkey: String,
    /// 开机自启动
    pub autostart: bool,
    /// 关闭窗口时最小化到托盘
    pub minimize_to_tray: bool,
    /// 单实例运行
    pub single_instance: bool,
    /// 启动时恢复上次窗口
    pub restore_window_on_start: bool,

    // ---- 安全黑名单（正则或关键字）----
    pub blacklist: Vec<String>,

    // ---- 首次启动引导 ----
    pub first_run_done: bool,
    /// 数据目录（只读展示）
    pub data_dir: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            locale: "zh-CN".into(),
            font_family: "Cascadia Mono, Consolas, Microsoft YaHei Mono, monospace".into(),
            font_size: 14,
            color_scheme: "cmddeck-dark".into(),
            cursor_blink: true,
            scrollback: 5000,
            ui_scale: 100,

            default_shell: "powershell".into(),
            shell_path: String::new(),
            default_working_dir: String::new(),
            show_welcome_banner: true,
            encoding: "auto".into(),
            max_concurrent_sessions: 12,
            auto_close_on_exit: false,

            confirm_dangerous: true,
            allow_unknown_exe: false,
            audit_enabled: true,
            history_enabled: true,
            history_limit: 500,

            global_hotkey: "CommandOrControl+Shift+Space".into(),
            autostart: false,
            minimize_to_tray: true,
            single_instance: true,
            restore_window_on_start: true,

            blacklist: vec![
                "format c:".into(),
                "rd /s /q c:\\\\".into(),
                "Remove-Item -Recurse -Force c:\\\\".into(),
                "del /f /s /q c:\\\\".into(),
                "diskpart".into(),
            ],

            first_run_done: false,
            data_dir: String::new(),
        }
    }
}

// ============================================================
// 系统信息 / 导入导出
// ============================================================

/// 应用信息，供「关于」页展示
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub name: String,
    pub tauri_version: String,
    pub os: String,
    pub arch: String,
    /// 数据目录
    pub data_dir: String,
    /// 数据库文件路径
    pub db_path: String,
    /// 是否首次运行
    pub first_run: bool,
    pub user_name: String,
}

/// 导入 JSON 时使用的模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ImportMode {
    /// 覆盖：清空现有数据后导入
    Replace,
    /// 合并：同名以导入文件为准，同 ID 保留最新
    Merge,
    /// 追加：只插入数据库里不存在的 ID
    Append,
}

/// 导出内容格式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Json,
    Csv,
}

/// 导入结果报告
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub groups_added: i32,
    pub groups_updated: i32,
    pub presets_added: i32,
    pub presets_updated: i32,
    pub workflows_added: i32,
    pub schedules_added: i32,
    /// 导入过程中跳过的问题（重复 ID、非法字段……）
    pub warnings: Vec<String>,
    pub settings_imported: bool,
}

/// 导出文件内容（由前端负责写文件或调用保存对话框）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBundle {
    /// 文件格式版本号
    pub version: String,
    pub exported_at: i64,
    pub app_version: String,
    pub settings: AppSettings,
    pub groups: Vec<Group>,
    pub presets: Vec<Preset>,
    pub workflows: Vec<Workflow>,
    pub schedules: Vec<Schedule>,
}

// ============================================================
// 工具函数
// ============================================================

/// 当前 Unix 毫秒时间戳
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 生成 uuid v4（去掉连字符，便于粘贴）
pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cmd_uses_comspec() {
        let opt = SpawnOptions {
            runtime: RuntimeBinding::default(),
            preset_id: String::new(),
            title: String::new(),
            kind: preset_kind::CMD.into(),
            program: "echo".into(),
            args: vec!["hello".into()],
            working_dir: String::new(),
            env: vec![],
            use_shell: true,
            elevated: false,
            interactive: false,
            cols: 80,
            rows: 24,
            source: run_source::MANUAL.into(),
        };
        let (prog, args) = opt.resolve();
        assert!(prog.to_lowercase().contains("cmd"));
        assert_eq!(args[0], "/d");
        assert_eq!(args[1], "/c");
        assert!(args[2].contains("echo"));
    }

    #[test]
    fn resolve_exe_direct() {
        let opt = SpawnOptions {
            runtime: RuntimeBinding::default(),
            preset_id: String::new(),
            title: String::new(),
            kind: preset_kind::EXE.into(),
            program: r"C:\tools\git\git.exe".into(),
            args: vec!["--version".into()],
            working_dir: String::new(),
            env: vec![],
            use_shell: false,
            elevated: false,
            interactive: false,
            cols: 80,
            rows: 24,
            source: run_source::MANUAL.into(),
        };
        let (prog, args) = opt.resolve();
        assert_eq!(prog, r"C:\tools\git\git.exe");
        assert_eq!(args, vec!["--version".to_string()]);
    }

    #[test]
    fn quote_if_needed_only_quotes_with_spaces() {
        assert_eq!(quote_if_needed("abc"), "abc");
        assert_eq!(quote_if_needed("a b"), "\"a b\"");
    }
}
