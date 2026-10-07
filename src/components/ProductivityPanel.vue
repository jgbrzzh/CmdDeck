<script setup lang="ts">
import {
  computed,
  nextTick,
  onMounted,
  onBeforeUnmount,
  reactive,
  ref,
  watch,
} from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import { productivityApi } from "@/api/productivity";
import { environmentApi } from "@/api/environments";
import { terminalApi, systemApi, toFriendlyError } from "@/api";
import { useProductivityStore } from "@/stores/productivity";
import { usePresetStore } from "@/stores/presets";
import { useTerminalStore } from "@/stores/terminals";
import { useUiStore } from "@/stores/ui";
import type {
  Workspace,
  TaskInfo,
  PortInfo,
  BackupInfo,
  ShareOptions,
  UpdateInfo,
} from "@/types/productivity";
import type { EnvironmentInfo, TerminalHistory, TerminalInfo } from "@/types";
const props = defineProps<{ page: string }>();
const emit = defineEmits<{ run: [id: string] }>();
const store = reactive(useProductivityStore()),
  presets = reactive(usePresetStore()),
  terminals = reactive(useTerminalStore()),
  ui = reactive(useUiStore());
const editor = ref<Workspace | null>(null),
  envText = ref(""),
  portsText = ref(""),
  environments = ref<EnvironmentInfo[]>([]),
  working = ref(false),
  error = ref("");
const tasks = ref<TaskInfo[]>([]),
  ports = ref<PortInfo[]>([]),
  backups = ref<BackupInfo[]>([]),
  logs = ref<TerminalHistory[]>([]);
const logText = ref(""),
  logSearch = ref(""),
  logTitle = ref(""),
  logSelection = ref<{
    sessionId: string | null;
    historyId: number | null;
  } | null>(null);
const update = ref<UpdateInfo | null>(null),
  appVersion = ref("");
const share = reactive<ShareOptions>({
    presetIds: [],
    includeEnvironment: false,
    includePaths: false,
    includeSettings: false,
  }),
  exportOpen = ref(false),
  exportPreview = ref("");
const filteredLines = computed(() =>
  logText.value
    .split(/\r?\n/)
    .filter(
      (line) =>
        !logSearch.value ||
        line.toLowerCase().includes(logSearch.value.toLowerCase()),
    ),
);
const visiblePorts = computed(() =>
  store.workspace?.ports.length
    ? ports.value.filter(
        (p) => store.workspace!.ports.includes(p.port) || p.sessionId,
      )
    : ports.value,
);
let timer = 0,
  refreshBusy = false,
  sequence = 0;
