/**
 * 预设指令仓库（模块级单例）。
 *
 * 数据策略：**全量载入内存 + 前端过滤**。
 * 预设量级通常在几百条以内，一次性拉全量可以做到：
 * - 搜索框输入零延迟（不用每敲一下就查库）
 * - 收藏 / 最近 / 标签 / 分组计数都是纯 computed
 * - 拖拽排序能实时反馈，落库失败再回滚
 */

import { computed, ref, type ComputedRef, type Ref } from "vue";

import { groupApi, presetApi, toFriendlyError } from "@/api";
import { createEmptyGroup, createEmptyPreset } from "@/types";
import type { Placeholder, Preset, PresetGroup } from "@/types";
import { useUiStore } from "./ui";

/** 分组计数里代表"未分组"的 key */
export const UNGROUPED_KEY = "";
/** 分组计数里代表"全部"的 key */
export const ALL_KEY = "__all__";

export interface PresetStore {
  // ---- 原始数据 ----
  presets: Ref<Preset[]>;
  groups: Ref<PresetGroup[]>;
  loading: Ref<boolean>;

  // ---- 筛选条件（组件里可直接 v-model） ----
  keyword: Ref<string>;
  /** '' 表示全部 */
  activeGroupId: Ref<string>;
  activeTag: Ref<string>;
  onlyFavorite: Ref<boolean>;
  onlyRecent: Ref<boolean>;

  // ---- 展示偏好 ----
  sortBy: Ref<"default" | "name" | "recent" | "runCount">;
  viewMode: Ref<"list" | "grid">;
  selectedId: Ref<string>;

  // ---- 派生数据 ----
  favorites: ComputedRef<Preset[]>;
  /** lastRunAt > 0，按时间倒序，最多 10 条 */
  recents: ComputedRef<Preset[]>;
  /** 去重并排序后的全部标签 */
  allTags: ComputedRef<string[]>;
  /** 经过关键字 / 分组 / 标签 / 收藏 / 最近过滤后的列表 */
  visiblePresets: ComputedRef<Preset[]>;
  /** 按分组聚合，首项 group=null 表示"未分组" */
  groupedPresets: ComputedRef<{ group: PresetGroup | null; items: Preset[] }[]>;
  /** 分组 → 预设数量，另有 ALL_KEY 与 UNGROUPED_KEY 两个虚拟项 */
  groupCounts: ComputedRef<Record<string, number>>;
  /** 当前激活分组的显示名 */
  activeGroupName: ComputedRef<string>;

  // ---- 查询 ----
  byId(id: string): Preset | undefined;
  groupById(id: string): PresetGroup | undefined;

  // ---- 增删改 ----
  load(): Promise<void>;
  save(preset: Preset): Promise<Preset>;
  /** 生成一个空预设（不落库），配合 `save()` 使用 */
  createPreset(groupId?: string): Preset;
  remove(id: string): Promise<void>;
  removeMany(ids: string[]): Promise<void>;
  duplicate(id: string): Promise<Preset>;
  toggleFavorite(id: string): Promise<Preset>;
  recordRun(id: string): Promise<void>;
  reorder(ids: string[]): Promise<void>;
  moveToGroup(id: string, groupId: string, targetIndex: number): Promise<void>;

  // ---- 分组 ----
  saveGroup(group: PresetGroup): Promise<PresetGroup>;
  removeGroup(id: string): Promise<number>;
  /** 生成一个空分组对象（不落库） */
  createGroup(): PresetGroup;

  // ---- 辅助 ----
  scanPlaceholders(text: string): Promise<Placeholder[]>;
  preview(preset: Preset, args: Record<string, string>): Promise<string>;
  /** 清空全部筛选条件与选中项 */
  reset(): void;
}

// ============================================================
// 模块级状态
// ============================================================

const presets = ref<Preset[]>([]);
const groups = ref<PresetGroup[]>([]);
const loading = ref(false);

const keyword = ref("");
const activeGroupId = ref("");
const activeTag = ref("");
const onlyFavorite = ref(false);
const onlyRecent = ref(false);

const sortBy = ref<"default" | "name" | "recent" | "runCount">("default");
const viewMode = ref<"list" | "grid">("list");
const selectedId = ref("");

