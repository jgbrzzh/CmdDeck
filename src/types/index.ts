/**
 * 全局类型定义（契约层）。
 *
 * ⚠️ 与 `src-tauri/src/db/models.rs` 一一对应，字段全部 camelCase。
 * 修改任何一侧都必须同步另一侧，否则 `invoke` 会静默拿到 undefined。
 */

// ============================================================
// 基础结构
// ============================================================

/** 环境变量键值对 */
export interface EnvVar {
  name: string;
  value: string;
}

/** 运行前弹窗输入项（对应命令里的 {{key}} 占位符） */
export interface Placeholder {
  /** 占位符键名，对应 {{key}} */
  key: string;
  /** 弹窗中显示的中文标签 */
  label: string;
  /** 控件类型 */
  inputType:
    "text" | "number" | "path" | "folder" | "file" | "select" | "password";
  /** 默认值 */
  defaultValue: string;
  /** select 类型的候选项 */
  options: string[];
  /** 是否必填 */
  required: boolean;
  /** 输入框下方的补充说明 */
  help: string;
}

// ============================================================
// 预设指令
// ============================================================

/** 预设执行类型 */
export type PresetKind =
  | "shell"
  | "cmd"
  | "powershell"
  | "pwsh"
  | "python"
  | "node"
  | "exe"
  | "custom";

/** 预设执行类型的中文说明 */
export const PRESET_KIND_LABEL: Record<PresetKind, string> = {
  shell: "交互式 Shell",
  cmd: "CMD 命令",
  powershell: "PowerShell",
  pwsh: "PowerShell 7",
  python: "Python",
  node: "Node.js",
  exe: "外部程序",
  custom: "自定义",
};

/** 全部执行类型，编辑面板下拉框按此顺序展示 */
export const PRESET_KINDS: PresetKind[] = [
  "powershell",
  "cmd",
  "pwsh",
  "python",
  "node",
  "exe",
  "custom",
  "shell",
];

/** 运行来源 */
export type RunSource =
  "manual" | "batch" | "schedule" | "workflow" | "quick" | "shortcut";

/** 运行来源中文说明 */
export const RUN_SOURCE_LABEL: Record<RunSource, string> = {
  manual: "手动运行",
  batch: "批量执行",
  schedule: "定时任务",
  workflow: "工作流",
  quick: "快速启动",
  shortcut: "快捷键",
};

/** 一条预设指令 */
export interface Preset {
  runtime: RuntimeBinding;
  id: string;
  name: string;
  kind: PresetKind;
  /** 主程序：可以是 python、绝对路径 exe，或留空由 kind 推断 */
  program: string;
  /** 参数数组，元素中可含 {{key}} 占位符 */
  args: string[];
  /** 工作目录，空表示用户主目录 */
  workingDir: string;
  env: EnvVar[];
  /** 是否通过 Shell 包装（支持 && | > 等语法） */
  useShell: boolean;
  icon: string;
  groupId: string;
  tags: string[];
  /** 运行前是否二次确认 */
  confirm: boolean;
  /** 是否以管理员身份运行 */
  elevated: boolean;
  /** 危险等级 0 安全 / 1 注意 / 2 危险 */
  dangerLevel: 0 | 1 | 2;
  notes: string;
  sortOrder: number;
  favorite: boolean;
  hidden: boolean;
  /** 绑定的全局快捷键 */
  shortcut: string;
  placeholderArgs: Placeholder[];
  runCount: number;
  /** Unix 毫秒 */
  lastRunAt: number;
  createdAt: number;
  updatedAt: number;
}

/** 新建预设时的默认值 */
export function createEmptyPreset(): Preset {
  const now = Date.now();
  return {
    runtime: { kind: "", path: "", managerPath: "" },
    id: "",
    name: "新预设",
    kind: "powershell",
    program: "",
    args: [],
    workingDir: "",
    env: [],
    useShell: true,
    icon: "terminal",
    groupId: "",
    tags: [],
    confirm: false,
    elevated: false,
    dangerLevel: 0,
    notes: "",
    sortOrder: 0,
    favorite: false,
    hidden: false,
    shortcut: "",
    placeholderArgs: [],
    runCount: 0,
    lastRunAt: 0,
    createdAt: now,
    updatedAt: now,
  };
}

