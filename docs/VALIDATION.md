# 验证记录

1.2.1 的最新修复与实际验收范围见 [1.2.1 验收说明](V1.2.1.md)。以下保留此前版本记录，交互输入、管理员任务与多窗口行为以最新说明为准。

验证环境：Windows x64、Rust stable MSVC、Node.js、WebView2。日期：2026-10-06、2026-10-07。

## 已执行

- `npm run build`：TypeScript 检查和 Vite 生产构建通过。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml -j 2`：v1.1.0 的 60 项测试通过，1 项文档示例忽略。
- 在真实 Tauri WebView2 窗口中新建、编辑并运行 PowerShell 预设，终端显示中文、占位符和环境变量，退出码为 0。
- 使用 `scripts/runtime-smoke.cjs` 连接真实窗口验证 CMD、PowerShell、Python、Node、自定义 EXE，检查编辑/收藏/删除、并发/停止、批量、串行工作流、定时任务手动触发、JSON 导入导出、审计和历史。
- 后端拒绝黑名单指令；带确认标记的预设在未确认时不能创建进程。
- v1.1.0 真实窗口检查检测工具、已有 Conda 环境和项目 `.venv`；Python / Node / Conda 绑定、环境落库、pip list、隔离目录的 venv 创建、多行 PowerShell 相对路径均通过。
- 1 分钟间隔任务由调度器自动触发所选虚拟环境，退出码为 0，没有调用手动触发接口。
- Windows 托盘对象创建成功；关闭主窗口后窗口不可见，调度器保持运行；连续启停调度器通过。停止含有子进程的 PowerShell 后，该子进程也不存在。
- 输出压力检查约 2.48 MB：334 个数据事件、2 次尺寸同步（包含标签初始化）；截断后前端缓存 899016 字符，尾部标记在真实 xterm 中显示。同步次数通过应用实际使用的 `terminalApi.resize` 统计，避免替换 Tauri 只读 IPC 属性导致无效计数。此数据证明输出路径修复，不能当作跨电脑的速度提升比例。
- 初始公开提交 `ac12673` 的 GitHub Actions 构建成功，生成 EXE 以及 zh-CN / en-US MSI；安装包和 SHA256 清单已下载核对。
- 终端空状态按钮在 1380 和 1024 像素窗口下检查通过：文字完整位于按钮内部，两个按钮没有重合。通用按钮按实际文字插槽决定宽度。
- 在真实窗口打开环境管理页，从项目虚拟环境点击“新建预设”，编辑器正确选择 Python 类型及该虚拟环境。
- v1.1.0 标签的 Actions 构建及自动发布成功，公开 EXE/MSI 下载后的 SHA256 与发布清单一致；发布版真实窗口的 19 项核心检查通过。
- 未发布更新已检查 GUI 的 GitHub 图标、Windows 资源中的新应用图标和真实窗口标题栏的新图标。打开数据目录改为调用资源管理器；目录本身存在且可访问。

运行验收使用独立 `CMDDECK_DATA_DIR`，没有读取或导出用户正式预设。该工具执行环境的默认 AppData 写入曾失败，因此这里的运行结论针对可写独立目录。

## 重复验证

先启动隔离实例，再连接 WebView2 调试端口。以下命令只适用于开发验收，不建议日常开启调试端口：

```powershell
$env:CMDDECK_DATA_DIR = Join-Path $PWD 'test-results\runtime'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9223'
npm run tauri:dev
```

另一个 PowerShell 窗口执行：

```powershell
npx --yes --package @playwright/cli playwright-cli -s=cmddeck attach --cdp http://127.0.0.1:9223
npx --yes --package @playwright/cli playwright-cli -s=cmddeck run-code --filename scripts/runtime-smoke.cjs
npx --yes --package @playwright/cli playwright-cli -s=cmddeck run-code --filename scripts/environment-smoke.cjs
```

脚本会新建验收预设并写入隔离目录，要求 Python 和 Node 在 PATH 中；请勿连接存有私人预设的实例。环境脚本的 venv 检查需要先在 `test-results\runtime\env-fixture` 内创建 `.venv`，没有 Conda 或 venv 时会明确记录跳过。

## 验证边界

当前工具启动的进程修改 Windows 启动项返回“拒绝访问”，因此 **开机自启注册与真实登录启动未验收通过**；已验证失败时不会写入成功标志。没有修改系统 ACL 或关闭系统安全功能。全新 Windows 虚拟机安装/卸载、UAC 提权、物理托盘菜单点击和物理快捷键按键尚未验证。定时任务依赖应用持续运行，退出后不补跑；终端交互输入不受预设黑名单约束。

本机未安装 fnm / nvm / Volta，相关目录检测和安装命令只完成代码与构建检查；没有假称真实安装了新 Node 版本。安装包未代码签名。

Windows 安装包构建结果以 [GitHub Actions](https://github.com/jgbrzzh/CmdDeck/actions/workflows/windows.yml) 的对应提交为准；签名和自动更新尚未配置。
