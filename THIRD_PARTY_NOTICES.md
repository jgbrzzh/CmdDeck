# 第三方依赖

CmdDeck 自有代码采用 GPL-3.0-only。依赖、Windows 系统组件与开发工具保留各自许可证。

- Tauri / Wry / Tao：MIT 或 Apache-2.0，提供 WebView 窗口和系统集成。
- Vue：MIT，提供界面组件。
- xterm.js 及官方插件：MIT，提供终端渲染。
- portable-pty：MIT，提供伪终端。
- rusqlite：MIT，SQLite 原始实现属于公共领域。
- Rust 与 npm 的完整锁定依赖信息见 [依赖清单](docs/DEPENDENCIES.md)。项目使用的第三方许可文件汇总于 [许可原文](docs/THIRD_PARTY_LICENSES.txt)。

依赖清单包含构建工具和目标平台依赖，不能据此判断每一项都进入最终安装包。依赖升级时请重新检查许可和原始版权声明。

Microsoft Edge WebView2、Visual C++、Windows 和 NSIS/WiX 构建工具使用各自许可；CmdDeck 的 GPL 不改变这些组件的许可。安装包通过 Tauri 的引导程序检测和安装 WebView2。
