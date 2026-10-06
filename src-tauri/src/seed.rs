//! 首次启动的示例数据。
//!
//! ## 设计原则
//!
//! 新用户打开软件看到空白界面会懵，所以第一次运行要"有东西可看、可点"。
//! 但示例数据也是负担：
//!
//! * **必须幂等**——只有 `presets` 表为空时才写，否则每次启动都在塞数据；
//! * **必须安全**——示例里一条黑名单级别的危险命令都没有；
//!   唯一那条高危预设（结束进程）一定要先弹确认框、还要用户填进程 ID；
//! * **必须能看懂**——覆盖 PowerShell / CMD / Python / Node / 外部程序 / 交互式 Shell
//!   六种执行类型，让用户照着改就能写出自己的预设。
//!
//! `settings.first_run_done` 保持 `false`，由前端引导页确认后再置 `true`
//! （写入逻辑在 `save_settings`，不在这里）。

use tauri::AppHandle;
use tauri::Manager;

use crate::db::groups;
use crate::db::models::{new_id, now_ms, preset_kind, Group, Placeholder, Preset};
use crate::db::presets;
use crate::db::Db;
use crate::error::AppResult;

// ============================================================
// 小构造器：让示例数据的声明读起来像配置而不是一堆字段赋值
// ============================================================

/// 预设构造器：链式调用，只写和默认值不一样的字段
struct SeedPreset {
    p: Preset,
}

impl SeedPreset {
    fn new(name: &str, kind: &str, group_id: &str, icon: &str) -> Self {
        let now = now_ms();
        Self {
            p: Preset {
                id: new_id(),
                name: name.to_string(),
                kind: kind.to_string(),
                program: String::new(),
                args: Vec::new(),
                working_dir: String::new(),
                env: Vec::new(),
                // 默认走 Shell 包装，这样用户改参数时能直接用 | > && 这些语法
                use_shell: true,
                icon: icon.to_string(),
                group_id: group_id.to_string(),
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
            },
        }
    }

    fn program(mut self, v: &str) -> Self {
        self.p.program = v.to_string();
        self
    }
    fn args(mut self, v: &[&str]) -> Self {
        self.p.args = v.iter().map(|s| s.to_string()).collect();
        self
    }
    fn no_shell(mut self) -> Self {
        self.p.use_shell = false;
        self
    }
    fn tags(mut self, v: &[&str]) -> Self {
        self.p.tags = v.iter().map(|s| s.to_string()).collect();
        self
    }
    fn notes(mut self, v: &str) -> Self {
        self.p.notes = v.to_string();
        self
    }
    fn confirm(mut self) -> Self {
        self.p.confirm = true;
        self
    }
    fn elevated(mut self) -> Self {
        self.p.elevated = true;
        self
    }
    fn danger(mut self, level: i32) -> Self {
        self.p.danger_level = level;
        self
    }
    fn favorite(mut self) -> Self {
        self.p.favorite = true;
        self
    }
    fn placeholders(mut self, v: Vec<Placeholder>) -> Self {
        self.p.placeholder_args = v;
        self
    }
    fn build(self) -> Preset {
        self.p
    }
}

/// 占位符定义
fn ph(key: &str, label: &str, input_type: &str, required: bool) -> Placeholder {
    Placeholder {
        key: key.to_string(),
        label: label.to_string(),
        input_type: input_type.to_string(),
        default_value: String::new(),
        options: Vec::new(),
        required,
        help: String::new(),
    }
}

/// 带候选项 / 说明 / 默认值的占位符
fn ph_full(
    key: &str,
    label: &str,
    input_type: &str,
    required: bool,
    default_value: &str,
    options: &[&str],
    help: &str,
) -> Placeholder {
    Placeholder {
        key: key.to_string(),
        label: label.to_string(),
        input_type: input_type.to_string(),
        default_value: default_value.to_string(),
        options: options.iter().map(|s| s.to_string()).collect(),
        required,
        help: help.to_string(),
    }
}

// ============================================================
// 分组
// ============================================================

