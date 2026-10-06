/**
 * 后端事件监听封装。
 *
 * 事件名与 `src-tauri/src/state.rs` 中的常量一一对应。
 * 统一在此处注册，组件通过 `onPtyData` 等函数订阅，返回值是取消订阅函数。
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  AppSettings,
  ScheduleFiredEvent,
  TerminalInfo,
  WorkflowRun,
} from "@/types";

/** 终端输出事件 */
export const EV_PTY_DATA = "cmddeck://pty-data";
export const EV_PTY_EXIT = "cmddeck://pty-exit";
export const EV_PTY_OPEN = "cmddeck://pty-open";
export const EV_SCHEDULE_FIRED = "cmddeck://schedule-fired";
export const EV_WORKFLOW_UPDATE = "cmddeck://workflow-update";
export const EV_QUICK_LAUNCH = "cmddeck://quick-launch";
export const EV_NAVIGATE = "cmddeck://navigate";
export const EV_SETTINGS_CHANGED = "cmddeck://settings-changed";
export const EV_PRESET_CHANGED = "cmddeck://preset-changed";

// ============================================================
// 负载类型
// ============================================================

export interface PtyDataPayload {
  sessionId: string;
  data: string;
}

export interface PtyExitPayload {
  sessionId: string;
  exitCode: number | null;
  /** exited / killed */
  status: "exited" | "killed";
}

export interface PtyOpenPayload extends TerminalInfo {}

export interface PresetChangedPayload {
  reason: string;
}

// ============================================================
// 订阅函数
// ============================================================

/** 终端输出 */
export function onPtyData(
  cb: (p: PtyDataPayload) => void,
): Promise<UnlistenFn> {
  return listen<PtyDataPayload>(EV_PTY_DATA, (e) => cb(e.payload));
}

/** 终端退出 */
export function onPtyExit(
  cb: (p: PtyExitPayload) => void,
): Promise<UnlistenFn> {
  return listen<PtyExitPayload>(EV_PTY_EXIT, (e) => cb(e.payload));
}

/** 新终端会话创建 */
export function onPtyOpen(
  cb: (p: PtyOpenPayload) => void,
): Promise<UnlistenFn> {
  return listen<PtyOpenPayload>(EV_PTY_OPEN, (e) => cb(e.payload));
}

/** 定时任务触发 */
export function onScheduleFired(
  cb: (p: ScheduleFiredEvent) => void,
): Promise<UnlistenFn> {
  return listen<ScheduleFiredEvent>(EV_SCHEDULE_FIRED, (e) => cb(e.payload));
}

/** 工作流状态更新 */
export function onWorkflowUpdate(
  cb: (p: WorkflowRun) => void,
): Promise<UnlistenFn> {
  return listen<WorkflowRun>(EV_WORKFLOW_UPDATE, (e) => cb(e.payload));
}

/** 快速启动面板请求（托盘点击 / 全局快捷键） */
export function onQuickLaunch(cb: () => void): Promise<UnlistenFn> {
  return listen(EV_QUICK_LAUNCH, () => cb());
}

/** 请求切换视图 */
export function onNavigate(cb: (view: string) => void): Promise<UnlistenFn> {
  return listen<string>(EV_NAVIGATE, (e) => cb(e.payload));
}

/** 设置已变更 */
export function onSettingsChanged(
  cb: (s: AppSettings) => void,
): Promise<UnlistenFn> {
  return listen<AppSettings>(EV_SETTINGS_CHANGED, (e) => cb(e.payload));
}

/** 预设数据已变更 */
export function onPresetChanged(
  cb: (p: PresetChangedPayload) => void,
): Promise<UnlistenFn> {
  return listen<PresetChangedPayload>(EV_PRESET_CHANGED, (e) => cb(e.payload));
}