// ============================================================
// 内部工具
// ============================================================

/** 在数组中找到 id 对应的下标，-1 表示不存在 */
function indexOfId(list: { id: string }[], id: string): number {
  return list.findIndex((p) => p.id === id);
}

/**
 * 生成用于搜索的可检索文本。
 *
 * 覆盖：名称、备注、标签、程序、参数、工作目录。
 * 统一 `toLowerCase()`，中英文都能搜（中文没有大小写问题，原样匹配即可）。
 */
function searchableText(p: Preset): string {
  return [
    p.name ?? "",
    p.notes ?? "",
    p.tags?.join(" ") ?? "",
    p.program ?? "",
    p.args?.join(" ") ?? "",
    p.workingDir ?? "",
  ]
    .join("\n")
    .toLowerCase();
}

/** 是否命中关键字（空关键字视为全部命中） */
function matchKeyword(p: Preset, kw: string): boolean {
  if (!kw) return true;
  return searchableText(p).includes(kw);
}

/** 默认排序：收藏优先 → sortOrder 升序 → createdAt 升序 */
function sortDefault(list: Preset[]): Preset[] {
  return [...list].sort((a, b) => {
    if (a.favorite !== b.favorite) return a.favorite ? -1 : 1;
    if (a.sortOrder !== b.sortOrder) return a.sortOrder - b.sortOrder;
    return a.createdAt - b.createdAt;
  });
}

/** 统一的排序入口 */
function sortList(list: Preset[]): Preset[] {
  const copy = [...list];
  switch (sortBy.value) {
    case "name":
      return copy.sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));
    case "recent":
      return copy.sort(
        (a, b) => b.lastRunAt - a.lastRunAt || b.runCount - a.runCount,
      );
    case "runCount":
      return copy.sort(
        (a, b) => b.runCount - a.runCount || b.lastRunAt - a.lastRunAt,
      );
    case "default":
    default:
      return sortDefault(copy);
  }
}

/** 收藏的预设（用于顶栏收藏区） */
const favorites = computed<Preset[]>(() =>
  sortDefault(presets.value.filter((p) => p.favorite)),
);

/** 最近使用的预设：lastRunAt>0，倒序，最多 10 条 */
const recents = computed<Preset[]>(() =>
  presets.value
    .filter((p) => p.lastRunAt > 0)
    .sort((a, b) => b.lastRunAt - a.lastRunAt)
    .slice(0, 10),
);

/** 全部标签，去重后按「中文优先」排序 */
const allTags = computed<string[]>(() => {
  const set = new Set<string>();
  presets.value.forEach((p) => (p.tags ?? []).forEach((t) => t && set.add(t)));
  return [...set].sort((a, b) => a.localeCompare(b, "zh-CN"));
});

/** 核心过滤链：关键字 → 分组 → 标签 → 收藏 → 最近 */
const visiblePresets = computed<Preset[]>(() => {
  const kw = keyword.value.trim().toLowerCase();
  const gid = activeGroupId.value;
  const tag = activeTag.value;
  const fav = onlyFavorite.value;
  const recent = onlyRecent.value;

  const filtered = presets.value.filter((p) => {
    if (!matchKeyword(p, kw)) return false;
    if (gid === UNGROUPED_KEY) {
      if (p.groupId) return false;
    } else if (gid && gid !== ALL_KEY) {
      if (p.groupId !== gid) return false;
    }
    if (tag && !(p.tags ?? []).includes(tag)) return false;
    if (fav && !p.favorite) return false;
    if (recent && p.lastRunAt <= 0) return false;
    return true;
  });

  // sortBy 为 'default' 时保留"收藏优先"的语义
  return sortBy.value === "default"
    ? sortDefault(filtered)
    : sortList(filtered);
});

/** 按分组聚合，"未分组"永远排在最前 */
const groupedPresets = computed<
  { group: PresetGroup | null; items: Preset[] }[]
