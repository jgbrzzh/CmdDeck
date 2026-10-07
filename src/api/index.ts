/**
 * 后端命令调用封装。
 *
 * 所有 `invoke` 都集中在这里，组件不直接 import `@tauri-apps/api`，
 * 好处是：错误提示统一、参数默认值统一、后续换后端只改这一个文件。
 */

import { invoke } from "@tauri-apps/api/core";

import type {
  AppInfo,
  AppSettings,
  AuditFilter,
  AuditLog,
  BatchResult,
  ExportBundle,
  ImportReport,
  Placeholder,
  Preset,
  PresetFilter,
  PresetGroup,
  Schedule,
  SecurityVerdict,
  ShellOption,
  SpawnOptions,
  TerminalHistory,
  TerminalInfo,
  Workflow,
  WorkflowRun,
} from "@/types";

/** 后端返回的错误结构（与 Rust 端 `AppError::serialize` 对应） */
export interface BackendError {
  kind:
    | "NotFound"
    | "Validation"
    | "Conflict"
    | "Io"
    | "Exec"
    | "Blocked"
    | "Permission"
    | "Other";
  message: string;
}

/** 把后端抛出的错误统一转换成可展示的中文 Error */
export function toFriendlyError(err: unknown): Error {
  if (err && typeof err === "object" && "message" in err) {
    const e = err as BackendError;
    return new Error(e.message || "操作失败，请重试");
  }
  if (typeof err === "string") return new Error(err);
  return new Error("操作失败，请重试");
}

/** 统一的 invoke 包装：自动把后端错误转成 Error 对象抛出 */
async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    throw toFriendlyError(err);
  }
}

// ============================================================
// 预设指令
// ============================================================

export const presetApi = {
  list: (filter?: PresetFilter) =>
    call<Preset[]>("list_presets", { filter: filter ?? null }),
  get: (id: string) => call<Preset>("get_preset", { id }),
  save: (preset: Preset) => call<Preset>("save_preset", { preset }),
  remove: (id: string) => call<void>("delete_preset", { id }),
  removeMany: (ids: string[]) => call<void>("delete_presets", { ids }),
  duplicate: (id: string) => call<Preset>("duplicate_preset", { id }),
  /** 按给定顺序重排（拖拽排序后调用），ids 为该分组内全部预设 ID */
  reorder: (ids: string[]) => call<void>("reorder_presets", { ids }),
  toggleFavorite: (id: string) =>
    call<Preset>("toggle_preset_favorite", { id }),
  recordRun: (id: string) => call<Preset>("record_preset_run", { id }),
  move: (id: string, groupId: string, sortOrder: number) =>
    call<void>("move_preset", { id, groupId, sortOrder }),
  /** 扫描文本中的 {{key}} 占位符 */
  scanPlaceholders: (text: string) =>
    call<Placeholder[]>("scan_placeholders", { text }),
  /** 按给定参数预览最终命令行 */
  preview: (preset: Preset, args: Record<string, string>) =>
    call<string>("preview_command", { preset, args }),
};

// ============================================================
// 分组
// ============================================================

export const groupApi = {
  list: () => call<PresetGroup[]>("list_groups"),
  save: (group: PresetGroup) => call<PresetGroup>("save_group", { group }),
  /** 删除分组，返回被移出该分组的预设数量 */
  remove: (id: string) => call<number>("delete_group", { id }),
};

// ============================================================
// 终端
// ============================================================

export const terminalApi = {
  spawn: (options: SpawnOptions) =>
    call<TerminalInfo>("spawn_terminal", { options }),
  /** 打开一个指定类型的交互式 Shell */
  openShell: (kind: string) => call<TerminalInfo>("open_shell", { kind }),
  /** 运行一条预设（内部会做安全检查 + 写审计日志） */
  runPreset: (
    presetId: string,
    args: Record<string, string>,
    source = "manual",
    confirmed = false,
  ) => call<TerminalInfo>("run_preset", { presetId, args, source, confirmed }),
  write: (sessionId: string, data: string) =>
    call<void>("write_terminal", { sessionId, data }),
  resize: (sessionId: string, cols: number, rows: number) =>
    call<void>("resize_terminal", { sessionId, cols, rows }),
  kill: (sessionId: string) => call<void>("kill_terminal", { sessionId }),
  close: (sessionId: string) => call<void>("close_terminal", { sessionId }),
  /** 取回缓冲内容，用于重新打开标签页时恢复现场 */
  snapshot: (sessionId: string) =>
    call<string>("get_terminal_snapshot", { sessionId }),
  list: () => call<TerminalInfo[]>("list_terminal_sessions"),
  /** 等待退出码，最长 timeoutMs；超时返回 null */
  waitExit: (sessionId: string, timeoutMs = 60000) =>
    call<number | null>("wait_terminal_exit", { sessionId, timeoutMs }),
};