/** 预设列表过滤条件 */
export interface PresetFilter {
  keyword?: string;
  groupId?: string;
  onlyFavorite?: boolean;
  onlyRecent?: boolean;
  includeHidden?: boolean;
  tag?: string;
  kind?: string;
}

/** 危险等级中文说明 */
export const DANGER_LABEL: Record<number, string> = {
  0: "安全",
  1: "注意",
  2: "危险",
};

// ============================================================
// 分组
// ============================================================

export interface PresetGroup {
  id: string;
  name: string;
  icon: string;
  /** #rrggbb，空表示跟随主题 */
  color: string;
  sortOrder: number;
  collapsed: boolean;
  createdAt: number;
  updatedAt: number;
}

export function createEmptyGroup(): PresetGroup {
  const now = Date.now();
  return {
    id: "",
    name: "新分组",
    icon: "folder",
    color: "",
    sortOrder: 0,
    collapsed: false,
    createdAt: now,
    updatedAt: now,
  };
}

// ============================================================
// 终端
// ============================================================

/** 创建终端会话的参数 */
export interface SpawnOptions {
  runtime?: RuntimeBinding;
  presetId?: string;
  title?: string;
  kind?: PresetKind;
  program?: string;
  args?: string[];
  workingDir?: string;
  env?: EnvVar[];
  useShell?: boolean;
  elevated?: boolean;
  /** true=Shell 常驻；false=执行完退出 */
  interactive?: boolean;
  cols?: number;
  rows?: number;
  source?: RunSource;
}

/** 终端会话状态 */
export interface TerminalInfo {
  endedAt?: number;
  workspaceId?: string;
  sessionId: string;
  title: string;
  presetId: string;
  presetName: string;
  kind: string;
  /** 可读的命令行文本 */
  command: string;
  cwd: string;
  startedAt: number;
  /** running / exited / killed */
  status: "running" | "exited" | "killed";
  exitCode: number | null;
  cols: number;
  rows: number;
  elevated: boolean;
  /** 工作流/批量产生的临时会话 */
  temporary: boolean;
}

/** 系统里可用的 Shell */
export interface ShellOption {
  id: string;
  label: string;
  path: string;
  args: string[];
  available: boolean;
}

// ============================================================
// 安全
// ============================================================

export type SecurityLevel = "safe" | "warn" | "danger" | "blocked";

export interface SecurityVerdict {
  level: SecurityLevel;
  /** 命中的风险说明 */
  reasons: string[];
  /** 命中的黑名单关键字 */
  matched: string[];
  requiresConfirm: boolean;
}

export const SECURITY_LEVEL_LABEL: Record<SecurityLevel, string> = {
  safe: "安全",
  warn: "需要留意",
  danger: "高危操作",
  blocked: "已被拦截",
};

// ============================================================
// 审计 / 历史
// ============================================================

export type AuditStatus =
  "success" | "failed" | "killed" | "blocked" | "running";

export interface AuditLog {
  id: number;
  at: number;
  presetId: string;
  presetName: string;
  command: string;
  cwd: string;
  level: number;
  status: AuditStatus;
  exitCode: number | null;
  message: string;
  durationMs: number;
  source: RunSource | string;
}

export interface AuditFilter {
  keyword?: string;
  level?: number;
  source?: string;
  limit?: number;
}

export interface TerminalHistory {
  id: number;
  sessionId: string;
  presetId: string;
  presetName: string;
  title: string;
  command: string;
  cwd: string;
  kind: string;
  startedAt: number;
  endedAt: number;
  exitCode: number | null;
  outputTail: string;
}

