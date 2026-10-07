# 应用图标

参考 [Windows Terminal 的官方图标](https://github.com/microsoft/terminal/blob/main/res/terminal/Terminal.svg) 和 [Tabby 的官方图标](https://github.com/Eugeny/tabby/blob/master/build/icons/icon.svg)，关注小尺寸的轮廓、对比和终端提示符。

CmdDeck 使用自行绘制的 SVG：青绿渐变底板与叠放的终端卡片表达集中管理，深色前景和较粗的提示符保证任务栏小尺寸下的辨识度。

源文件：`src-tauri/icons/app-icon.svg`。安装依赖后运行 `npm run tauri:icon`，会更新 Windows ICO、32/128/256 PNG 和网页图标，其它平台中间文件只放入忽略的依赖缓存。

修改图标后需要重新编译并重启桌面程序；前端热更新不会改写正在运行的 EXE 图标。构建脚本显式监听图标资源，窗口也显式使用新版图标。已发布的旧安装包不会因源码修改而更新。
