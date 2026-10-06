/**
 * 全局设置仓库（模块级单例）。
 *
 * 职责边界：
 * - 数据：完整的 `AppSettings`，从 SQLite 的 kv 表读出
 * - 落库：一律走 `save()`，成功后用后端返回值覆盖本地并 `applyAll`
 * - 副作用：把主题、字号、终端字体写到 `<html>` 上（只碰 DOM 的 inline style / dataset）
 *
 * 为什么不用 pinia：本项目要求零额外依赖，手写单例足够，
 * 而且 `useSettingsStore()` 返回同一份引用，组件里直接解构 ref 即可。
 */

import { ref, type Ref } from "vue";

import { systemApi } from "@/api";
import { defaultSettings } from "@/types";
import type { AppSettings } from "@/types";

export interface SettingsStore {
  /** 当前设置对象（响应式） */
  settings: Ref<AppSettings>;
  /** 是否已成功从后端加载过一次 */
  loaded: Ref<boolean>;
  /** 从后端读取设置；失败时用默认值兜底 */
  load(): Promise<void>;
  /** 合并补丁 → 落库 → 应用；返回最终保存的设置 */
  save(patch: Partial<AppSettings>): Promise<AppSettings>;
  /** 只改内存不落库（主题即时切换等零延迟操作） */
  patchLocal(patch: Partial<AppSettings>): void;
  /** 把一份设置完整应用到 DOM（主题、字号、终端字体） */
  applyAll(s: AppSettings): void;
  /** 恢复出厂设置 */
  reset(): Promise<AppSettings>;
  /** 切换主题的便捷方法，内部等价于 `save({ theme })` */
  setTheme(theme: AppSettings["theme"]): Promise<AppSettings>;
}

// ============================================================
// 模块级单例状态
// ============================================================

const settings = ref<AppSettings>(defaultSettings());
const loaded = ref(false);

/** 解析后的实际主题（system 会被折叠成 dark/light），供其它模块读取 */
const resolvedTheme = ref<"dark" | "light">("dark");

/** 是否已经注册过 matchMedia 监听 */
let mediaQuery: MediaQueryList | null = null;
let mediaHandler: ((e: MediaQueryListEvent) => void) | null = null;

/** 浅合并补丁（数组类字段整体替换，不做元素级合并） */
function mergeSettings(
  base: AppSettings,
  patch: Partial<AppSettings>,
): AppSettings {
  const next: AppSettings = { ...base };
  const writable = next as unknown as Record<string, unknown>;
  for (const key of Object.keys(patch) as (keyof AppSettings)[]) {
    const value = patch[key];
    if (value === undefined) continue;
    // key 与 value 来自同一份 patch，类型天然一致；
    // 这里降级成 Record 写入以绕开联合类型赋值的编译期限制。
    writable[key as string] = value;
  }
  return next;
}

/** 根据系统偏好解析出 dark / light */
function detectSystemTheme(): "dark" | "light" {
  if (typeof window === "undefined" || !window.matchMedia) return "dark";
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

/** 解析设置里的 theme 字段 */
function resolveTheme(theme: AppSettings["theme"]): "dark" | "light" {
  if (theme === "dark" || theme === "light") return theme;
  return detectSystemTheme();
}

/** 把 terminal 字体相关设置写到 CSS 变量上，xterm.css 会消费它 */
function applyTerminalVars(s: AppSettings): void {
  const root = document.documentElement.style;
  root.setProperty("--terminal-font-size", `${s.fontSize}px`);
  root.setProperty("--font-mono", s.fontFamily || defaultSettings().fontFamily);
}

/**
 * 把设置完整应用到 DOM。
 *
 * 只做两件事：
 * 1. `data-theme` 属性 —— variables.css 靠它切换整套颜色
 * 2. `html.style.fontSize` —— 整个界面的 UI 缩放基准
 */
function applyAll(s: AppSettings): void {
  if (typeof document === "undefined") return;

  const root = document.documentElement;
  const dark = resolveTheme(s.theme);
  resolvedTheme.value = dark;
  root.setAttribute("data-theme", dark);
  root.setAttribute("lang", s.locale || "zh-CN");

  // uiScale 是百分比：100% → 16px；125% → 20px
  const scale = Number.isFinite(s.uiScale)
    ? Math.min(200, Math.max(70, s.uiScale))
    : 100;
  root.style.fontSize = `${(scale / 100) * 16}px`;

  applyTerminalVars(s);

  // 系统主题跟随变化
  if (s.theme === "system" && !mediaQuery) {
    if (typeof window !== "undefined" && window.matchMedia) {
      mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
      mediaHandler = (e: MediaQueryListEvent) => {
        if (settings.value.theme !== "system") return;
        resolvedTheme.value = e.matches ? "dark" : "light";
        document.documentElement.setAttribute(
          "data-theme",
          resolvedTheme.value,
        );
      };
      // 老版 WebView2 用 addListener，新版用 addEventListener
      if (mediaQuery.addEventListener)
        mediaQuery.addEventListener("change", mediaHandler);
      else if (mediaQuery.addListener) mediaQuery.addListener(mediaHandler);
    }
  } else if (s.theme !== "system" && mediaQuery && mediaHandler) {
    if (mediaQuery.removeEventListener)
      mediaQuery.removeEventListener("change", mediaHandler);
    else if (mediaQuery.removeListener) mediaQuery.removeListener(mediaHandler);
    mediaQuery = null;
    mediaHandler = null;
  }
}

/** 读取后端设置并应用 */
async function load(): Promise<void> {
  try {
    const s = await systemApi.getSettings();
    settings.value = s;
    applyAll(s);
    loaded.value = true;
  } catch (err) {
    // 后端未就绪 / 数据库损坏时兜底成默认设置，界面照样能用
    console.warn("[CmdDeck] 读取设置失败，已回退到默认设置", err);
    settings.value = defaultSettings();
    applyAll(settings.value);
    loaded.value = true;
  }
}

/** 合并补丁并落库 */
async function save(patch: Partial<AppSettings>): Promise<AppSettings> {
  const merged = mergeSettings(settings.value, patch);
  try {
    // 后端会做字段裁剪与默认值补全，以它返回的为准
    const saved = await systemApi.saveSettings(merged);
    settings.value = saved;
    applyAll(saved);
    return saved;
  } catch (err) {
    console.warn("[CmdDeck] 保存设置失败", err);
    throw err;
  }
}

/** 只改内存 */
function patchLocal(patch: Partial<AppSettings>): void {
  settings.value = mergeSettings(settings.value, patch);
  applyAll(settings.value);
}

/** 恢复默认设置 */
async function reset(): Promise<AppSettings> {
  try {
    const s = await systemApi.resetSettings();
    settings.value = s;
    applyAll(s);
    return s;
  } catch (err) {
    console.warn("[CmdDeck] 恢复默认设置失败", err);
    settings.value = defaultSettings();
    applyAll(settings.value);
    return settings.value;
  }
}

/** 切换主题并落库 */
async function setTheme(theme: AppSettings["theme"]): Promise<AppSettings> {
  return save({ theme });
}

// ============================================================
// 导出
// ============================================================

const store: SettingsStore = {
  settings,
  loaded,
  load,
  save,
  patchLocal,
  applyAll,
  reset,
  setTheme,
};

/** 组件里 `const s = useSettingsStore()` 拿到的是同一份引用 */
export function useSettingsStore(): SettingsStore {
  return store;
}

/** 解析后的实际主题（dark/light），不落库的只读访问 */
export function useResolvedTheme(): Ref<"dark" | "light"> {
  return resolvedTheme;
}