// ============================================================
// 批量
// ============================================================

export interface BatchRequest {
  presetIds: string[];
  argsMap: Record<string, string>;
  concurrency: number;
  continueOnError: boolean;
}

export interface BatchResult {
  runId: string;
  sessions: TerminalInfo[];
  blocked: string[];
  startedAt: number;
}

// ============================================================
// 定时任务
// ============================================================

export type ScheduleMode = "interval" | "daily" | "weekly" | "once";

export interface Schedule {
  id: string;
  name: string;
  presetId: string;
  args: Record<string, string>;
  mode: ScheduleMode;
  intervalMinutes: number;
  /** "HH:mm" */
  time: string;
  /** 1=周一 … 7=周日 */
  weekdays: number[];
  /** "YYYY-MM-DD" */
  date: string;
  enabled: boolean;
  nextRunAt: number;
  lastRunAt: number;
  lastStatus: string;
  createdAt: number;
  updatedAt: number;
}

export function createEmptySchedule(presetId = ""): Schedule {
  const now = Date.now();
  return {
    id: "",
    name: "新定时任务",
    presetId,
    args: {},
    mode: "interval",
    intervalMinutes: 60,
    time: "09:00",
    weekdays: [],
    date: "",
    enabled: true,
    nextRunAt: 0,
    lastRunAt: 0,
    lastStatus: "",
    createdAt: now,
    updatedAt: now,
  };
}

/** 定时任务触发时广播给前端的事件负载 */
export interface ScheduleFiredEvent {
  scheduleId: string;
  name: string;
  presetId: string;
  presetName: string;
  source: RunSource;
  sessionId: string;
}

// ============================================================
// 工作流
// ============================================================

export type WorkflowRunMode = "serial" | "parallel";

export interface WorkflowStep {
  id: string;
  name: string;
  presetId: string;
  args: Record<string, string>;
  enabled: boolean;
  delayMs: number;
  condition: "always" | "onSuccess" | "onFail";
}

export interface Workflow {
  id: string;
  name: string;
  description: string;
  runMode: WorkflowRunMode;
  continueOnError: boolean;
  steps: WorkflowStep[];
  enabled: boolean;
  createdAt: number;
  updatedAt: number;
}

export function createEmptyWorkflow(): Workflow {
  const now = Date.now();
  return {
    id: "",
    name: "新工作流",
    description: "",
    runMode: "serial",
    continueOnError: true,
    steps: [],
    enabled: true,
    createdAt: now,
    updatedAt: now,
  };
}

export type StepStatus =
  "pending" | "running" | "success" | "failed" | "blocked" | "skipped";

export interface WorkflowStepResult {
  stepId: string;
  stepName: string;
  presetName: string;
  status: StepStatus;
  sessionId: string;
  exitCode: number | null;
  message: string;
  startedAt: number;
  finishedAt: number;
}

export interface WorkflowRun {
  id: string;
  workflowId: string;
  workflowName: string;
  startedAt: number;
  finishedAt: number;
  status: "running" | "success" | "failed" | "canceled";
  steps: WorkflowStepResult[];
  source: string;
}

// ============================================================
// 设置
// ============================================================

export interface AppSettings {
  // 外观
  theme: "dark" | "light" | "system";
  locale: string;
  fontFamily: string;
  fontSize: number;
  colorScheme: string;
  cursorBlink: boolean;
  scrollback: number;
  uiScale: number;
  // 终端行为
  defaultShell: string;
  shellPath: string;
  defaultWorkingDir: string;
  showWelcomeBanner: boolean;
  encoding: "auto" | "utf-8" | "gbk";
  maxConcurrentSessions: number;
  autoCloseOnExit: boolean;
  // 行为
  confirmDangerous: boolean;
  allowUnknownExe: boolean;
  auditEnabled: boolean;
  historyEnabled: boolean;
  historyLimit: number;
  // 系统集成
  globalHotkey: string;
  autostart: boolean;
  minimizeToTray: boolean;
  singleInstance: boolean;
  restoreWindowOnStart: boolean;
  // 安全
  blacklist: string[];
  // 首次运行
  firstRunDone: boolean;
  dataDir: string;
}