async function attempt(fn: () => Promise<unknown>) {
  error.value = "";
  try {
    return await fn();
  } catch (e) {
    error.value = toFriendlyError(e).message;
    ui.toast("error", "操作失败", error.value, 8000);
    return undefined;
  }
}
async function refresh() {
  if (refreshBusy) return;
  refreshBusy = true;
  await attempt(async () => {
    if (props.page === "tasks") {
      [tasks.value, ports.value] = await Promise.all([
        productivityApi.tasks(),
        productivityApi.ports(),
      ]);
    }
    if (props.page === "backups")
      backups.value = await productivityApi.backups();
    if (props.page === "logs") {
      logs.value = await productivityApi.logEntries();
      if (logSelection.value?.sessionId)
        await readLive(logSelection.value.sessionId);
    }
  });
  refreshBusy = false;
}
function edit(w?: Workspace) {
  editor.value = w
    ? JSON.parse(JSON.stringify(w))
    : {
        id: crypto.randomUUID(),
        name: "新项目",
        directory: "",
        python: { kind: "", path: "", managerPath: "" },
        node: { kind: "", path: "", managerPath: "" },
        env: [],
        presetIds: [],
        ports: [],
      };
  envText.value = editor
    .value!.env.map((e) => `${e.name}=${e.value}`)
    .join("\n");
  portsText.value = editor.value!.ports.join(", ");
}
async function pickDirectory() {
  await attempt(async () => {
    const path = await open({
      directory: true,
      multiple: false,
      title: "选择项目目录",
    });
    if (typeof path === "string" && editor.value) editor.value.directory = path;
  });
}
async function detect() {
  await attempt(async () => {
    working.value = true;
    environments.value = (
      await environmentApi.discover(editor.value?.directory || "")
    ).environments;
  });
  working.value = false;
}
function bind(which: "python" | "node", id: string) {
  if (!editor.value) return;
  const e = environments.value.find((e) => e.id === id);
  editor.value[which] = e
    ? { kind: e.kind, path: e.path, managerPath: e.managerPath }
    : { kind: "", path: "", managerPath: "" };
}
async function saveProject() {
  await attempt(async () => {
    if (!editor.value) return;
    const w = editor.value;
    w.env = envText.value
      .split("\n")
      .filter(Boolean)
      .map((line) => {
        const i = line.indexOf("=");
        if (i < 1) throw new Error("环境变量使用 KEY=VALUE，每行一条");
        return { name: line.slice(0, i).trim(), value: line.slice(i + 1) };
      });
    w.ports = portsText.value
      .split(/[,，\s]+/)
      .filter(Boolean)
      .map((s) => {
        const n = Number(s);
        if (!Number.isInteger(n) || n < 1 || n > 65535)
          throw new Error("端口应为 1～65535 的整数");
        return n;
      });
    const old = JSON.parse(JSON.stringify(store.config));
    store.config.workspaces = [
      ...store.config.workspaces.filter((p) => p.id !== w.id),
      w,
    ];
    try {
      await store.save();
      editor.value = null;
    } catch (e) {
      store.config = old;
      throw e;
    }
  });
}
async function remove(w: Workspace) {
  if (
    !(await ui.confirm({
      title: "删除项目",
      message: `删除「${w.name}」？预设和运行中的任务会保留。`,
      danger: true,
    }))
  )
    return;
  await attempt(async () => {
    const old = JSON.parse(JSON.stringify(store.config));
    store.config.workspaces = store.config.workspaces.filter(
      (p) => p.id !== w.id,
    );
    delete store.config.layouts[w.id];
    if (store.config.activeWorkspace === w.id)
      store.config.activeWorkspace = "";
    try {
      await store.save();
    } catch (e) {
      store.config = old;
      throw e;
    }
  });
}
async function select(w: Workspace) {
  const old = store.config.activeWorkspace;
  let success = false;
  await attempt(async () => {
    store.config.activeWorkspace = w.id;
    try {
      await store.save();
      success = true;
      ui.toast("success", `已切换到 ${w.name}`);
    } catch (e) {
      store.config.activeWorkspace = old;
      throw e;
    }
  });
  return success;
}
async function projectRun(w: Workspace, id: string) {
  if (await select(w)) emit("run", id);
}
function plain(text: string) {
  return text.replace(
    /\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\)|[@-_])/g,
    "",
  );
}
async function readLive(id: string) {
  const selected = logSelection.value?.sessionId;
  if (selected !== id) return;
  if (id.startsWith("restored-")) {
    logText.value = plain(
      terminals.byId(id)?.pendingSnapshot || terminals.getBuffer(id),
    );
    return;
  }
  const text = await terminalApi.snapshot(id);
  if (logSelection.value?.sessionId === id) logText.value = plain(text);
}
async function liveLog(id: string) {
  const token = ++sequence;
  logSelection.value = { sessionId: id, historyId: null };
  logTitle.value = terminals.byId(id)?.title || "任务输出";
  logSearch.value = "";
  await attempt(async () => {
    await readLive(id);
    if (sequence !== token) return;
    ui.setView("logs");
  });
}
async function historyLog(h: TerminalHistory) {
  const token = ++sequence;
  logSelection.value = { sessionId: null, historyId: h.id };
  logTitle.value = h.title;
  logSearch.value = "";
  await attempt(async () => {
    const text = await productivityApi.historyOutput(h.id);
    if (sequence === token) logText.value = plain(text);
  });
}
async function exportLog() {
  await attempt(async () => {
    if (!logSelection.value) return;
    if (logSelection.value.sessionId?.startsWith("restored-"))
      throw new Error("此标签尚未执行命令，没有任务日志可以导出");
    const path = await save({
      defaultPath: "CmdDeck-task.log",
      filters: [{ name: "日志", extensions: ["log", "txt"] }],
    });
    if (path) {
      await productivityApi.exportLog(
        path,
        logSelection.value.sessionId,
        logSelection.value.historyId,
      );
      ui.toast("success", "日志已导出", path);
    }
  });
}
async function revealTask(info: TerminalInfo, rerun = false) {
  const id = info.workspaceId || "";
  store.config.activeWorkspace = store.config.workspaces.some(
    (w) => w.id === id,
  )
    ? id
    : "";
  if (rerun) {
    await nextTick();
    emit("run", info.presetId);
    return;
  }
  await terminals.attach(info, await terminalApi.snapshot(info.sessionId));
  ui.setView("terminals");
  await nextTick();
  terminals.setActive(info.sessionId);
}
async function stop(id: string) {
  if (
    await ui.confirm({
      title: "停止任务",
      message: "将终止此任务及其子进程，是否继续？",
      danger: true,
    })
  ) {
    await attempt(() => terminals.stop(id));
    await refresh();
  }
}
async function backup() {
  await attempt(async () => {
    await productivityApi.backup();
    await refresh();
    ui.toast("success", "配置快照已保存");
  });
}
async function restore(id: string) {
  if (
    !(await ui.confirm({
      title: "恢复配置",
      message: "当前配置会先备份，然后替换为所选快照。请先停止运行中的任务。",
      danger: true,
    }))
  )
    return;
  await attempt(async () => {
    await productivityApi.restore(id, true);
    await store.load();
    await presets.load();
    await refresh();
    ui.toast("success", "配置已恢复", "外观和系统集成设置在重新启动后完整生效");
  });
}
async function openExport() {
  share.presetIds = presets.presets.filter((p) => !p.hidden).map((p) => p.id);
  exportOpen.value = true;
  await previewExport();
}
async function previewExport() {
  await attempt(async () => {
    exportPreview.value = await productivityApi.preview(
      JSON.parse(JSON.stringify(share)),
    );
  });
}
async function exportConfig() {
  await attempt(async () => {
    const path = await save({
      defaultPath: "CmdDeck-shared.json",
      filters: [{ name: "JSON 配置", extensions: ["json"] }],
    });
    if (path) {
      await productivityApi.export(
        path,
        JSON.parse(JSON.stringify(share)),
        exportPreview.value,
      );
      exportOpen.value = false;
      ui.toast("success", "分享配置已导出", path);
    }
  });
}
async function savePreferences() {
  await attempt(async () => {
    await store.save();
    ui.toast("success", "通知与恢复偏好已保存");
  });
}
async function openService(port: number) {
  await attempt(() => productivityApi.openService(port));
}
async function openReleases() {
  await attempt(() => productivityApi.releases());
}
async function testNotification() {
  await attempt(() => productivityApi.testNotification());
}
async function checkUpdate() {
  working.value = true;
  update.value = null;
  await attempt(async () => {
    update.value = await productivityApi.checkUpdate();
  });
  working.value = false;
}
watch(
  () => props.page,
  () => void refresh(),
);
onMounted(async () => {
  await refresh();
  timer = window.setInterval(() => {
    if (["tasks", "logs"].includes(props.page) && !document.hidden)
      void refresh();
  }, 4000);
  await attempt(async () => {
    appVersion.value = (await systemApi.appInfo()).version;
  });
});
onBeforeUnmount(() => clearInterval(timer));
</script>