// ============================================================
// 批量执行
// ============================================================

export const batchApi = {
  run: (
    presetIds: string[],
    argsMap: Record<string, string>,
    concurrency: number,
    continueOnError: boolean,
  ) =>
    call<BatchResult>("run_batch", {
      presetIds,
      argsMap,
      concurrency,
      continueOnError,
    }),
};

// ============================================================
// 定时任务
// ============================================================

export const scheduleApi = {
  list: () => call<Schedule[]>("list_schedules"),
  save: (schedule: Schedule) => call<Schedule>("save_schedule", { schedule }),
  remove: (id: string) => call<void>("delete_schedule", { id }),
  setEnabled: (id: string, enabled: boolean) =>
    call<void>("set_schedule_enabled", { id, enabled }),
  start: () => call<boolean>("start_scheduler"),
  stop: () => call<boolean>("stop_scheduler"),
  triggerNow: (id: string) => call<string>("trigger_schedule_now", { id }),
};

// ============================================================
// 工作流
// ============================================================

export const workflowApi = {
  list: () => call<Workflow[]>("list_workflows"),
  save: (workflow: Workflow) => call<Workflow>("save_workflow", { workflow }),
  remove: (id: string) => call<void>("delete_workflow", { id }),
  start: (id: string, source = "manual") =>
    call<WorkflowRun>("start_workflow", { id, source }),
  listRuns: (workflowId: string, limit = 20) =>
    call<WorkflowRun[]>("list_workflow_runs", { workflowId, limit }),
  cancel: (runId: string) => call<void>("cancel_workflow", { runId }),
};

// ============================================================
// 设置 / 系统
// ============================================================

export const systemApi = {
  integrationStatus: () =>
    call<{ autostart: boolean; tray: boolean; scheduler: boolean }>(
      "get_integration_status",
    ),
  appInfo: () => call<AppInfo>("get_app_info"),
  getSettings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) =>
    call<AppSettings>("save_settings", { settings }),
  resetSettings: () => call<AppSettings>("reset_settings"),
  openDataDir: () => call<void>("open_data_dir"),
  openRepository: () => call<void>("open_project_repository"),
  openPath: (path: string) => call<void>("open_path", { path }),
  revealPath: (path: string) => call<void>("reveal_path", { path }),
  /** 导出为 JSON 文件，返回导出的预设数量 */
  exportData: (path: string) => call<number>("export_data", { path }),
  importData: (path: string, mode: "replace" | "merge" | "append") =>
    call<ImportReport>("import_data", { path, mode }),
  setGlobalHotkey: (accelerator: string) =>
    call<void>("set_global_hotkey", { accelerator }),
  setAutostart: (enabled: boolean) => call<void>("set_autostart", { enabled }),
  shells: () => call<ShellOption[]>("get_system_shells"),
  isElevated: () => call<boolean>("is_elevated"),
  seedDefaults: () => call<number>("seed_default_data"),
};

// ============================================================
// 安全
// ============================================================

export const securityApi = {
  check: (preset: Preset, args: Record<string, string>) =>
    call<SecurityVerdict>("check_command", { preset, args }),
  getBlacklist: () => call<string[]>("get_blacklist"),
  addBlacklist: (pattern: string) =>
    call<string[]>("add_blacklist", { pattern }),
  removeBlacklist: (pattern: string) =>
    call<string[]>("remove_blacklist", { pattern }),
};

// ============================================================
// 审计 / 历史
// ============================================================

export const auditApi = {
  list: (filter?: AuditFilter) =>
    call<AuditLog[]>("list_audit_logs", { filter: filter ?? null }),
  clear: () => call<void>("clear_audit_logs"),
  history: (presetId = "", limit = 200) =>
    call<TerminalHistory[]>("list_terminal_history", { presetId, limit }),
  clearHistory: () => call<void>("clear_terminal_history"),
};

// ============================================================
// 导出数据结构（供"查看导出的 JSON"使用）
// ============================================================

export type { ExportBundle };