/// 四个示例分组，返回 `(名称, 图标, 颜色)`
const SEED_GROUPS: [(&str, &str, &str); 4] = [
    ("常用工具", "wrench", "#4f9cf9"),
    ("开发环境", "code", "#39c07f"),
    ("系统运维", "server", "#f2a33c"),
    ("网络与服务", "globe", "#a97bf0"),
];

// ============================================================
// 主入口
// ============================================================

/// 首次启动写入示例数据，返回写入的预设数量。
///
/// 已有任何预设时直接返回 `0`（幂等）。
pub fn seed_default_data(app: &AppHandle) -> AppResult<i32> {
    let state = app.state::<crate::state::AppState>();
    let count = presets::count(&state.db)?;
    if count > 0 {
        log::info!("已有 {count} 条预设，跳过示例数据写入");
        return Ok(0);
    }

    let version = app.package_info().version.to_string();
    let written = seed_into(&state.db)?;
    log::info!("CmdDeck {version} 首次运行：已写入 {written} 条示例预设");
    Ok(written)
}

/// 把示例数据写进指定数据库（单元测试用得着）
fn seed_into(db: &Db) -> AppResult<i32> {
    let now = now_ms();

    // ---- 1. 分组 ----
    let mut group_ids: Vec<String> = Vec::new();
    for (idx, (name, icon, color)) in SEED_GROUPS.iter().enumerate() {
        let g = groups::save(
            db,
            &Group {
                id: new_id(),
                name: name.to_string(),
                icon: icon.to_string(),
                color: color.to_string(),
                sort_order: idx as i32,
                collapsed: false,
                created_at: now,
                updated_at: now,
            },
        )?;
        group_ids.push(g.id);
    }
    let (g_tools, g_dev, g_ops, g_net) = (
        group_ids[0].clone(),
        group_ids[1].clone(),
        group_ids[2].clone(),
        group_ids[3].clone(),
    );

    // ---- 2. 预设（组内顺序即列表顺序）----
    let items: Vec<Preset> = vec![
        // ================= 常用工具 =================
        SeedPreset::new("查看系统信息", preset_kind::POWERSHELL, &g_tools, "info")
            .program("systeminfo")
            .tags(&["系统", "信息"])
            .notes("查看本机 Windows 版本、补丁、内存、启动项等详细信息")
            .favorite()
            .build(),
        SeedPreset::new(
            "磁盘空间统计",
            preset_kind::POWERSHELL,
            &g_tools,
            "hard-drive",
        )
        .program("Get-PSDrive")
        .args(&["-PSProvider", "FileSystem"])
        .tags(&["磁盘", "系统"])
        .notes("列出所有磁盘分区的容量与剩余空间")
        .build(),
        SeedPreset::new("清理 DNS 缓存", preset_kind::CMD, &g_tools, "refresh")
            .program("ipconfig")
            .args(&["/flushdns"])
            .tags(&["网络", "系统"])
            .notes("访问不到网站时先试这一条，清掉本机缓存的 DNS 记录")
            .build(),
        SeedPreset::new("Ping 测试连通性", preset_kind::CMD, &g_tools, "globe")
            .program("ping")
            .args(&["-n", "4", "www.baidu.com"])
            .tags(&["网络"])
            .notes("发 4 个包看延迟与丢包，判断网络是否正常")
            .build(),
        SeedPreset::new("查看本机 IP 配置", preset_kind::CMD, &g_tools, "list")
            .program("ipconfig")
            .args(&["/all"])
            .tags(&["网络"])
            .notes("查看网卡、IP、网关、DNS 等完整配置")
            .build(),
        SeedPreset::new("进程列表", preset_kind::CMD, &g_tools, "grid")
            .program("tasklist")
            .tags(&["系统", "进程"])
            .notes("相当于任务管理器的进程页")
            .build(),
        SeedPreset::new(
            "查找文件位置（自定义）",
            preset_kind::CUSTOM,
            &g_tools,
            "search",
        )
        .program(r"C:\Windows\System32\where.exe")
        .args(&["{{文件名}}"])
        .no_shell()
        .tags(&["自定义", "文件"])
        .notes(
            "「自定义」类型：程序与参数完全由你指定，不做任何推断。\
                这里用系统的 where.exe 演示怎么填绝对路径，\
                换成你自己的 exe 即可。",
        )
        .placeholders(vec![ph_full(
            "文件名",
            "要查找的文件名",
            "text",
            true,
            "notepad.exe",
            &["notepad.exe", "cmd.exe", "code.cmd"],
            "占位符支持中文名字，填什么就照着替换",
        )])
        .build(),
        SeedPreset::new(
            "打开 PowerShell 交互终端",
            preset_kind::SHELL,
            &g_tools,
            "terminal",
        )
        .tags(&["终端", "Shell"])
        .notes("直接开一个 PowerShell 窗口，命令敲完不自动退出")
        .favorite()
        .build(),
        SeedPreset::new(
            "打开 CMD 交互终端",
            preset_kind::SHELL,
            &g_tools,
            "terminal",
        )
        .notes("直接开一个 cmd.exe 窗口")
        .build(),
        SeedPreset::new("打开 Git Bash", preset_kind::SHELL, &g_tools, "git-branch")
            .program("bash")
            .no_shell()
            .tags(&["开发", "Shell"])
            .notes("需要已安装 Git for Windows，并且 bash 在 PATH 中")
            .build(),
        // ================= 开发环境 =================
        SeedPreset::new("Python 版本", preset_kind::PYTHON, &g_dev, "code")
            .program("python")
            .args(&["--version"])
            .no_shell()
            .tags(&["Python"])
            .build(),
        SeedPreset::new("pip 升级自身", preset_kind::PYTHON, &g_dev, "package")
            .program("python")
            .args(&["-m", "pip", "install", "--upgrade", "pip"])
            .no_shell()
            .tags(&["Python", "包管理"])
            .notes("会改动全局 Python 环境，升级后建议重启终端")
            .confirm()
            .danger(1)
            .build(),
        SeedPreset::new("pip 安装 Python 包", preset_kind::PYTHON, &g_dev, "package")
            .program("python")
            .args(&["-m", "pip", "install", "{{package}}"])
            .no_shell()
            .tags(&["Python", "包管理"])
            .notes("运行前会弹窗让你填写包名")
            .placeholders(vec![ph_full(
                "package",
                "包名",
                "text",
                true,
                "requests",
                &["requests", "rich", "httpx", "pyinstaller", "black"],
                "可以直接输入版本，例如 requests==2.31.0",
            )])
            .build(),
        SeedPreset::new("Node 版本", preset_kind::NODE, &g_dev, "code")
            .program("node")
            .args(&["--version"])
            .no_shell()
            .tags(&["Node"])
            .build(),
        SeedPreset::new("npm 安装全局包", preset_kind::NODE, &g_dev, "package")
            .program("npm")
            .args(&["install", "-g", "{{package}}"])
            .no_shell()
            .tags(&["Node", "包管理"])
            .notes("会写入 npm 全局目录，部分机器需要管理员权限")
            .confirm()
            .danger(1)
            .placeholders(vec![ph_full(
                "package",
                "包名",
                "text",
                true,
                "yarn",
                &["yarn", "pnpm", "typescript", "@vue/cli", "electron"],
                "带 @ 的包可以直接输入，例如 @vue/cli",
            )])
            .build(),
        SeedPreset::new("启动本地开发服务器", preset_kind::NODE, &g_dev, "server")
            .program("npm")
            .args(&["run", "dev", "--", "--port", "{{port}}"])
            .no_shell()
            .tags(&["Node", "开发"])
            .notes("在「工作目录」里选好项目根目录再运行")
            .placeholders(vec![ph_full(
                "port",
                "端口号",
                "number",
                true,
                "5173",
                &["3000", "5173", "8080"],
                "端口被占用时换一个",
            )])
            .build(),
        SeedPreset::new("Git 版本", preset_kind::EXE, &g_dev, "git-branch")
            .program("git")
            .args(&["--version"])
            .no_shell()
            .tags(&["Git"])
            .build(),
        SeedPreset::new("VS Code 打开当前目录", preset_kind::EXE, &g_dev, "code")
            .program("code.cmd")
            .args(&["."])
            .no_shell()
            .tags(&["编辑器"])
            .notes("需要命令行里能执行到 code.cmd；没装的话可以改成完整路径")
            .favorite()
            .build(),
        SeedPreset::new("PowerShell 7 版本", preset_kind::PWSH, &g_dev, "terminal")
            .program("pwsh")
            .args(&["-Version"])
            .no_shell()
            .tags(&["PowerShell"])
            .notes("没装 PowerShell 7 的话这条会执行失败，装完可在设置里设为默认 Shell")
            .build(),
        // ================= 系统运维 =================
        SeedPreset::new(
            "列出最大的 20 个文件",
            preset_kind::POWERSHELL,
            &g_ops,
            "list",
        )
        .program("Get-ChildItem")
        .args(&[
            "-Recurse",
            "-File",
            "-ErrorAction",
            "SilentlyContinue",
            "|",
            "Sort-Object",
            "Length",
            "-Descending",
            "|",
            "Select-Object",
            "-First",
            "20",
            "FullName",
            "@{n='SizeMB';e={[math]::Round($_.Length/1MB,2)}}",
        ])
        .tags(&["磁盘", "排查"])
        .notes("会递归扫描「工作目录」下所有文件，建议先切到一个小目录（比如下载文件夹）")
        .confirm()
        .danger(1)
        .build(),
        SeedPreset::new("查看电池健康", preset_kind::POWERSHELL, &g_ops, "cpu")
            .program("Get-CimInstance")
            .args(&["-ClassName", "Win32_Battery"])
            .tags(&["硬件", "笔记本"])
            .notes("台式机没有电池时会没有输出，属于正常现象")
            .build(),
        SeedPreset::new(
            "强制结束进程",
            preset_kind::POWERSHELL,
            &g_ops,
            "alert-triangle",
        )
        .program("Stop-Process")
        .args(&["-Id", "{{pid}}", "-Force"])
        .tags(&["进程", "危险"])
        .notes("危险操作：会立刻杀掉指定进程，未保存的数据会丢失")
        .confirm()
        .danger(2)
        .placeholders(vec![ph("pid", "进程 ID", "number", true)])
        .build(),
        SeedPreset::new("查看开机启动项", preset_kind::POWERSHELL, &g_ops, "power")
            .program("Get-CimInstance")
            .args(&["-ClassName", "Win32_StartupCommand"])
            .tags(&["系统", "开机启动"])
            .notes("排查开机变慢时先看这一条")
            .build(),
        SeedPreset::new("以管理员身份清理 DNS", preset_kind::CMD, &g_ops, "shield")
            .program("ipconfig")
            .args(&["/flushdns"])
            .tags(&["网络", "系统"])
            .notes("需要管理员权限时用这一条，会弹 UAC")
            .confirm()
            .danger(1)
            .elevated()
            .build(),
        // ================= 网络与服务 =================
        SeedPreset::new("查看 8080 端口占用", preset_kind::CMD, &g_net, "server")
            .program("netstat")
            .args(&["-ano", "|", "findstr", ":8080"])
            .tags(&["端口", "网络"])
            .notes("最后面的 PID 可以拿去配合上面的「强制结束进程」")
            .build(),
        SeedPreset::new("查看全部监听端口", preset_kind::CMD, &g_net, "list")
            .program("netstat")
            .args(&["-ano", "-p", "TCP"])
            .tags(&["端口", "网络"])
            .notes("列出所有 TCP 监听端口与对应进程 ID")
            .build(),
        SeedPreset::new("测试 HTTP 端口", preset_kind::POWERSHELL, &g_net, "zap")
            .program("Test-NetConnection")
            .args(&["-ComputerName", "127.0.0.1", "-Port", "{{port}}"])
            .tags(&["端口", "排查"])
            .notes("判断本机某个服务端口有没有起来")
            .placeholders(vec![ph_full(
                "port",
                "端口号",
                "number",
                true,
                "8080",
                &["80", "443", "3000", "5173", "8080"],
                "只测本机服务；测远程主机请把「计算机名」也改掉",
            )])
            .build(),
        SeedPreset::new("WSL 发行版列表", preset_kind::EXE, &g_net, "globe")
            .program("wsl")
            .args(&["-l", "-v"])
            .no_shell()
            .tags(&["Linux", "WSL"])
            .notes("没装 WSL 的话这条会提示找不到命令")
            .build(),
        SeedPreset::new("DNS 解析查询", preset_kind::CMD, &g_net, "search")
            .program("nslookup")
            .args(&["www.baidu.com"])
            .tags(&["网络", "排查"])
            .notes("查某个域名解析到了哪个 IP；解析超时多半是本地 DNS 有问题")
            .build(),
    ];

    // ---- 3. 写库（同分组内 sort_order 递增）----
    let mut per_group: i32 = 0;
    let mut last_group = String::new();
    let mut prepared: Vec<Preset> = Vec::with_capacity(items.len());
    for mut p in items {
        if p.group_id != last_group {
            last_group = p.group_id.clone();
            per_group = 0;
        }
        p.sort_order = per_group;
        per_group += 1;
        prepared.push(p);
    }

    let n = presets::insert_many(db, &prepared)?;
    Ok(n)
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::preset_kind::ALL;

    #[test]
    fn seed_covers_all_kinds_and_requirements() {
        let db = Db::open_in_memory().unwrap();
        let n = seed_into(&db).unwrap();

        assert!(n >= 16, "示例预设至少 16 条，实际 {n}");
        assert_eq!(groups::list(&db).unwrap().len(), 4);

        let all = presets::list(
            &db,
            &crate::db::models::PresetFilter {
                include_hidden: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all.len(), n as usize);

        // 覆盖全部执行类型
        for kind in ALL {
            assert!(
                all.iter().any(|p| p.kind == kind),
                "示例数据缺少执行类型 {kind}"
            );
        }

        // 至少两条带占位符
        assert!(
            all.iter()
                .filter(|p| !p.placeholder_args.is_empty())
                .count()
                >= 2
        );
        // 至少一条「危险 + 二次确认」
        assert!(all.iter().any(|p| p.confirm && p.danger_level == 2));
        // 至少两条收藏
        assert!(all.iter().filter(|p| p.favorite).count() >= 2);
        // 每条都有名称
        assert!(all.iter().all(|p| !p.name.trim().is_empty()));
    }

    #[test]
    fn seed_presets_have_valid_group_and_order() {
        let db = Db::open_in_memory().unwrap();
        seed_into(&db).unwrap();
        let gids: Vec<String> = groups::list(&db)
            .unwrap()
            .into_iter()
            .map(|g| g.id)
            .collect();
        let all = presets::list(
            &db,
            &crate::db::models::PresetFilter {
                include_hidden: true,
                ..Default::default()
            },
        )
        .unwrap();
        for p in &all {
            assert!(
                gids.contains(&p.group_id),
                "预设 {} 挂在不存在的分组上",
                p.name
            );
            assert!(!p.id.is_empty());
            assert!(p.created_at > 0 && p.updated_at > 0);
        }
    }

    #[test]
    fn seed_twice_creates_independent_sets() {
        // 幂等判断在 seed_default_data（按 presets 表是否为空），它需要 AppHandle，没法进单测。
        // 这里验证底层写入两次不会互相覆盖：每次都生成全新 ID，两组数据互不相干。
        let db = Db::open_in_memory().unwrap();
        let a = seed_into(&db).unwrap();
        let b = seed_into(&db).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            presets::count(&db).unwrap(),
            a * 2,
            "两次写入是两组不同 ID 的示例"
        );
    }
}