>(() => {
  const visible = visiblePresets.value;

  // 未分组
  const ungrouped = visible.filter((p) => !p.groupId);
  const buckets: { group: PresetGroup | null; items: Preset[] }[] = [];
  if (ungrouped.length > 0) buckets.push({ group: null, items: ungrouped });

  // 按 sortOrder / 名称排序后的分组逐个填充
  const orderedGroups = [...groups.value].sort((a, b) => {
    if (a.sortOrder !== b.sortOrder) return a.sortOrder - b.sortOrder;
    return a.name.localeCompare(b.name, "zh-CN");
  });

  orderedGroups.forEach((g) => {
    const items = visible.filter((p) => p.groupId === g.id);
    if (items.length > 0) buckets.push({ group: g, items });
  });

  return buckets;
});

/**
 * 分组计数。
 * key 为分组 id；额外提供：
 * - `ALL_KEY`（'__all__'）→ 预设总数
 * - `UNGROUPED_KEY`（''）→ 未分组数量
 * 计数基于**当前关键字与标签过滤**后的集合（不含分组条件本身），
 * 这样切换分组时数字不会变成只有当前组一个非零。
 */
const groupCounts = computed<Record<string, number>>(() => {
  const kw = keyword.value.trim().toLowerCase();
  const tag = activeTag.value;

  const base = presets.value.filter((p) => {
    if (!matchKeyword(p, kw)) return false;
    if (tag && !(p.tags ?? []).includes(tag)) return false;
    return true;
  });

  const counts: Record<string, number> = {
    [ALL_KEY]: base.length,
    [UNGROUPED_KEY]: 0,
  };
  base.forEach((p) => {
    if (p.groupId) counts[p.groupId] = (counts[p.groupId] ?? 0) + 1;
    else counts[UNGROUPED_KEY] += 1;
  });
  return counts;
});

/** 当前激活分组的显示名 */
const activeGroupName = computed<string>(() => {
  if (activeGroupId.value === UNGROUPED_KEY) return "未分组";
  if (!activeGroupId.value) return "全部";
  return groups.value.find((g) => g.id === activeGroupId.value)?.name ?? "全部";
});

// ============================================================
// 查询
// ============================================================

function byId(id: string): Preset | undefined {
  if (!id) return undefined;
  return presets.value.find((p) => p.id === id);
}

function groupById(id: string): PresetGroup | undefined {
  if (!id) return undefined;
  return groups.value.find((g) => g.id === id);
}

// ============================================================
// 数据加载与变更
// ============================================================

/**
 * 重新从后端拉取预设与分组。
 *
 * 两个接口并行发起，互不阻塞；任何一个失败都只提示 toast，不抛异常，
 * 保证界面在数据库异常时仍能用（只是数据可能是旧的）。
 */
async function load(): Promise<void> {
  loading.value = true;
  try {
    const [presetList, groupList] = await Promise.all([
      presetApi.list({ includeHidden: true }),
      groupApi.list(),
    ]);
    presets.value = Array.isArray(presetList) ? presetList : [];
    groups.value = Array.isArray(groupList) ? groupList : [];

    // 选中的预设可能已被删除
    if (selectedId.value && !byId(selectedId.value)) selectedId.value = "";
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 加载预设失败", e);
    useUiStore().toast("error", "加载预设失败", e.message);
  } finally {
    loading.value = false;
  }
}

/**
 * 保存预设（新增或更新）。
 *
 * 后端会做安全校验、生成 id、补默认字段，返回值才是权威数据。
 * 这里用返回值替换本地数组中同 id 的项，没有则插到末尾。
 */
async function save(preset: Preset): Promise<Preset> {
  try {
    const saved = await presetApi.save(preset);
    const list = [...presets.value];
    const idx = indexOfId(list, saved.id);
    if (idx >= 0) list[idx] = saved;
    else list.push(saved);
    presets.value = list;
    return saved;
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 保存预设失败", e);
    useUiStore().toast("error", "保存预设失败", e.message);
    throw e;
  }
}

/** 生成一个空预设（不落库），默认归入指定分组 */
function createPreset(groupId = ""): Preset {
  const p = createEmptyPreset();
  p.groupId = groupId || activeGroupId.value || "";
  // 新预设排在当前分组末尾
  const siblings = presets.value.filter((x) => x.groupId === p.groupId);
  p.sortOrder = siblings.reduce((max, x) => Math.max(max, x.sortOrder), 0) + 10;
  return p;
}

