# 验证记录

验证环境：Windows x64、Rust stable MSVC、Node.js、WebView2。日期：2026-10-06。

## 已执行

- `npm run build`：TypeScript 检查和 Vite 生产构建通过。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml -j 2`：55 项测试通过，1 项文档示例忽略。
- 在真实 Tauri WebView2 窗口中新建、编辑并运行 PowerShell 预设，终端显示中文、占位符和环境变量，退出码为 0。
- 使用 `scripts/runtime-smoke.cjs` 连接真实窗口验证 CMD、PowerShell、Python、Node、自定义 EXE，检查编辑/收藏/删除、并发/停止、批量、串行工作流、定时任务手动触发、JSON 导入导出、审计和历史。
- 后端拒绝黑名单指令；带确认标记的预设在未确认时不能创建进程。

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
```

脚本会新建验收预设并写入隔离目录，要求 Python 和 Node 在 PATH 中；请勿连接存有私人预设的实例。

## 验证边界

未完成全新 Windows 虚拟机上的安装/卸载、UAC 提权、开机登录自启和物理快捷键验收。单元测试和手动触发不能代替这些系统级验收。定时任务依赖应用持续运行，退出后不补跑；终端交互输入不受预设黑名单约束。

Windows 安装包构建结果以 [GitHub Actions](https://github.com/jgbrzzh/CmdDeck/actions/workflows/windows.yml) 的对应提交为准；签名和自动更新尚未配置。
