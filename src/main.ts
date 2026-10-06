/**
 * 应用入口。
 *
 * 顺序很重要：
 * 1. 先挂载 App（让界面立刻出来）
 * 2. 再注册全局错误处理 —— 否则 App 创建过程中的异常无处可弹
 */

import { createApp } from "vue";

import App from "@/App.vue";
import { toFriendlyError } from "@/api";
import { useUiStore } from "@/stores/ui";

// 全局样式：变量 → 重置 → 动画 → xterm
import "@/styles/variables.css";
import "@/styles/base.css";
import "@/styles/transitions.css";
import "@/styles/xterm.css";

const app = createApp(App);
app.mount("#app");

/**
 * 注册全局错误处理。
 *
 * 桌面端没有浏览器控制台给你看，未捕获异常如果只 `console.error`
 * 用户完全无感 —— 这里统一弹成 error toast，停留 8 秒。
 *
 * @param source 错误来源，用于 toast 文案
 */
function reportError(source: string, raw: unknown): void {
  const err = toFriendlyError(raw);
  console.error(`[CmdDeck] ${source}`, err);
  try {
    useUiStore().toast("error", "程序错误", `${source}：${err.message}`, 8000);
  } catch {
    // store 还没准备好时（例如模块加载阶段出错）只能忽略
  }
}

// 同步异常
window.addEventListener("error", (e: ErrorEvent) => {
  // 资源加载错误（img/script 404）没有 message，交给浏览器默认处理
  if (!e.message) return;
  reportError("运行出错", e.error ?? e.message);
});

// 异步 Promise 未捕获
window.addEventListener("unhandledrejection", (e: PromiseRejectionEvent) => {
  reportError("异步操作失败", e.reason);
});
