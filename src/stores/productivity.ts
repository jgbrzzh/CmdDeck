import { computed, ref } from "vue";
import { productivityApi } from "@/api/productivity";
import type { Productivity, Layout } from "@/types/productivity";
const config = ref<Productivity>({
  workspaces: [],
  activeWorkspace: "",
  layouts: {},
  notificationMode: "off",
  notificationMinSeconds: 10,
  restoreLayout: true,
  logMaxMegabytes: 128,
});
const workspace = computed(() =>
  config.value.workspaces.find((w) => w.id === config.value.activeWorkspace),
);
const layoutMode = ref<Layout["mode"]>("single"),
  secondaryId = ref("");
async function load() {
  config.value = await productivityApi.get();
}
async function save() {
  config.value = await productivityApi.save(
    JSON.parse(JSON.stringify(config.value)),
  );
}
export function useProductivityStore() {
  return { config, workspace, layoutMode, secondaryId, load, save };
}