export function defaultSettings(): AppSettings {
  return {
    theme: "dark",
    locale: "zh-CN",
    fontFamily: 'Cascadia Mono, Consolas, "Microsoft YaHei Mono", monospace',
    fontSize: 14,
    colorScheme: "cmddeck-dark",
    cursorBlink: true,
    scrollback: 5000,
    uiScale: 100,

    defaultShell: "powershell",
    shellPath: "",
    defaultWorkingDir: "",
    showWelcomeBanner: true,
    encoding: "auto",
    maxConcurrentSessions: 12,
    autoCloseOnExit: false,

    confirmDangerous: true,
    allowUnknownExe: false,
    auditEnabled: true,
    historyEnabled: true,
    historyLimit: 500,

    globalHotkey: "CommandOrControl+Shift+Space",
    autostart: false,
    minimizeToTray: true,
    singleInstance: true,
    restoreWindowOnStart: true,

    blacklist: [
      "format c:",
      "rd /s /q c:\\",
      "Remove-Item -Recurse -Force c:\\",
      "del /f /q c:\\",
      "diskpart",
    ],

    firstRunDone: false,
    dataDir: "",
  };
}

// ============================================================
// 系统信息 / 导入导出
// ============================================================

export interface AppInfo {
  version: string;
  name: string;
  tauriVersion: string;
  os: string;
  arch: string;
  dataDir: string;
  dbPath: string;
  firstRun: boolean;
  userName: string;
}

export type ImportMode = "replace" | "merge" | "append";

export interface ImportReport {
  groupsAdded: number;
  groupsUpdated: number;
  presetsAdded: number;
  presetsUpdated: number;
  workflowsAdded: number;
  schedulesAdded: number;
  warnings: string[];
  settingsImported: boolean;
}

export interface ExportBundle {
  version: string;
  exportedAt: number;
  appVersion: string;
  settings: AppSettings;
  groups: PresetGroup[];
  presets: Preset[];
  workflows: Workflow[];
  schedules: Schedule[];
}

// ============================================================
// 前端内部类型
// ============================================================

/** 终端标签页在前端的运行时信息（Rust 侧数据 + UI 状态） */
export interface TerminalTab extends TerminalInfo {
  /** 未读取的历史缓冲，渲染完成后置空 */
  pendingSnapshot: string | null;
  /** 最近一次输出，用于"复制全部" */
  outputBuffer: string;
  /** 缓冲头部已丢弃的字符数，用绝对偏移继续渲染新输出。 */
  outputOffset: number;
  /** 是否已被用户手动终止 */
  stoppedByUser: boolean;
  /** 是否在后台运行（标签页不可见但仍在跑） */
  background: boolean;
}

/** 右下角提示 */
export interface Toast {
  id: string;
  type: "info" | "success" | "warning" | "error";
  title: string;
  message: string;
  /** 毫秒，0 表示不自动关闭 */
  duration: number;
}

/** 应用视图 */
export type ViewName =
  | "projects"
  | "tasks"
  | "logs"
  | "backups"
  | "updates"
  | "environments"
  | "presets"
  | "terminals"
  | "workflows"
  | "schedules"
  | "audit"
  | "settings";

/** 弹窗类型 */
export type DialogName =
  | "none"
  | "presetEditor"
  | "runParams"
  | "confirm"
  | "settings"
  | "batchRun"
  | "quickLaunch"
  | "audit"
  | "workflowEditor"
  | "scheduleEditor"
  | "history"
  | "about";
import type { RuntimeBinding } from "./environments";
export type {
  RuntimeBinding,
  EnvironmentInfo,
  EnvironmentReport,
  EnvironmentAction,
  ToolInfo,
} from "./environments";
