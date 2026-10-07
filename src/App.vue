<script setup lang="ts">
// 界面通过 Tauri IPC 连接 Rust；浏览器预览不执行本地命令。
import {
  computed,
  onMounted,
  onBeforeUnmount,
  reactive,
  ref,
  watch,
} from "vue";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  auditApi,
  batchApi,
  presetApi,
  scheduleApi,
  securityApi,
  systemApi,
  terminalApi,
  workflowApi,
  toFriendlyError,
} from "@/api";
import { onQuickLaunch, onPresetChanged, onWorkflowUpdate } from "@/api/events";
import { usePresetStore, ALL_KEY } from "@/stores/presets";
import { useSettingsStore } from "@/stores/settings";
import { useTerminalStore } from "@/stores/terminals";
import { useUiStore } from "@/stores/ui";
import { useTheme } from "@/composables/useTheme";
import TerminalWorkspace from "@/components/terminal/TerminalWorkspace.vue";
import EnvironmentManager from "@/components/EnvironmentManager.vue";
import { environmentApi } from "@/api/environments";
import type {
  AuditLog,
  Preset,
  Schedule,
  TerminalHistory,
  Workflow,
  WorkflowRun,
  EnvironmentInfo,
  EnvironmentReport,
} from "@/types";
import "./styles/deck.css";
const ps = reactive(usePresetStore()),
  ts = reactive(useTerminalStore()),
  ss = reactive(useSettingsStore()),
  ui = reactive(useUiStore());
useTheme();
const editor = ref<Preset | null>(null),
  argsText = ref(""),
  envText = ref(""),
  tagsText = ref("");
const runPreset = ref<Preset | null>(null),
  values = reactive<Record<string, string>>({}),
  runKeys = ref<string[]>([]),
  preview = ref(""),
  verdictText = ref(""),
  busy = ref(false);
const quick = ref(false),
  quickQuery = ref(""),
  welcome = ref(false),
  failure = ref(""),
  ready = ref(false);
const selected = ref<string[]>([]),
  dragged = ref(""),
  batchBusy = ref(false),
  batchArgs = ref("{}"),
  concurrency = ref(4);
const workflows = ref<Workflow[]>([]),
  runs = ref<WorkflowRun[]>([]),
  workflowEditor = ref<Workflow | null>(null);
const schedules = ref<Schedule[]>([]),
  scheduleEditor = ref<Schedule | null>(null),
  scheduleArgs = ref("{}");
const logs = ref<AuditLog[]>([]),
  history = ref<TerminalHistory[]>([]),
  blacklistText = ref("");
const pages = [
  { id: "presets", label: "预设指令", icon: "⌘" },
  { id: "environments", label: "运行环境", icon: "◇" },
  { id: "workflows", label: "工作流", icon: "⛓" },
  { id: "schedules", label: "定时任务", icon: "◷" },
  { id: "audit", label: "运行记录", icon: "≡" },
  { id: "settings", label: "设置", icon: "⚙" },
] as const;
const environmentReport = ref<EnvironmentReport | null>(null),
  environmentLoading = ref(false);