<template>
  <p v-if="error" class="warning" role="alert">{{ error }}</p>
  <template v-if="page === 'projects'">
    <div class="page-heading">
      <div>
        <h1>项目工作区</h1>
        <p>切换目录、Python / Node 环境和常用预设。预设单独指定的设置优先。</p>
      </div>
      <button class="primary" @click="edit()">＋ 新建项目</button>
    </div>
    <div v-if="!store.config.workspaces.length" class="empty">
      添加一个项目，把目录、环境和指令放在一起。
    </div>
    <article
      v-for="w in store.config.workspaces"
      :key="w.id"
      class="product-card"
    >
      <h2>
        {{ w.name }}
        <small v-if="store.config.activeWorkspace === w.id">当前项目</small>
      </h2>
      <code>{{ w.directory }}</code>
      <p>
        Python：{{ w.python.path || "系统默认" }} · Node：{{
          w.node.path || "系统默认"
        }}
      </p>
      <p>关注端口：{{ w.ports.join("、") || "未指定" }}</p>
      <div class="product-actions">
        <button @click="select(w)">切换到项目</button
        ><button @click="edit(w)">编辑</button
        ><button @click="remove(w)">删除</button>
      </div>
      <div class="product-actions">
        <button
          v-for="id in w.presetIds"
          :key="id"
          :disabled="!presets.byId(id)"
          @click="projectRun(w, id)"
        >
          ▷ {{ presets.byId(id)?.name || "预设已删除" }}
        </button>
      </div>
    </article>
  </template>
  <template v-if="page === 'tasks'">
    <div class="page-heading">
      <div>
        <h1>任务与端口</h1>
        <p>每 4 秒更新。CPU 以单核为 100%，内存为任务进程树工作集合计。</p>
      </div>
      <button @click="refresh">刷新</button>
    </div>
    <article
      v-for="t in tasks"
      :key="t.terminal.sessionId"
      class="product-card"
    >
      <h2>
        {{ t.terminal.title }}
        <small>{{
          t.terminal.status === "running"
            ? "运行中"
            : t.terminal.status === "killed"
              ? "已停止"
              : "已结束"
        }}</small>
      </h2>
      <p>
        PID：{{ t.pids.join("、") || "—" }} · 内存：{{
          t.memoryBytes === null
            ? "不可用"
            : (t.memoryBytes / 1048576).toFixed(1) + " MB"
        }}
        · CPU：{{
          t.cpuPercent === null
            ? "采样中 / 不可用"
            : t.cpuPercent.toFixed(1) + "%"
        }}
        ·
        {{
          Math.max(
            0,
            Math.floor(
              ((t.terminal.endedAt || Date.now()) - t.terminal.startedAt) /
                1000,
            ),
          )
        }}
        秒
      </p>
      <code>{{ t.terminal.cwd }}</code>
      <p v-if="t.terminal.exitCode !== null">
        退出码：{{ t.terminal.exitCode }}
      </p>
      <div class="product-actions">
        <button @click="liveLog(t.terminal.sessionId)">查看日志</button
        ><button @click="revealTask(t.terminal)">打开终端</button
        ><button
          v-if="t.terminal.status === 'running'"
          @click="stop(t.terminal.sessionId)"
        >
          停止进程树</button
        ><button
          v-if="t.terminal.presetId"
          @click="revealTask(t.terminal, true)"
        >
          重新运行预设
        </button>
      </div>
    </article>
    <h2>TCP 监听端口</h2>
    <p>
      外部进程只显示信息。项目配置了关注端口时，列表优先显示这些端口与 CmdDeck
      任务端口。
    </p>
    <div class="port-scroll">
      <table>
        <thead>
          <tr>
            <th>地址</th>
            <th>端口</th>
            <th>PID</th>
            <th>归属</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="p in visiblePorts"
            :key="`${p.address}:${p.port}:${p.pid}`"
          >
            <td>{{ p.address }}</td>
            <td>{{ p.port }}</td>
            <td>{{ p.pid }}</td>
            <td>{{ p.sessionId ? "CmdDeck 任务" : "外部进程" }}</td>
            <td>
              <button @click="openService(p.port)">打开网页</button
              ><button v-if="p.sessionId" @click="stop(p.sessionId)">
                停止任务
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-if="!visiblePorts.length">没有匹配的 TCP 监听端口。</p>
  </template>
  <template v-if="page === 'logs'">
    <div class="page-heading">
      <div>
        <h1>日志中心</h1>
        <p>
          查看正在运行或已结束的任务输出。每条保留最近约 2
          MB，历史条数受设置限制。
        </p>
      </div>
      <button @click="refresh">刷新</button>
    </div>
    <div class="log-grid">
      <aside>
        <h3>当前会话</h3>
        <button
          v-for="t in terminals.tabs"
          :key="t.sessionId"
          class="log-entry"
          @click="liveLog(t.sessionId)"
        >
          {{ t.title }}
        </button>
        <h3>历史日志</h3>
        <button
          v-for="h in logs"
          :key="h.id"
          class="log-entry"
          @click="historyLog(h)"
        >
          {{ h.title
          }}<small
            >{{ new Date(h.startedAt).toLocaleString() }} ·
            {{ h.exitCode }}</small
          >
        </button>
      </aside>
      <section>
        <div class="product-actions">
          <h3>{{ logTitle || "选择一条日志" }}</h3>
          <button :disabled="!logSelection" @click="exportLog">导出日志</button>
        </div>
        <input
          v-model="logSearch"
          placeholder="搜索日志行…"
          aria-label="搜索日志行"
        />
        <p v-if="logSearch">匹配 {{ filteredLines.length }} 行</p>
        <pre class="log-output">{{ filteredLines.join("\n") }}</pre>
      </section>
    </div>
  </template>
  <template v-if="page === 'backups'">
    <div class="page-heading">
      <div>
        <h1>配置备份与分享</h1>
        <p>
          修改配置前自动保存快照，最多保留 30
          份。快照包含私人路径和环境变量，只保存在本机。
        </p>
      </div>
      <div class="product-actions">
        <button @click="openExport">分享预设</button
        ><button class="primary" @click="backup">立即备份</button>
      </div>
    </div>
    <article v-for="b in backups" :key="b.id" class="product-card">
      <h3>{{ new Date(b.createdAt).toLocaleString() }}</h3>
      <p>{{ (b.bytes / 1024).toFixed(1) }} KB · {{ b.id }}</p>
      <button @click="restore(b.id)">恢复这份配置</button>
    </article>
  </template>
  <template v-if="page === 'updates'">
    <div class="page-heading">
      <div>
        <h1>更新与通知</h1>
        <p>
          当前版本 {{ appVersion || "读取中" }} · 手动检查 GitHub 正式版本。
        </p>
      </div>
      <button :disabled="working" class="primary" @click="checkUpdate">
        {{ working ? "检查中…" : "检查更新" }}
      </button>
      <button @click="openReleases">打开下载页面</button>
    </div>
    <article v-if="update" class="product-card">
      <h2>
        {{ update.available ? "发现新版本" : "当前版本无需更新" }} ·
        {{ update.latest }}
      </h2>
      <p>{{ update.publishedAt }}</p>
      <pre class="release-notes">{{ update.notes }}</pre>
      <button @click="openReleases">打开 GitHub 下载页面</button>
    </article>
    <h2>任务通知</h2>
    <div class="settings-grid">
      <label
        >历史日志总容量（MB）<input
          v-model.number="store.config.logMaxMegabytes"
          type="number"
          min="4"
          max="2048"
      /></label>
      <label
        >通知方式<select v-model="store.config.notificationMode">
          <option value="off">关闭</option>
          <option value="all">任务结束和失败</option>
          <option value="failure">只通知失败</option>
        </select></label
      ><label
        >运行至少多少秒才通知<input
          v-model.number="store.config.notificationMinSeconds"
          type="number"
          min="0"
          max="86400" /></label
      ><label class="check"
        ><input
          v-model="store.config.restoreLayout"
          type="checkbox"
        />启动时恢复保存的终端布局（不执行命令）</label
      >
    </div>
    <div class="product-actions">
      <button @click="savePreferences">保存偏好</button
      ><button @click="testNotification">发送测试通知</button>
    </div>
  </template>

  <div v-if="editor" class="overlay">
    <form class="modal large" @submit.prevent="saveProject">
      <header class="modal-heading">
        <h2>项目设置</h2>
        <button type="button" @click="editor = null">×</button>
      </header>
      <label>名称<input v-model="editor.name" required /></label
      ><label
        >项目目录
        <div class="product-actions">
          <input v-model="editor.directory" required /><button
            type="button"
            @click="pickDirectory"
          >
            选择目录
          </button>
        </div></label
      ><button type="button" :disabled="working" @click="detect">
        识别此目录的环境
      </button>
      <label
        >Python 环境<select
          :value="
            environments.find((e) => e.path === editor!.python.path)?.id || ''
          "
          @change="bind('python', ($event.target as HTMLSelectElement).value)"
        >
          <option value="">系统默认 / 保留当前选择</option>
          <option
            v-for="e in environments.filter((e) => e.kind !== 'node')"
            :key="e.id"
            :value="e.id"
          >
            {{ e.name }} · {{ e.path }}
          </option></select
        ><small>{{ editor.python.path || "系统默认" }}</small
        ><button type="button" @click="bind('python', '')">
          清除绑定
        </button></label
      >
      <label
        >Node 环境<select
          :value="
            environments.find((e) => e.path === editor!.node.path)?.id || ''
          "
          @change="bind('node', ($event.target as HTMLSelectElement).value)"
        >
          <option value="">系统默认 / 保留当前选择</option>
          <option
            v-for="e in environments.filter((e) => e.kind === 'node')"
            :key="e.id"
            :value="e.id"
          >
            {{ e.name }} · {{ e.path }}
          </option></select
        ><small>{{ editor.node.path || "系统默认" }}</small
        ><button type="button" @click="bind('node', '')">
          清除绑定
        </button></label
      >
      <label
        >环境变量（每行 KEY=VALUE）<textarea
          v-model="envText"
          rows="3"
        /></label
      ><label
        >关注端口（逗号分隔）<input
          v-model="portsText"
          placeholder="3000, 8000"
      /></label>
      <fieldset>
        <legend>项目常用预设</legend>
        <label v-for="p in presets.presets" :key="p.id" class="check"
          ><input v-model="editor.presetIds" type="checkbox" :value="p.id" />{{
            p.name
          }}</label
        >
      </fieldset>
      <footer>
        <button type="button" @click="editor = null">取消</button
        ><button class="primary">保存项目</button>
      </footer>
    </form>
  </div>
  <div v-if="exportOpen" class="overlay">
    <form class="modal large" @submit.prevent="exportConfig">
      <header class="modal-heading">
        <h2>分享预设</h2>
        <button type="button" @click="exportOpen = false">×</button>
      </header>
      <p>
        默认排除环境变量、运行目录和环境绑定。命令与备注也可能包含私人信息，请检查下面的完整内容。
      </p>
      <fieldset>
        <legend>选择预设</legend>
        <label v-for="p in presets.presets" :key="p.id" class="check"
          ><input v-model="share.presetIds" type="checkbox" :value="p.id" />{{
            p.name
          }}</label
        >
      </fieldset>
      <label class="check"
        ><input
          v-model="share.includeEnvironment"
          type="checkbox"
        />包含环境变量</label
      ><label class="check"
        ><input
          v-model="share.includePaths"
          type="checkbox"
        />包含目录与环境绑定</label
      ><label class="check"
        ><input
          v-model="share.includeSettings"
          type="checkbox"
        />包含应用设置</label
      ><button type="button" @click="previewExport">更新预览</button>
      <pre class="export-preview">{{ exportPreview }}</pre>
      <footer>
        <button type="button" @click="exportOpen = false">取消</button
        ><button class="primary" :disabled="!share.presetIds.length">
          导出选中预设
        </button>
      </footer>
    </form>
  </div>
