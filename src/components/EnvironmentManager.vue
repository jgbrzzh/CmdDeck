<script setup lang="ts">
// 只封装已有工具，检测不下载软件，也不修改系统环境变量。
import { computed, onMounted, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { environmentApi } from "@/api/environments";
import { toFriendlyError } from "@/api";
import { useUiStore } from "@/stores/ui";
import { useTerminalStore } from "@/stores/terminals";
import type {
  EnvironmentInfo,
  EnvironmentReport,
  EnvironmentAction,
  RuntimeBinding,
} from "@/types";
const props = defineProps<{ defaultProject: string }>();
const emit = defineEmits<{
  updated: [report: EnvironmentReport];
  preset: [environment: EnvironmentInfo];
}>();
const ui = useUiStore(),
  terminal = useTerminalStore();
const project = ref(props.defaultProject),
  report = ref<EnvironmentReport | null>(null),
  loading = ref(false),
  busy = ref(false),
  error = ref("");
const tool = ref("python"),
  action = ref<EnvironmentAction["action"]>("list-packages"),
  selected = ref(""),
  pkg = ref(""),
  name = ref(""),
  version = ref("3.12");
const managers = computed(
  () =>
    report.value?.tools.filter((t) =>
      [
        "python",
        "conda",
        "uv",
        "npm",
        "pnpm",
        "yarn",
        "fnm",
        "nvm",
        "volta",
      ].includes(t.name),
    ) ?? [],
);
const actions = computed(() =>
  ["fnm", "nvm", "volta"].includes(tool.value)
    ? [{ value: "install-version", label: "安装 Node 版本" }]
    : tool.value === "uv"
      ? [{ value: "create-environment", label: "创建项目 .venv" }]
      : [
          { value: "list-packages", label: "列出已安装包" },
          { value: "install-package", label: "安装包" },
          ...(["python", "conda"].includes(tool.value)
            ? [
                {
                  value: "create-environment",
                  label:
                    tool.value === "conda"
                      ? "创建 Conda 环境"
                      : "创建项目 .venv",
                },
              ]
            : []),
        ],
);
const profiles = computed(() =>
  (report.value?.environments ?? []).filter((e) =>
    tool.value === "conda"
      ? e.kind === "conda"
      : tool.value === "python"
        ? ["conda", "python", "venv"].includes(e.kind)
        : e.kind === "node",
  ),
);
watch(tool, () => {
  action.value = actions.value[0]?.value as EnvironmentAction["action"];
  selected.value = "";
  version.value = ["fnm", "nvm", "volta"].includes(tool.value) ? "22" : "3.12";
});
async function refresh() {
  if (loading.value) return;
  loading.value = true;
  error.value = "";
  try {
    report.value = await environmentApi.discover(project.value);
    emit("updated", report.value);
    if (!managers.value.some((t) => t.name === tool.value))
      tool.value = managers.value[0]?.name ?? "";
  } catch (e) {
    error.value = toFriendlyError(e).message;
  } finally {
    loading.value = false;
  }
}
async function pickProject() {
  try {
    const p = await open({
      directory: true,
      multiple: false,
      title: "选择项目，检测 .venv / venv",
    });
    if (typeof p === "string") {
      project.value = p;
      await refresh();
    }
  } catch (e) {
    error.value = toFriendlyError(e).message;
  }
}
function binding(e?: EnvironmentInfo): RuntimeBinding {
  return e
    ? { kind: e.kind, path: e.path, managerPath: e.managerPath }
    : { kind: "", path: "", managerPath: "" };
}
async function openTerminal(e: EnvironmentInfo) {
  try {
    const info = await environmentApi.openTerminal(binding(e), project.value);
    await terminal.attach(info, "");
    ui.toast("success", "已打开环境终端", "终端位于预设页面，可直接输入命令");
  } catch (err) {
    error.value = toFriendlyError(err).message;
  }
}
async function run() {
  if (busy.value) return;
  const env = profiles.value.find((e) => e.id === selected.value),
    modifying = action.value !== "list-packages";
  if (modifying) {
    const ok = await ui.confirm({
      title: "确认管理环境",
      message:
        "此操作会调用现有管理器创建环境或安装软件包。项目依赖文件可能更新；Volta 安装版本可能改变默认版本。",
      detail: `${tool.value} · ${action.value}\n${project.value || "用户主目录"}\n${pkg.value || name.value || version.value}`,
      danger: false,
    });
    if (!ok) return;
  }
  busy.value = true;
  error.value = "";
  try {
    const info = await environmentApi.runAction({
      tool: tool.value,
      action: action.value,
      package: pkg.value,
      name: name.value,
      version: version.value,
      projectDir: project.value,
      runtime: binding(env),
      confirmed: modifying,
    });
    await terminal.attach(info, "");
    ui.toast(
      "success",
      "操作已启动",
      "在终端中查看输出和退出码；完成后刷新环境列表",
    );
  } catch (e) {
    error.value = toFriendlyError(e).message;
  } finally {
    busy.value = false;
  }
}
onMounted(refresh);
</script>
<template>
  <section class="manager">
    <header class="section-heading">
      <div>
        <h1>运行环境</h1>
        <p>
          复用 Conda / venv / uv 与 npm / pnpm / fnm / nvm /
          Volta，操作输出进入内嵌终端。
        </p>
      </div>
      <button :disabled="loading" @click="refresh">
        {{ loading ? "正在检测…" : "刷新环境" }}
      </button>
    </header>
    <div class="settings-grid">
      <label
        >项目目录<input
          v-model="project"
          placeholder="选择项目后检测 .venv；留空使用用户主目录"
        /><button @click="pickProject">选择项目文件夹</button></label
      >
    </div>
    <p v-if="error" role="alert" class="danger-text">{{ error }}</p>
    <p v-for="w in report?.warnings" :key="w" class="muted">{{ w }}</p>
    <h2>已检测到的工具</h2>
    <div class="tool-list">
      <div v-for="t in report?.tools" :key="t.name" class="tool-card">
        <strong>{{ t.name }}</strong
        ><span>{{ t.version }}</span
        ><code>{{ t.path }}</code>
      </div>
    </div>
    <p v-if="report && !report.tools.length">
      未检测到工具。请先安装 Python / Node / Conda，重启 CmdDeck 后刷新。
    </p>
    <h2>已有环境</h2>
    <div class="environment-list">
      <article
        v-for="e in report?.environments"
        :key="e.id"
        class="environment-card"
      >
        <div>
          <strong>{{ e.name }}</strong
          ><span class="tag">{{ e.kind }} · {{ e.manager }}</span
          ><code>{{ e.path }}</code>
        </div>
        <div class="actions">
          <button @click="emit('preset', e)">新建预设</button
          ><button @click="openTerminal(e)">打开终端</button>
        </div>
      </article>
    </div>
    <h2>管理环境与依赖</h2>
    <div class="settings-grid">
      <label
        >管理器<select v-model="tool">
          <option v-for="t in managers" :key="t.name" :value="t.name">
            {{ t.name }}
          </option>
        </select></label
      ><label
        >操作<select v-model="action">
          <option v-for="a in actions" :key="a.value" :value="a.value">
            {{ a.label }}
          </option>
        </select></label
      ><label v-if="['list-packages', 'install-package'].includes(action)"
        >目标环境<select v-model="selected">
          <option value="">系统默认（Conda 需选择环境）</option>
          <option v-for="e in profiles" :key="e.id" :value="e.id">
            {{ e.name }}
          </option>
        </select></label
      ><label v-if="action === 'install-package'"
        >包名与版本<input
          v-model="pkg"
          placeholder="例如 requests==2.32.3 或 lodash@4.17.21" /></label
      ><label v-if="action === 'create-environment' && tool === 'conda'"
        >环境名<input v-model="name" placeholder="例如 my-project" /></label
      ><label
        v-if="
          action === 'install-version' ||
          (action === 'create-environment' && tool === 'conda')
        "
        >版本<input v-model="version"
      /></label>
    </div>
    <button class="primary" :disabled="busy || !tool" @click="run">
      {{ busy ? "正在启动…" : "运行管理器" }}
    </button>
    <p class="muted">
      检测范围：PATH、Conda 环境、py 启动器、选定项目的虚拟环境，以及 NVM_HOME /
      FNM_DIR 中的 Node
      安装。创建和安装需确认；Volta 安装可能更新默认版本。环境终端输入不受预设黑名单过滤。
    </p>
  </section>
</template>
<style scoped>
.tool-list,
.environment-list {
  display: grid;
  gap: 12px;
  margin: 16px 0 24px;
}
.tool-list {
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
}
.tool-card,
.environment-card {
  padding: 16px;
  background: var(--deck-card);
  border: 1px solid var(--deck-border);
  border-radius: 10px;
  min-width: 0;
}
.tool-card {
  display: grid;
  gap: 8px;
}
.tool-card span {
  font-size: 12px;
  overflow-wrap: anywhere;
}
.tool-card code,
.environment-card code {
  display: block;
  font-size: 12px;
  margin-top: 10px;
  color: var(--deck-muted);
  overflow-wrap: anywhere;
}
.environment-card {
  display: flex;
  gap: 14px;
  justify-content: space-between;
  align-items: center;
}
.environment-card .tag {
  margin-left: 10px;
}
.environment-card > div {
  min-width: 0;
}
.actions {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}
.danger-text {
  color: #ff7878;
}
.manager h2 {
  font-size: 15px;
  margin: 24px 0 10px;
}
.manager > p {
  font-size: 12px;
  line-height: 1.7;
}
</style>