const integrations = ref<{
  autostart: boolean;
  tray: boolean;
  scheduler: boolean;
} | null>(null);
async function refreshIntegrations() {
  await attempt(async () => {
    integrations.value = await systemApi.integrationStatus();
  });
}
async function toggleScheduler() {
  await attempt(async () => {
    if (integrations.value?.scheduler) await scheduleApi.stop();
    else await scheduleApi.start();
    await refreshIntegrations();
  });
}
async function refreshEnvironments() {
  if (environmentLoading.value) return;
  environmentLoading.value = true;
  await attempt(async () => {
    environmentReport.value = await environmentApi.discover(
      editor.value?.workingDir || ss.settings.defaultWorkingDir,
    );
  });
  environmentLoading.value = false;
}
function bindEnvironment(id: string) {
  if (!editor.value) return;
  const e = environmentReport.value?.environments.find((e) => e.id === id);
  editor.value.runtime = e
    ? { kind: e.kind, path: e.path, managerPath: e.managerPath }
    : { kind: "", path: "", managerPath: "" };
}
function presetFromEnvironment(e: EnvironmentInfo) {
  edit();
  if (editor.value) {
    editor.value.name = `${e.name} 指令`;
    editor.value.runtime = {
      kind: e.kind,
      path: e.path,
      managerPath: e.managerPath,
    };
    editor.value.kind = e.kind === "node" ? "node" : "python";
    editor.value.useShell = false;
  }
}
const quickResults = computed(() =>
  ps.presets
    .filter(
      (p) =>
        !p.hidden &&
        `${p.name} ${p.program} ${p.tags}`
          .toLowerCase()
          .includes(quickQuery.value.toLowerCase()),
    )
    .slice(0, 30),
);
const cleanups: (() => void)[] = [];
async function attempt(fn: () => Promise<unknown>) {
  try {
    return await fn();
  } catch (e) {
    ui.toast("error", "操作失败", toFriendlyError(e).message, 8000);
    return undefined;
  }
}
function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}
function edit(p?: Preset) {
  editor.value = p
    ? clone(p)
    : ps.createPreset(ps.activeGroupId === ALL_KEY ? "" : ps.activeGroupId);
  argsText.value = editor.value.args.join("\n");
  envText.value = editor.value.env
    .map((e) => `${e.name}=${e.value}`)
    .join("\n");
  tagsText.value = editor.value.tags.join(", ");
}
async function savePreset() {
  await attempt(async () => {
    if (!editor.value) return;
    editor.value.args = argsText.value.split("\n").filter(Boolean);
    editor.value.env = envText.value
      .split("\n")
      .filter(Boolean)
      .map((line) => {
        const i = line.indexOf("=");
        if (i < 1) throw new Error("环境变量每行使用 KEY=VALUE 格式");
        return { name: line.slice(0, i), value: line.slice(i + 1) };
      });
    editor.value.tags = tagsText.value
      .split(/[,，]/)
      .map((s) => s.trim())
      .filter(Boolean);
    editor.value.placeholderArgs = await presetApi.scanPlaceholders(
      [
        editor.value.program,
        argsText.value,
        editor.value.workingDir,
        envText.value,
      ].join(" "),
    );
    await ps.save(editor.value);
    editor.value = null;
  });
}
async function remove(p: Preset) {
  if (
    await ui.confirm({
      title: "删除预设",
      message: `确定删除「${p.name}」？`,
      danger: true,
    })
  )
    await attempt(() => ps.remove(p.id));
}
async function addGroup() {
  const name = window.prompt("新分组名称");
  if (name?.trim())
    await attempt(() =>
      ps.saveGroup({ ...ps.createGroup(), name: name.trim() }),
    );
}
async function deleteGroup() {
  if (!ps.activeGroupId || ps.activeGroupId === ALL_KEY) return;
  if (
    await ui.confirm({
      title: "删除分组",
      message: "预设会移到未分组，是否继续？",
    })
  )
    await attempt(() => ps.removeGroup(ps.activeGroupId));
}
async function dropOn(id: string) {
  if (!dragged.value || dragged.value === id) return;
  const ids = ps.visiblePresets
    .map((p) => p.id)
    .filter((i) => i !== dragged.value);
  ids.splice(ids.indexOf(id), 0, dragged.value);
  await attempt(() => ps.reorder(ids));
  dragged.value = "";
}
async function requestRun(p: Preset) {
  quick.value = false;
  runPreset.value = clone(p);
  Object.keys(values).forEach((k) => delete values[k]);
  const keys = await presetApi.scanPlaceholders(
    [p.program, ...p.args, p.workingDir, ...p.env.map((e) => e.value)].join(
      " ",
    ),
  );
  runKeys.value = keys.map((k) => k.key);
  for (const key of runKeys.value)
    values[key] =
      p.placeholderArgs.find((a) => a.key === key)?.defaultValue || "";
  preview.value = "";
  verdictText.value = "";
  await updatePreview();
}
async function updatePreview() {
  if (!runPreset.value) return;
  await attempt(async () => {
    preview.value = await presetApi.preview(runPreset.value!, values);
    const v = await securityApi.check(runPreset.value!, values);
    verdictText.value = v.reasons.join("；");
  });
}
async function execute() {
  if (!runPreset.value || busy.value) return;
  busy.value = true;
  await attempt(async () => {
    const p = runPreset.value!,
      v = await securityApi.check(p, values);
    if (v.level === "blocked") throw new Error(v.reasons.join("；"));
    let confirmed = false;
    if (v.requiresConfirm || p.confirm) {
      confirmed = await ui.confirm({
        title: "确认执行命令",
        message: v.reasons.join("；") || "此预设要求二次确认",
        detail: preview.value,
        danger: true,
      });
      if (!confirmed) return;
    }
    const info = await terminalApi.runPreset(
      p.id,
      { ...values },
      "manual",
      confirmed,
    );
    await ts.attach(info, await terminalApi.snapshot(info.sessionId));
    await ps.load();
    runPreset.value = null;
  });
  busy.value = false;
}
async function batch() {
  if (batchBusy.value) return;
  batchBusy.value = true;
  await attempt(async () => {
    const r = await batchApi.run(
      selected.value,
      JSON.parse(batchArgs.value),
      concurrency.value,
      true,
    );
    if (r.blocked.length)
      ui.toast("warning", "部分任务失败或被拦截", r.blocked.join("\n"), 10000);
    await ps.load();
  });
  batchBusy.value = false;
}
async function refreshPanels() {
  await attempt(async () => {
    // 只查询当前页面，避免定时页重复传输终端历史与审计输出。
    if (ui.view === "workflows")
      [workflows.value, runs.value] = await Promise.all([
        workflowApi.list(),
        workflowApi.listRuns("", 50),
      ]);
    else if (ui.view === "schedules") {
      schedules.value = await scheduleApi.list();
      await refreshIntegrations();
    } else if (ui.view === "audit")
      [logs.value, history.value] = await Promise.all([
        auditApi.list(),
        auditApi.history(),
      ]);
  });
}
function newWorkflow(w?: Workflow) {
  workflowEditor.value = w
    ? clone(w)
    : {
        id: "",
        name: "新工作流",
        description: "",
        runMode: "serial",
        continueOnError: false,
        steps: [],
        enabled: true,
        createdAt: 0,
        updatedAt: 0,
      };
}
function addStep() {
  workflowEditor.value?.steps.push({
    id: crypto.randomUUID(),
    name: "",
    presetId: ps.presets[0]?.id || "",
    args: {},
    enabled: true,
    delayMs: 0,
    condition: "always",
  });
}
async function saveWorkflow() {
  await attempt(async () => {
    if (workflowEditor.value) await workflowApi.save(workflowEditor.value);
    workflowEditor.value = null;
    await refreshPanels();
  });
}
function newSchedule(s?: Schedule) {
  scheduleEditor.value = s
    ? clone(s)
    : {
        id: "",
        name: "新定时任务",
        presetId: ps.presets[0]?.id || "",
        args: {},
        mode: "interval",
        intervalMinutes: 60,
        time: "09:00",
        weekdays: [1, 2, 3, 4, 5],
        date: "",
        enabled: true,
        nextRunAt: 0,
        lastRunAt: 0,
        lastStatus: "",
        createdAt: 0,
        updatedAt: 0,
      };
  scheduleArgs.value = JSON.stringify(scheduleEditor.value!.args, null, 2);
}
async function saveSchedule() {
  await attempt(async () => {
    if (scheduleEditor.value) {
      scheduleEditor.value.args = JSON.parse(scheduleArgs.value);
      await scheduleApi.save(scheduleEditor.value);
    }
    scheduleEditor.value = null;
    await refreshPanels();
  });
}
async function saveSettings() {
  await attempt(async () => {
    ss.settings.blacklist = blacklistText.value
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean);
    await ss.save({ ...ss.settings });
    await refreshIntegrations();
    ui.toast("success", "设置已保存");
  });
}
async function pickWorkingDirectory(forPreset: boolean) {
  await attempt(async () => {
    const path = await open({
      directory: true,
      multiple: false,
      title: "选择命令运行的项目目录",
    });
    if (typeof path === "string") {
      if (forPreset && editor.value) editor.value.workingDir = path;
      else ss.settings.defaultWorkingDir = path;
    }
  });
}
async function exportConfig() {
  await attempt(async () => {
    const path = await save({
      defaultPath: "CmdDeck-config.json",
      filters: [{ name: "JSON 配置", extensions: ["json"] }],
    });
    if (path) {
      const n = await systemApi.exportData(path);
      ui.toast("success", `已导出 ${n} 条预设`, path);
    }
  });
}
async function importConfig() {
  await attempt(async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "JSON 配置", extensions: ["json"] }],
    });
    if (typeof path === "string") {
      const r = await systemApi.importData(path, "merge");
      await ps.load();
      await refreshPanels();
      ui.toast(
        "success",
        "配置已合并",
        `新增 ${r.presetsAdded} 条，更新 ${r.presetsUpdated} 条。${r.warnings.join("；")}`,
      );
    }
  });
}
function newWindow() {
  const win = new WebviewWindow(`deck-${crypto.randomUUID()}`, {
    url: "index.html",
    title: "CmdDeck · 工作窗口",
    width: 1280,
    height: 800,
  });
  void win.once("tauri://error", (e) =>
    ui.toast("error", "新窗口失败", String(e.payload)),
  );
}
async function finishWelcome() {
  await attempt(async () => {
    await ss.save({ firstRunDone: true });
    welcome.value = false;
  });
}
async function deleteWorkflow(id: string) {
  if (
    await ui.confirm({ title: "删除工作流", message: "确认删除这个工作流？" })
  )
    await attempt(async () => {
      await workflowApi.remove(id);
      await refreshPanels();
    });
}
async function runWorkflow(id: string) {
  await attempt(() => workflowApi.start(id));
}
async function cancelWorkflow(id: string) {
  await attempt(() => workflowApi.cancel(id));
}
async function toggleSchedule(s: Schedule) {
  await attempt(async () => {
    await scheduleApi.setEnabled(s.id, !s.enabled);
    await refreshPanels();
  });
}
async function deleteSchedule(id: string) {
  if (
    await ui.confirm({
      title: "删除定时任务",
      message: "确认删除这个定时任务？",
    })
  )
    await attempt(async () => {
      await scheduleApi.remove(id);
      await refreshPanels();
    });
}
async function triggerSchedule(id: string) {
  await attempt(() => scheduleApi.triggerNow(id));
}
async function openDataDir() {
  await attempt(() => systemApi.openDataDir());
}
function date(n: number) {
  return n ? new Date(n).toLocaleString("zh-CN") : "—";
}
function keydown(e: KeyboardEvent) {
  if (e.ctrlKey && e.key.toLowerCase() === "k") {
    e.preventDefault();
    quick.value = !quick.value;
  }
  if (e.key === "Escape") {
    quick.value = false;
    editor.value = null;
    runPreset.value = null;
    workflowEditor.value = null;
    scheduleEditor.value = null;
  }
}
watch(
  () => ui.dialog,
  () => {
    if (ui.dialog === "runParams" && ui.dialogPayload) {
      void requestRun(ui.dialogPayload as Preset);
      ui.closeDialog();
    }
  },
);
watch(
  () => ui.quickLaunchOpen,
  (v) => {
    if (v) {
      quick.value = true;
      ui.quickLaunchOpen = false;
    }
  },
);
watch(
  () => ui.view,
  () => {
    if (["workflows", "schedules", "audit"].includes(ui.view))
      void refreshPanels();
    if (ui.view === "settings") void refreshIntegrations();
    window.setTimeout(() => ts.fitAll(), 80);
  },
);
onMounted(async () => {
  try {
    await ss.load();
    blacklistText.value = ss.settings.blacklist.join("\n");
    await ps.load();
    ps.activeGroupId = ALL_KEY;
    cleanups.push(await ts.init());
    cleanups.push(await onQuickLaunch(() => (quick.value = true)));
    cleanups.push(await onPresetChanged(() => void ps.load()));
    cleanups.push(
      await onWorkflowUpdate((r) => {
        runs.value = [r, ...runs.value.filter((v) => v.id !== r.id)];
      }),
    );
    for (const info of await terminalApi.list())
      await ts.attach(info, await terminalApi.snapshot(info.sessionId));
    welcome.value = !ss.settings.firstRunDone;
    ready.value = true;
  } catch (e) {
    failure.value = toFriendlyError(e).message;
  }
  window.addEventListener("keydown", keydown);
  const timer = window.setInterval(() => {
    if (ui.view === "schedules" || ui.view === "audit") void refreshPanels();
  }, 5000);
  cleanups.push(() => clearInterval(timer));
});
onBeforeUnmount(() => {
  cleanups.forEach((fn) => fn());
  window.removeEventListener("keydown", keydown);
});
</script>
<template>
  <div class="deck">
    <header class="topbar">
      <div class="brand">
        <span class="brand-mark">&gt;_</span>CmdDeck <small>控制台中心</small>
      </div>
      <input
        v-model="ps.keyword"
        aria-label="搜索预设"
        class="search"
        placeholder="搜索指令、标签、备注…"
      /><button @click="quick = true">快速启动 <kbd>Ctrl K</kbd></button
      ><button @click="newWindow">新窗口</button
      ><button @click="ui.toggleTheme()">
        {{ ui.theme === "dark" ? "☀" : "☾" }}
      </button>
    </header>
    <main class="deck-main">
      <nav class="rail">
        <button
          v-for="p in pages"
          :key="p.id"
          :class="{ active: ui.view === p.id }"
          @click="ui.setView(p.id)"
        >
          <span>{{ p.icon }}</span
          >{{ p.label }}
        </button>
        <div class="rail-bottom"><span class="live-dot" />本地运行</div>
      </nav>
      <template v-if="ui.view === 'presets' || ui.view === 'terminals'">
        <aside class="library">
          <div class="panel-heading">
            <h2>
              指令库 <small>{{ ps.presets.length }}</small>
            </h2>
            <button class="primary" @click="edit()">＋ 新建</button>
          </div>
          <div class="filters">
            <button
              :class="{ active: !ps.onlyFavorite && !ps.onlyRecent }"
              @click="
                ps.onlyFavorite = false;
                ps.onlyRecent = false;
              "
            >
              全部</button
            ><button
              :class="{ active: ps.onlyFavorite }"
              @click="
                ps.onlyFavorite = !ps.onlyFavorite;
                ps.onlyRecent = false;
              "
            >
              ★ 收藏</button
            ><button
              :class="{ active: ps.onlyRecent }"
              @click="
                ps.onlyRecent = !ps.onlyRecent;
                ps.onlyFavorite = false;
              "
            >
              最近
            </button>
          </div>
          <div class="group-filter">
            <select v-model="ps.activeGroupId" aria-label="选择分组">
              <option :value="ALL_KEY">全部分组</option>
              <option value="">未分组</option>
              <option v-for="g in ps.groups" :key="g.id" :value="g.id">
                {{ g.name }}
              </option></select
            ><button title="新建分组" @click="addGroup">＋</button
            ><button title="删除分组" @click="deleteGroup">−</button>
          </div>
          <div class="preset-list">
            <article
              v-for="p in ps.visiblePresets"
              :key="p.id"
              class="preset-card"
              draggable="true"
              @dragstart="dragged = p.id"
              @dragover.prevent
              @drop.prevent="dropOn(p.id)"
              @click="ps.selectedId = p.id"
              @dblclick="requestRun(p)"
            >
              <div class="card-title">
                <input
                  v-model="selected"
                  type="checkbox"
                  :value="p.id"
                  :aria-label="`选择 ${p.name}`"
                /><span class="kind-icon">{{
                  p.kind === "powershell"
                    ? "PS"
                    : p.kind === "python"
                      ? "Py"
                      : p.kind === "node"
                        ? "JS"
                        : "›_"
                }}</span
                ><strong>{{ p.name }}</strong
                ><button
                  class="icon-button"
                  :class="{ gold: p.favorite }"
                  :aria-label="`收藏 ${p.name}`"
                  @click="attempt(() => ps.toggleFavorite(p.id))"
                >
                  {{ p.favorite ? "★" : "☆" }}
                </button>
              </div>
              <code class="command-preview"
                >{{ p.program || p.kind }} {{ p.args.join(" ") }}</code
              >
              <div class="card-footer">
                <span
                  >{{ p.kind }}
                  <small v-for="tag in p.tags" :key="tag"
                    >#{{ tag }}</small
                  ></span
                >
                <div>
                  <button @click="edit(p)">编辑</button
                  ><button title="删除" @click="remove(p)">×</button
                  ><button class="run-button" @click="requestRun(p)">▶</button>
                </div>
              </div>
            </article>
            <div v-if="!ps.visiblePresets.length" class="empty">
              没有匹配的指令<br />新建预设，或调整搜索条件
            </div>
          </div>
          <div class="batch-bar">
            <span>已选 {{ selected.length }} 条</span
            ><input
              v-model.number="concurrency"
              type="number"
              min="1"
              max="12"
              title="并发数量"
            /><button
              :disabled="!selected.length || batchBusy"
              @click="ui.openDialog('batchRun')"
            >
              {{ batchBusy ? "批量运行中…" : "批量执行" }}
            </button>
          </div>
        </aside>
        <section class="terminal-panel">
          <div class="terminal-heading">
            <div>
              <span class="live-dot" />终端工作区
              <small>{{ ts.runningCount }} 个运行中</small>
            </div>
            <div>
              <button @click="attempt(() => ts.openShell('cmd'))">＋ CMD</button
              ><button
                class="primary"
                @click="attempt(() => ts.openShell(ss.settings.defaultShell))"
              >
                ＋ PowerShell
              </button>
            </div>
          </div>
          <TerminalWorkspace />
        </section>
      </template>
      <section v-else class="page">
        <EnvironmentManager
          v-if="ui.view === 'environments'"
          :default-project="ss.settings.defaultWorkingDir"
          @updated="environmentReport = $event"
          @preset="presetFromEnvironment"
        />
        <template v-if="ui.view === 'workflows'"
          ><div class="page-heading">
            <div>
              <h1>工作流</h1>
              <p>串联常用指令，根据上一步的结果继续执行。</p>
            </div>
            <button class="primary" @click="newWorkflow()">
              ＋ 新建工作流
            </button>
          </div>
          <article v-for="w in workflows" :key="w.id" class="resource-card">
            <div>
              <h3>{{ w.name }}</h3>
              <p>
                {{ w.description }} · {{ w.steps.length }} 步 ·
                {{ w.runMode === "serial" ? "串行" : "并行" }}
              </p>
            </div>
            <button @click="newWorkflow(w)">编辑</button
            ><button @click="deleteWorkflow(w.id)">删除</button
            ><button class="primary" @click="runWorkflow(w.id)">运行</button>
          </article>
          <h2>执行记录</h2>
          <article v-for="r in runs" :key="r.id" class="run-record">
            <div>
              <strong>{{ r.workflowName }}</strong
              ><span class="badge">{{ r.status }}</span
              ><button
                v-if="r.status === 'running'"
                @click="cancelWorkflow(r.id)"
              >
                取消</button
              ><small>{{ date(r.startedAt) }}</small>
            </div>
            <p v-for="s in r.steps" :key="s.stepId">
              {{ s.stepName || s.presetName || s.stepId }} · {{ s.status }} ·
              退出码 {{ s.exitCode ?? "—" }} {{ s.message }}
            </p>
          </article></template
        >
        <template v-if="ui.view === 'schedules'"
          ><div class="page-heading">
            <div>
              <h1>定时任务</h1>
              <p>程序运行时执行。每日、每周和单次任务使用北京时间 UTC+8。</p>
              <p>
                调度器：{{ integrations?.scheduler ? "正在运行" : "已暂停" }} ·
                暂停只阻止新任务，已启动进程继续运行。
              </p>
            </div>
            <button @click="toggleScheduler">
              {{ integrations?.scheduler ? "暂停调度" : "启动调度" }}
            </button>
            <button class="primary" @click="newSchedule()">＋ 新建任务</button>
          </div>
          <article v-for="s in schedules" :key="s.id" class="resource-card">
            <div>
              <h3>{{ s.name }}</h3>
              <p>
                下次：{{ date(s.nextRunAt) }} · {{ s.lastStatus || "尚未运行" }}
              </p>
            </div>
            <button @click="toggleSchedule(s)">
              {{ s.enabled ? "停用" : "启用" }}</button
            ><button @click="newSchedule(s)">编辑</button
            ><button @click="deleteSchedule(s.id)">删除</button
            ><button @click="triggerSchedule(s.id)">立即运行</button>
          </article>
          <div v-if="!schedules.length" class="empty">
            创建第一个定时任务
          </div></template
        >
        <template v-if="ui.view === 'audit'"
          ><div class="page-heading">
            <div>
              <h1>运行记录</h1>
              <p>本机审计日志与终端历史</p>
            </div>
            <button @click="refreshPanels">刷新</button>
          </div>
          <h2>审计日志</h2>
          <table>
            <thead>
              <tr>
                <th>时间</th>
                <th>预设 / 来源</th>
                <th>命令</th>
                <th>结果</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="l in logs" :key="l.id">
                <td>{{ date(l.at) }}</td>
                <td>{{ l.presetName || "终端" }} / {{ l.source }}</td>
                <td>
                  <code>{{ l.command }}</code>
                  <p>{{ l.message }}</p>
                </td>
                <td>{{ l.status }} / {{ l.exitCode ?? "—" }}</td>
              </tr>
            </tbody>
          </table>
          <h2>终端历史</h2>
          <details v-for="h in history" :key="h.id">
            <summary>
              {{ h.title }} · {{ date(h.startedAt) }} · 退出码 {{ h.exitCode }}
            </summary>
            <pre>{{ h.outputTail }}</pre>
          </details></template
        >
        <template v-if="ui.view === 'settings'"
          ><div class="page-heading">
            <div>
              <h1>设置</h1>
              <p>本地配置 · WebView2 + Rust + SQLite</p>
            </div>
            <button class="primary" @click="saveSettings">保存设置</button>
          </div>
          <div class="settings-grid">
            <p v-if="integrations" class="wide">
              系统状态：托盘{{ integrations.tray ? "已创建" : "未创建" }} ·
              开机自启{{ integrations.autostart ? "已注册" : "未注册" }} ·
              调度器{{ integrations.scheduler ? "运行中" : "已暂停" }}
            </p>
            <label
              >主题<select v-model="ss.settings.theme">
                <option value="dark">暗色</option>
                <option value="light">亮色</option>
                <option value="system">跟随系统</option>
              </select></label
            ><label
              >默认终端<select v-model="ss.settings.defaultShell">
                <option value="powershell">PowerShell</option>
                <option value="cmd">CMD</option>
                <option value="pwsh">PowerShell 7</option>
              </select></label
            ><label>终端字体<input v-model="ss.settings.fontFamily" /></label
            ><label
              >终端字号<input
                v-model.number="ss.settings.fontSize"
                type="number"
                min="8"
                max="48" /></label
            ><label
              >默认工作目录<input
                v-model="ss.settings.defaultWorkingDir"
                placeholder="留空时使用用户主目录"
              />
              <button @click="pickWorkingDirectory(false)">选择文件夹</button>
              <small
                >预设目录优先；未指定时使用此目录，留空则打开用户主目录。</small
              ></label
            ><label
              >全局快捷键<input v-model="ss.settings.globalHotkey" /></label
            ><label
              >并发终端上限<input
                v-model.number="ss.settings.maxConcurrentSessions"
                type="number"
                min="1"
                max="64" /></label
            ><label
              >输出编码<select v-model="ss.settings.encoding">
                <option value="auto">自动（默认 UTF-8）</option>
                <option value="utf-8">UTF-8</option>
                <option value="gbk">GBK（旧程序）</option>
              </select></label
            ><label class="check"
              ><input
                v-model="ss.settings.autostart"
                type="checkbox"
              />开机启动</label
            ><label class="check"
              ><input
                v-model="ss.settings.minimizeToTray"
                type="checkbox"
              />关闭主窗口时隐藏到托盘</label
            ><label class="check"
              ><input
                v-model="ss.settings.confirmDangerous"
                type="checkbox"
              />危险命令二次确认</label
            ><label class="check"
              ><input
                v-model="ss.settings.allowUnknownExe"
                type="checkbox"
              />允许自定义 exe</label
            ><label class="check"
              ><input
                v-model="ss.settings.auditEnabled"
                type="checkbox"
              />记录审计日志</label
            ><label class="check"
              ><input
                v-model="ss.settings.historyEnabled"
                type="checkbox"
              />保留终端历史</label
            ><label class="wide"
              >黑名单（每行一个关键字，正则以 regex: 开头）<textarea
                v-model="blacklistText"
                rows="6"
              />
            </label>
          </div>
          <div class="actions">
            <button @click="exportConfig">导出 JSON</button
            ><button @click="importConfig">导入 JSON（合并）</button
            ><button @click="openDataDir()">打开数据目录</button>
          </div>
          <p class="muted">管理员预设需要先以管理员身份启动 CmdDeck。</p>
          <code>{{ ss.settings.dataDir }}</code></template
        >
      </section>
    </main>
    <footer class="statusbar">
      <span
        ><span class="live-dot" />{{ ready ? "就绪" : "正在连接后端" }} ·
        {{ ps.presets.length }} 条预设</span
      ><span
        >{{ ts.runningCount }} 个运行中 · {{ ts.tabs.length }} 个终端 ·
        WebView2</span
      ><span>Ctrl K 快速启动</span>
    </footer>
    <div v-if="failure" class="overlay">
      <section class="modal">
        <h2>启动失败</h2>
        <p>{{ failure }}</p>
        <p>请通过 Start.ps1 启动桌面程序，浏览器预览不连接 Rust 后端。</p>
      </section>
    </div>
    <div v-if="welcome" class="overlay">
      <section class="modal">
        <span class="eyebrow">WELCOME TO CMDDECK</span>
        <h1>你的指令，一个工作台。</h1>
        <p>
          左侧保存常用指令，右侧查看实时终端。已准备示例预设，双击即可填写参数并运行。
        </p>
        <p>Ctrl K 快速搜索；关闭窗口后在托盘继续运行，退出请使用托盘菜单。</p>
        <button class="primary" @click="finishWelcome">开始使用</button>
      </section>
    </div>
    <div v-if="quick" class="overlay" @click.self="quick = false">
      <section class="modal launcher">
        <input
          v-model="quickQuery"
          autofocus
          placeholder="快速启动：搜索名称或命令"
        /><button
          v-for="p in quickResults"
          :key="p.id"
          class="launcher-item"
          @click="requestRun(p)"
        >
          <strong>{{ p.favorite ? "★ " : "" }}{{ p.name }}</strong
          ><small>{{ p.kind }}</small>
        </button>
      </section>
    </div>
    <div v-if="editor" class="overlay">
      <form class="modal large" @submit.prevent="savePreset">
        <div class="modal-heading">
          <h2>编辑预设</h2>
          <button type="button" @click="editor = null">×</button>
        </div>
        <div class="form-grid">
          <label>名称<input v-model="editor.name" required /></label
          ><label
            >执行类型<select
              v-model="editor.kind"
              @change="
                editor.useShell = [
                  'cmd',
                  'powershell',
                  'pwsh',
                  'shell',
                ].includes(editor.kind)
              "
            >
              <option value="powershell">PowerShell</option>
              <option value="cmd">CMD</option>
              <option value="pwsh">PowerShell 7</option>
              <option value="python">Python</option>
              <option value="node">Node</option>
              <option value="exe">自定义 exe</option>
            </select></label
          ><label class="wide"
            >命令 / 可执行程序<textarea
              v-model="editor.program"
              rows="3"
              placeholder="Shell 填完整脚本；Python/Node 可留空；支持 {{name}} 占位符"
            />
            <small v-if="['powershell', 'pwsh'].includes(editor.kind)"
              >支持多行脚本、$env: 环境变量和 &amp; 调用。所有行在同一个
              PowerShell 进程执行；相对路径以工作目录为起点。</small
            ></label
          ><label class="wide"
            >参数（每行一个，Shell 模式按空格追加）<textarea
              v-model="argsText"
              rows="3"
            /></label
          ><label
            >工作目录<input
              v-model="editor.workingDir"
              placeholder="留空则使用默认目录"
            />
            <button type="button" @click="pickWorkingDirectory(true)">
              选择文件夹
            </button></label
          ><label
            >运行环境<select
              :value="
                editor.runtime?.kind
                  ? `${editor.runtime.kind}:${editor.runtime.path}`
                  : ''
              "
              @change="
                bindEnvironment(($event.target as HTMLSelectElement).value)
              "
            >
              <option value="">继承系统环境</option>
              <option
                v-if="
                  editor.runtime?.kind &&
                  !environmentReport?.environments.some(
                    (e) =>
                      e.kind === editor!.runtime.kind &&
                      e.path === editor!.runtime.path,
                  )
                "
                :value="`${editor.runtime.kind}:${editor.runtime.path}`"
              >
                已保存：{{ editor.runtime.path }}
              </option>
              <option
                v-for="e in environmentReport?.environments"
                :key="e.id"
                :value="e.id"
              >
                {{ e.name }} · {{ e.kind }}
              </option></select
            ><button
              type="button"
              :disabled="environmentLoading"
              @click="refreshEnvironments"
            >
              {{ environmentLoading ? "检测中…" : "检测运行环境" }}</button
            ><small
              >选择只影响当前子进程；定时任务和工作流使用同一环境。</small
            ></label
          ><label
            >分组<select v-model="editor.groupId">
              <option value="">未分组</option>
              <option v-for="g in ps.groups" :key="g.id" :value="g.id">
                {{ g.name }}
              </option>
            </select></label
          ><label>标签（逗号分隔）<input v-model="tagsText" /></label
          ><label>图标标识<input v-model="editor.icon" /></label
          ><label class="wide"
            >环境变量（每行 KEY=VALUE）<textarea
              v-model="envText"
              rows="2"
            /></label
          ><label class="wide"
            >备注<textarea v-model="editor.notes" rows="2" /></label
          ><label class="check"
            ><input
              v-model="editor.confirm"
              type="checkbox"
            />运行前二次确认</label
          ><label class="check"
            ><input
              v-model="editor.elevated"
              type="checkbox"
            />管理员权限</label
          ><label class="check"
            ><input v-model="editor.favorite" type="checkbox" />收藏</label
          ><label class="check"
            ><input v-model="editor.useShell" type="checkbox" />通过所选 Shell
            执行</label
          >
        </div>
        <div class="actions">
          <button type="button" @click="editor = null">取消</button
          ><button class="primary">保存预设</button>
        </div>
      </form>
    </div>
    <div v-if="runPreset" class="overlay">
      <form class="modal" @submit.prevent="execute">
        <div class="modal-heading">
          <h2>运行 · {{ runPreset.name }}</h2>
          <button type="button" @click="runPreset = null">×</button>
        </div>
        <label v-for="key in runKeys" :key="key"
          >{{ key
          }}<input v-model="values[key]" required @input="updatePreview"
        /></label>
        <p>将执行：</p>
        <pre class="preview">{{ preview }}</pre>
        <p v-if="verdictText" class="warning">{{ verdictText }}</p>
        <p v-if="runPreset.elevated" class="warning">
          需要以管理员身份启动 CmdDeck
        </p>
        <div class="actions">
          <button type="button" @click="runPreset = null">取消</button
          ><button class="primary" :disabled="busy">
            {{ busy ? "启动中…" : "运行" }}
          </button>
        </div>
      </form>
    </div>
    <div v-if="ui.confirmState.open" class="overlay confirmation">
      <section class="modal">
        <h2>{{ ui.confirmState.title }}</h2>
        <p>{{ ui.confirmState.message }}</p>
        <pre v-if="ui.confirmState.detail">{{ ui.confirmState.detail }}</pre>
        <div class="actions">
          <button @click="ui.resolveConfirm(false)">取消</button
          ><button
            :class="ui.confirmState.danger ? 'danger' : 'primary'"
            @click="ui.resolveConfirm(true)"
          >
            确认
          </button>
        </div>
      </section>
    </div>
    <div v-if="ui.dialog === 'batchRun'" class="overlay">
      <section class="modal">
        <h2>批量执行 {{ selected.length }} 条预设</h2>
        <p>并发 {{ concurrency }} 个；需要二次确认的命令会被跳过。</p>
        <label
          >所有预设共享的参数 JSON<textarea v-model="batchArgs" rows="5" />
        </label>
        <div class="actions">
          <button @click="ui.closeDialog()">取消</button
          ><button
            class="primary"
            @click="
              ui.closeDialog();
              batch();
            "
          >
            开始执行
          </button>
        </div>
      </section>
    </div>
    <div v-if="workflowEditor" class="overlay">
      <form class="modal large" @submit.prevent="saveWorkflow">
        <div class="modal-heading">
          <h2>编辑工作流</h2>
          <button type="button" @click="workflowEditor = null">×</button>
        </div>
        <label>名称<input v-model="workflowEditor.name" required /></label
        ><label>说明<input v-model="workflowEditor.description" /></label
        ><label
          >模式<select v-model="workflowEditor.runMode">
            <option value="serial">串行</option>
            <option value="parallel">并行（最多 4 步同时执行）</option>
          </select></label
        ><label class="check"
          ><input
            v-model="workflowEditor.continueOnError"
            type="checkbox"
          />失败后继续</label
        >
        <div v-for="(s, i) in workflowEditor.steps" :key="s.id" class="step">
          <strong>{{ i + 1 }}.</strong
          ><select v-model="s.presetId">
            <option v-for="p in ps.presets" :key="p.id" :value="p.id">
              {{ p.name }}
            </option></select
          ><select v-model="s.condition">
            <option value="always">始终</option>
            <option value="on_success">上一步成功</option>
            <option value="on_fail">上一步失败</option></select
          ><input
            v-model.number="s.delayMs"
            type="number"
            min="0"
            title="延迟毫秒"
          /><button type="button" @click="workflowEditor.steps.splice(i, 1)">
            ×</button
          ><label class="wide"
            >步骤参数 JSON<textarea
              :value="JSON.stringify(s.args)"
              @change="
                attempt(async () => {
                  s.args = JSON.parse(
                    ($event.target as HTMLTextAreaElement).value,
                  );
                })
              "
            />
          </label>
        </div>
        <button type="button" @click="addStep">＋ 添加步骤</button>
        <div class="actions">
          <button type="button" @click="workflowEditor = null">取消</button
          ><button class="primary">保存</button>
        </div>
      </form>
    </div>
    <div v-if="scheduleEditor" class="overlay">
      <form class="modal" @submit.prevent="saveSchedule">
        <div class="modal-heading">
          <h2>编辑定时任务</h2>
          <button type="button" @click="scheduleEditor = null">×</button>
        </div>
        <label>名称<input v-model="scheduleEditor.name" required /></label
        ><label
          >执行预设<select v-model="scheduleEditor.presetId">
            <option v-for="p in ps.presets" :key="p.id" :value="p.id">
              {{ p.name }}
            </option>
          </select></label
        ><label
          >模式<select v-model="scheduleEditor.mode">
            <option value="interval">间隔</option>
            <option value="daily">每天</option>
            <option value="weekly">每周</option>
            <option value="once">单次</option>
          </select></label
        ><label v-if="scheduleEditor.mode === 'interval'"
          >间隔分钟<input
            v-model.number="scheduleEditor.intervalMinutes"
            type="number"
            min="1" /></label
        ><label v-else
          >北京时间<input v-model="scheduleEditor.time" type="time" /></label
        ><label v-if="scheduleEditor.mode === 'once'"
          >日期<input v-model="scheduleEditor.date" type="date"
        /></label>
        <div v-if="scheduleEditor.mode === 'weekly'" class="weekdays">
          <label v-for="n in 7" :key="n" class="check"
            ><input
              v-model="scheduleEditor.weekdays"
              type="checkbox"
              :value="n"
            />{{ ["一", "二", "三", "四", "五", "六", "日"][n - 1] }}</label
          >
        </div>
        <label>参数 JSON<textarea v-model="scheduleArgs" rows="3" /></label
        ><label class="check"
          ><input v-model="scheduleEditor.enabled" type="checkbox" />启用</label
        >
        <div class="actions">
          <button type="button" @click="scheduleEditor = null">取消</button
          ><button class="primary">保存</button>
        </div>
      </form>
    </div>
    <div class="toast-host">
      <button
        v-for="t in ui.toasts"
        :key="t.id"
        class="toast"
        :class="t.type"
        @click="ui.dismissToast(t.id)"
      >
        <strong>{{ t.title }}</strong
        ><span>{{ t.message }}</span>
      </button>
    </div>
  </div>
</template>
