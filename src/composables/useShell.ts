/**
 * 系统 Shell 列表 composable。
 *
 * 后端扫描注册表与 `System32` 找出可用的 Shell（PowerShell / pwsh / cmd /
 * Git Bash / WSL…），前端在"新建终端""预设编辑器""欢迎向导"里都要用，
 * 这里做一层缓存，避免重复 IPC。
 */

import { computed, ref, type ComputedRef, type Ref } from "vue";

import { systemApi, toFriendlyError } from "@/api";
import { useSettingsStore } from "@/stores/settings";
import type { ShellOption } from "@/types";

export interface UseShellReturn {
  shells: Ref<ShellOption[]>;
  loading: Ref<boolean>;
  /** isLoading 的别名 */
  isLoading: Ref<boolean>;
  /** 重新扫描（设置页里"检测 Shell"按钮用） */
  reload(): Promise<ShellOption[]>;
  /** reload 的别名 */
  detect(): Promise<ShellOption[]>;
  /** 当前设置里的默认 Shell id */
  defaultShell: ComputedRef<string>;
  /** 把某个 Shell 设为默认（落库） */
  setDefaultShell(id: string): Promise<void>;
  /** 判断一个预设 kind 是否属于 Shell 类 */
  isShellKind(kind: string): boolean;
  /** 按 id 找 Shell */
  byId(id: string): ShellOption | undefined;
}

/** 视为"交互式 Shell"的预设类型 */
const SHELL_KINDS: string[] = ["shell", "cmd", "powershell", "pwsh"];

const shells = ref<ShellOption[]>([]);
const loading = ref(false);
let cacheValid = false;

export function useShell(): UseShellReturn {
  const settingsStore = useSettingsStore();

  /** 扫描可用 Shell，结果缓存到模块级 ref */
  async function reload(): Promise<ShellOption[]> {
    if (cacheValid && shells.value.length > 0) return shells.value;

    loading.value = true;
    try {
      const list = await systemApi.shells();
      // 可用的排前面；同类按 label 排，保证下拉框顺序稳定
      shells.value = [...list].sort((a, b) => {
        if (a.available !== b.available) return a.available ? -1 : 1;
        return a.label.localeCompare(b.label, "zh-CN");
      });
      cacheValid = true;
    } catch (err) {
      console.warn("[CmdDeck] 读取系统 Shell 列表失败", toFriendlyError(err));
      shells.value = [];
    } finally {
      loading.value = false;
    }
    return shells.value;
  }

  /** 当前默认 Shell id */
  const defaultShell = computed<string>(
    () => settingsStore.settings.value.defaultShell || "powershell",
  );

  /** 设为默认 Shell */
  async function setDefaultShell(id: string): Promise<void> {
    await settingsStore.save({
      defaultShell: id,
      shellPath: byId(id)?.path ?? "",
    });
  }

  function byId(id: string): ShellOption | undefined {
    if (!id) return undefined;
    return shells.value.find((s) => s.id === id);
  }

  function isShellKind(kind: string): boolean {
    return SHELL_KINDS.includes(kind);
  }

  return {
    shells,
    loading,
    isLoading: loading,
    reload,
    detect: reload,
    defaultShell,
    setDefaultShell,
    isShellKind,
    byId,
  };
}

/** 供 App 启动时预热的快捷入口 */
export function preloadShells(): void {
  if (cacheValid) return;
  void systemApi
    .shells()
    .then((list) => {
      shells.value = [...list].sort((a, b) =>
        a.available === b.available ? 0 : a.available ? -1 : 1,
      );
      cacheValid = true;
    })
    .catch(() => undefined);
}
