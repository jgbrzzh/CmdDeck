import { invoke } from "@tauri-apps/api/core";
import type { Preset, ImportReport, TerminalHistory } from "@/types";
import type {
  Productivity,
  Layout,
  Preflight,
  TaskInfo,
  PortInfo,
  BackupInfo,
  ShareOptions,
  UpdateInfo,
} from "@/types/productivity";
export const productivityApi = {
  get: () => invoke<Productivity>("get_productivity"),
  save: (config: Productivity) =>
    invoke<Productivity>("save_productivity", { config }),
  layout: (workspaceId: string, layout: Layout) =>
    invoke<void>("save_terminal_layout", { workspaceId, layout }),
  preflight: (
    preset: Preset,
    args: Record<string, string>,
    workspaceId: string,
  ) => invoke<Preflight>("preflight_preset", { preset, args, workspaceId }),
  tasks: () => invoke<TaskInfo[]>("list_task_metrics"),
  ports: () => invoke<PortInfo[]>("list_listening_ports"),
  openService: (port: number) => invoke<void>("open_local_service", { port }),
  backups: () => invoke<BackupInfo[]>("list_config_backups"),
  backup: () => invoke<string>("create_config_backup"),
  restore: (id: string, confirmed: boolean) =>
    invoke<ImportReport>("restore_config_backup", { id, confirmed }),
  preview: (options: ShareOptions) =>
    invoke<string>("preview_config_export", { options }),
  export: (path: string, options: ShareOptions, preview: string) =>
    invoke<void>("export_shared_config", { path, options, preview }),
  historyOutput: (id: number) => invoke<string>("get_history_output", { id }),
  logEntries: () => invoke<TerminalHistory[]>("list_log_entries"),
  exportLog: (
    path: string,
    sessionId: string | null,
    historyId: number | null,
  ) => invoke<void>("export_task_log", { path, sessionId, historyId }),
  testNotification: () => invoke<void>("test_task_notification"),
  checkUpdate: () => invoke<UpdateInfo>("check_for_updates"),
  releases: () => invoke<void>("open_releases"),
};
