/**
 * 主题 composable。
 *
 * 真正的 DOM 应用逻辑在 `settingsStore.applyAll()` 里（它同时管字号、
 * 终端字体、语言属性）。这里只做「把 settingsStore 的 theme 变成一个
 * 组件可以直接 watch 的 ref」，并提供切换方法。
 *
 * 用法（App.vue 里调用一次即可，全站都会生效）：
 * ```ts
 * const { theme, effective, setTheme, toggle } = useTheme()
 * ```
 */

import { computed, watch, type ComputedRef, type Ref } from "vue";

import { useSettingsStore } from "@/stores/settings";
import { useUiStore } from "@/stores/ui";
import type { AppSettings } from "@/types";

export interface UseThemeReturn {
  /** 设置里存的原始值：dark / light / system */
  theme: Ref<AppSettings["theme"]>;
  /** 解析后的实际主题（system 已折叠成 dark/light） */
  effective: Ref<"dark" | "light">;
  /** effective 的别名，方便组件直接 v-bind */
  resolved: ComputedRef<"dark" | "light">;
  /** 设置主题并落库 */
  setTheme(t: AppSettings["theme"]): Promise<void>;
  /** 在 dark / light 之间来回切（落库） */
  toggle(): Promise<void>;
  /** 只改本地不落库，用于需要零延迟的场景 */
  setThemeLocal(t: AppSettings["theme"]): void;
}

let applied = false;

export function useTheme(): UseThemeReturn {
  const settingsStore = useSettingsStore();
  const ui = useUiStore();

  const theme = computed<AppSettings["theme"]>(
    () => settingsStore.settings.value.theme,
  );
  const effective = ui.theme;

  // settingsStore.applyAll 已经写过 data-theme，这里再做一次兜底，
  // 覆盖「设置未加载完就渲染」的瞬间（避免白屏闪一下）
  if (!applied) {
    applied = true;
    watch(
      effective,
      (t) => {
        if (typeof document !== "undefined") {
          document.documentElement.setAttribute("data-theme", t);
        }
      },
      { immediate: true },
    );
  }

  /** 设置主题并落库 */
  async function setTheme(t: AppSettings["theme"]): Promise<void> {
    // 先本地生效，避免等 IPC 返回导致的主题闪烁
    settingsStore.patchLocal({ theme: t });
    await settingsStore.setTheme(t);
  }

  /** 只改本地不落库 */
  function setThemeLocal(t: AppSettings["theme"]): void {
    settingsStore.patchLocal({ theme: t });
  }

  /** 切换 dark / light（当前是 system 时按系统实际值取反） */
  async function toggle(): Promise<void> {
    await setTheme(effective.value === "dark" ? "light" : "dark");
  }

  return {
    theme,
    effective,
    resolved: computed(() => effective.value),
    setTheme,
    setThemeLocal,
    toggle,
  };
}
