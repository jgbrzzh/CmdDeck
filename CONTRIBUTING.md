# 参与贡献

感谢帮助完善 CmdDeck。请先通过 Issue 描述问题和复现步骤；较大的功能改动先讨论接口和使用场景。

## 开发环境

Windows 10 1809+ / Windows 11 x64、Node.js 24 LTS、Rust stable MSVC、Visual Studio C++ Build Tools、Windows SDK 和 WebView2。完整安装步骤见 [README](README.md)。

```powershell
npm ci
npm run tauri:dev
```

提交前运行：

```powershell
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path src-tauri/Cargo.toml -j 2
```

修改终端、快捷键、托盘、开机启动或调度时，还需要真实桌面验收。编译通过不能代替交互验收。请在 PR 中写明测试环境、操作和结果，勿提交数据库、环境变量、令牌、运行日志、安装缓存或生成目录。

`AGENTS.md` 是本地私人指令，不进入公开仓库。个人预设、配置导出 JSON、环境路径清单和验收数据库都留在本地；公开示例仅来自 `src-tauri/src/seed.rs`，请逐项核对暂存文件，勿直接全目录暂存。

## 代码和提交约定

- 前端 Vue 3 + TypeScript；后端 Rust。SQL 放在 `src-tauri/src/db`，IPC 放在 `commands`。
- 注释用中文，优先解释原因与边界。前后端事件名、参数名和数据类型保持一致。
- 新增数据库结构通过递增迁移实现，保护现有用户数据。
- 所有运行入口都必须经过后端安全检查；不要绕开确认、黑名单或管理员权限检查。
- 提交使用 Conventional Commits，例如 `feat(core): 增加预设参数校验`、`fix(terminal): 修复停止进程`、`docs(readme): 更新安装说明`。

贡献即表示你有权提交该代码，并同意以本项目的 GPL-3.0-only 许可证发布。无需版权转让。
