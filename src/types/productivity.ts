import type { EnvVar, RuntimeBinding, TerminalInfo } from "@/types";
export interface Workspace {
  id: string;
  name: string;
  directory: string;
  python: RuntimeBinding;
  node: RuntimeBinding;
  env: EnvVar[];
  presetIds: string[];
  ports: number[];
}
export interface SavedTab {
  title: string;
  kind: string;
  cwd: string;
  presetId: string;
}
export interface Layout {
  mode: "single" | "columns" | "rows";
  tabs: SavedTab[];
  active: number;
  secondary: number;
}
export interface Productivity {
  workspaces: Workspace[];
  activeWorkspace: string;
  layouts: Record<string, Layout>;
  notificationMode: "off" | "all" | "failure";
  notificationMinSeconds: number;
  restoreLayout: boolean;
  logMaxMegabytes: number;
}
export interface PortInfo {
  address: string;
  port: number;
  pid: number;
  sessionId: string;
}
export interface TaskInfo {
  terminal: TerminalInfo;
  pids: number[];
  memoryBytes: number | null;
  cpuPercent: number | null;
  ports: PortInfo[];
}
export interface Preflight {
  command: string;
  directory: string;
  checks: { level: string; message: string }[];
  canRun: boolean;
}
export interface BackupInfo {
  id: string;
  createdAt: number;
  bytes: number;
}
export interface ShareOptions {
  presetIds: string[];
  includeEnvironment: boolean;
  includePaths: boolean;
  includeSettings: boolean;
}
export interface UpdateInfo {
  current: string;
  latest: string;
  available: boolean;
  notes: string;
  url: string;
  publishedAt: string;
}