</template>

<style scoped>
.product-card {
  border: 1px solid var(--border-default);
  border-radius: 10px;
  padding: 16px;
  margin: 12px 0;
  overflow-wrap: anywhere;
}
.product-card h2 {
  margin: 0 0 10px;
}
.product-card small {
  font-size: 12px;
  color: var(--accent);
}
.product-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  margin: 10px 0;
}
.product-actions input {
  flex: 1;
  min-width: 180px;
}
.port-scroll {
  overflow: auto;
}
table {
  border-collapse: collapse;
  width: 100%;
}
th,
td {
  text-align: left;
  padding: 10px;
  border-bottom: 1px solid var(--border-default);
}
.log-grid {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
  gap: 16px;
}
.log-grid aside {
  max-height: 65vh;
  overflow: auto;
}
.log-entry {
  display: block;
  width: 100%;
  text-align: left;
  margin: 4px 0;
  overflow-wrap: anywhere;
}
.log-entry small {
  display: block;
  font-size: 11px;
}
.log-output,
.release-notes,
.export-preview {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  max-height: 55vh;
  overflow: auto;
  padding: 14px;
  background: var(--bg-app);
  border-radius: 8px;
}
.log-output {
  min-height: 240px;
  font-family: Consolas, monospace;
}
fieldset {
  max-height: 180px;
  overflow: auto;
  border: 1px solid var(--border-default);
  border-radius: 8px;
}
.modal {
  max-height: 90vh;
  overflow: auto;
}
label small {
  overflow-wrap: anywhere;
}
@media (max-width: 1100px) {
  .log-grid {
    grid-template-columns: 170px minmax(0, 1fr);
  }
}
</style>