/** 删除一条预设 */
async function remove(id: string): Promise<void> {
  const target = byId(id);
  try {
    await presetApi.remove(id);
    presets.value = presets.value.filter((p) => p.id !== id);
    if (selectedId.value === id) selectedId.value = "";
    useUiStore().toast(
      "success",
      "已删除预设",
      target ? `「${target.name}」已移除` : "",
    );
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 删除预设失败", e);
    useUiStore().toast("error", "删除预设失败", e.message);
    throw e;
  }
}

/** 批量删除预设 */
async function removeMany(ids: string[]): Promise<void> {
  const list = (ids ?? []).filter(Boolean);
  if (list.length === 0) return;
  try {
    await presetApi.removeMany(list);
    const gone = new Set(list);
    presets.value = presets.value.filter((p) => !gone.has(p.id));
    if (selectedId.value && gone.has(selectedId.value)) selectedId.value = "";
    useUiStore().toast(
      "success",
      "批量删除完成",
      `共删除 ${list.length} 条预设`,
    );
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 批量删除预设失败", e);
    useUiStore().toast("error", "批量删除失败", e.message);
    throw e;
  }
}

/** 复制一条预设（后端负责生成新 id 与「副本」后缀） */
async function duplicate(id: string): Promise<Preset> {
  try {
    const copy = await presetApi.duplicate(id);
    presets.value = [...presets.value, copy];
    return copy;
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 复制预设失败", e);
    useUiStore().toast("error", "复制预设失败", e.message);
    throw e;
  }
}

/** 切换收藏状态 */
async function toggleFavorite(id: string): Promise<Preset> {
  try {
    const updated = await presetApi.toggleFavorite(id);
    const list = [...presets.value];
    const idx = indexOfId(list, id);
    if (idx >= 0) list[idx] = updated;
    presets.value = list;
    return updated;
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 切换收藏失败", e);
    useUiStore().toast("error", "切换收藏失败", e.message);
    throw e;
  }
}

/** 记录一次运行（累加次数 + 更新时间），影响"最近使用" */
async function recordRun(id: string): Promise<void> {
  try {
    const updated = await presetApi.recordRun(id);
    const list = [...presets.value];
    const idx = indexOfId(list, id);
    if (idx >= 0) list[idx] = updated;
    presets.value = list;
  } catch (err) {
    // 运行记录失败不应该打扰用户，只记日志
    console.warn("[CmdDeck] 记录运行次数失败", toFriendlyError(err));
  }
}

/**
 * 按给定顺序重排预设。
 *
 * @param ids 该分组（或全部）范围内的预设 ID 顺序
 */
async function reorder(ids: string[]): Promise<void> {
  const list = (ids ?? []).filter(Boolean);
  if (list.length === 0) return;

  // 本地先按 ids 顺序重写 sortOrder，保证界面立即生效
  const orderMap = new Map<string, number>();
  list.forEach((id, i) => orderMap.set(id, (i + 1) * 10));

  const snapshot = presets.value;
  presets.value = presets.value.map((p) => {
    const next = orderMap.get(p.id);
    return next === undefined ? p : { ...p, sortOrder: next };
  });

  try {
    await presetApi.reorder(list);
  } catch (err) {
    // 落库失败 → 回滚到拖拽前的快照
    presets.value = snapshot;
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 保存排序失败", e);
    useUiStore().toast("error", "保存排序失败", e.message);
    throw e;
  }
}

/**
 * 把一条预设移动到指定分组的第 `targetIndex` 个位置。
 *
 * 先在本地完成"移出原组 → 插入目标组指定位置"，让拖拽手感连贯；
 * 落库失败时整体 `load()` 回滚。
 */
async function moveToGroup(
  id: string,
  groupId: string,
  targetIndex: number,
): Promise<void> {
  const moving = byId(id);
  if (!moving) return;

  const snapshot = presets.value;
  const targetGroup = groupId || "";

  // 先把所有同分组预设按当前排序展开，算清楚目标位置
  const inTarget = sortDefault(
    presets.value.filter((p) => p.groupId === targetGroup && p.id !== id),
  );
  const others = presets.value.filter(
    (p) => p.groupId !== targetGroup && p.id !== id,
  );
  const idx = Math.max(0, Math.min(inTarget.length, targetIndex));
  inTarget.splice(idx, 0, { ...moving, groupId: targetGroup });

  // 同分组内的 sortOrder 重新连续编号
  const rebased = inTarget.map((p, i) => ({ ...p, sortOrder: (i + 1) * 10 }));
  presets.value = rebased.map((p) => {
    const hit = others.find((o) => o.id === p.id);
    return hit ?? p;
  });

  try {
    await presetApi.move(id, targetGroup, (idx + 1) * 10);
    // 后端 sortOrder 以传入值为准，这里刷新一次保证与库一致
    await load();
  } catch (err) {
    presets.value = snapshot;
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 移动预设失败", e);
    useUiStore().toast("error", "移动预设失败", e.message);
    throw e;
  }
}

// ============================================================
// 分组
// ============================================================

/** 保存分组（新增或更新） */
async function saveGroup(group: PresetGroup): Promise<PresetGroup> {
  try {
    const saved = await groupApi.save(group);
    const list = [...groups.value];
    const idx = list.findIndex((g) => g.id === saved.id);
    if (idx >= 0) list[idx] = saved;
    else list.push(saved);
    groups.value = list;
    return saved;
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 保存分组失败", e);
    useUiStore().toast("error", "保存分组失败", e.message);
    throw e;
  }
}

/**
 * 删除分组。
 * @returns 被移出该分组的预设数量（后端在落库前把这些预设的 groupId 置空）
 */
async function removeGroup(id: string): Promise<number> {
  try {
    const moved = await groupApi.remove(id);
    groups.value = groups.value.filter((g) => g.id !== id);
    if (activeGroupId.value === id) activeGroupId.value = "";

    // 受影响的预设变成"未分组"，按返回数量同步本地
    const affected = presets.value.filter((p) => p.groupId === id);
    if (affected.length > 0) {
      const gone = new Set(affected.map((p) => p.id));
      presets.value = presets.value.map((p) =>
        gone.has(p.id) ? { ...p, groupId: "", updatedAt: Date.now() } : p,
      );
    }

    useUiStore().toast(
      "success",
      "已删除分组",
      `${moved} 条预设已移动到「未分组」`,
    );
    return moved;
  } catch (err) {
    const e = toFriendlyError(err);
    console.error("[CmdDeck] 删除分组失败", e);
    useUiStore().toast("error", "删除分组失败", e.message);
    throw e;
  }
}

/** 生成一个空分组对象（不落库） */
function createGroup(): PresetGroup {
  const g = createEmptyGroup();
  g.sortOrder =
    groups.value.reduce((max, x) => Math.max(max, x.sortOrder), 0) + 10;
  return g;
}

// ============================================================
// 占位符辅助
// ============================================================

/** 扫描文本中的 `{{key}}`，由后端推断出控件类型与中文标签 */
async function scanPlaceholders(text: string): Promise<Placeholder[]> {
  return presetApi.scanPlaceholders(text);
}

/** 按给定参数预览最终命令行 */
async function preview(
  preset: Preset,
  args: Record<string, string>,
): Promise<string> {
  return presetApi.preview(preset, args);
}

// ============================================================
// 重置
// ============================================================

/** 清空全部筛选条件与选中项（"清除筛选"按钮用） */
function reset(): void {
  keyword.value = "";
  activeGroupId.value = "";
  activeTag.value = "";
  onlyFavorite.value = false;
  onlyRecent.value = false;
  selectedId.value = "";
}

// ============================================================
// 导出
// ============================================================

const store: PresetStore = {
  presets,
  groups,
  loading,

  keyword,
  activeGroupId,
  activeTag,
  onlyFavorite,
  onlyRecent,

  sortBy,
  viewMode,
  selectedId,

  favorites,
  recents,
  allTags,
  visiblePresets,
  groupedPresets,
  groupCounts,
  activeGroupName,

  byId,
  groupById,

  load,
  save,
  createPreset,
  remove,
  removeMany,
  duplicate,
  toggleFavorite,
  recordRun,
  reorder,
  moveToGroup,

  saveGroup,
  removeGroup,
  createGroup,

  scanPlaceholders,
  preview,
  reset,
};

export function usePresetStore(): PresetStore {
  return store;
}
