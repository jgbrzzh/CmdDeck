/**
 * 时间、数字等格式化工具。
 *
 * 全站统一使用「本地时间 + 24 小时制」，日期格式固定为 `YYYY-MM-DD`。
 * 所有函数在入参非法时都返回一个可显示的兜底字符串，不抛异常。
 */

function pad2(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}

/** 非法时间戳的兜底显示 */
const INVALID = "—";

/** 判断是否是合法的 Unix 毫秒时间戳 */
function isValidMs(ms: number): boolean {
  return Number.isFinite(ms) && ms > 0 && ms < 8.64e15;
}

/**
 * 把 Unix 毫秒格式化成 `YYYY-MM-DD HH:mm:ss`。
 */
export function formatDateTime(ms: number): string {
  if (!isValidMs(ms)) return INVALID;
  const d = new Date(ms);
  return (
    `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ` +
    `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`
  );
}

/**
 * 把 Unix 毫秒格式化成 `YYYY-MM-DD HH:mm`。
 */
export function formatDateMinute(ms: number): string {
  if (!isValidMs(ms)) return INVALID;
  const d = new Date(ms);
  return (
    `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ` +
    `${pad2(d.getHours())}:${pad2(d.getMinutes())}`
  );
}

/**
 * 智能时间显示 —— 按「距离现在的远近」自动选择粒度：
 *
 * - < 60 秒            → `刚刚`
 * - < 60 分钟          → `N 分钟前`
 * - < 24 小时          → `N 小时前`
 * - 同一年            → `MM-dd HH:mm`
 * - 跨年              → `yyyy-MM-dd HH:mm`
 *
 * 传入未来时间时返回 `HH:mm`（定时任务的「下次运行」常用）。
 */
export function formatTime(ms: number): string {
  if (!isValidMs(ms)) return INVALID;

  const now = Date.now();
  const diff = now - ms;

  // 未来时间：超过 24 小时的按日期显示，否则只显示时分
  if (diff < 0) {
    return -diff < 86400000 ? formatHm(ms) : formatDateMinute(ms);
  }

  if (diff < 60000) return "刚刚";
  if (diff < 3600000) return `${Math.floor(diff / 60000)} 分钟前`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)} 小时前`;

  const d = new Date(ms);
  if (d.getFullYear() === new Date(now).getFullYear()) return formatMdHm(ms);
  return formatDateMinute(ms);
}

/** 只取 `HH:mm` */
export function formatHm(ms: number): string {
  if (!isValidMs(ms)) return INVALID;
  const d = new Date(ms);
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}

/** 只取 `MM-dd` */
export function formatMd(ms: number): string {
  if (!isValidMs(ms)) return INVALID;
  const d = new Date(ms);
  return `${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

/** `MM-dd HH:mm` */
export function formatMdHm(ms: number): string {
  if (!isValidMs(ms)) return INVALID;
  return `${formatMd(ms)} ${formatHm(ms)}`;
}

/**
 * 相对时间的中文描述，粒度更细，用于审计日志列表。
 *
 * - 未来：`3 分钟后` / `2 小时后` / `3 天后`
 * - 过去：`刚刚` / `5 分钟前` / `2 天前`
 */
export function formatRelative(ms: number): string {
  if (!isValidMs(ms)) return INVALID;
  const diff = Date.now() - ms;
  const abs = Math.abs(diff);
  const suffix = diff >= 0 ? "前" : "后";

  if (abs < 60000) return "刚刚";
  if (abs < 3600000) return `${Math.floor(abs / 60000)} 分钟${suffix}`;
  if (abs < 86400000) return `${Math.floor(abs / 3600000)} 小时${suffix}`;
  if (abs < 2592000000) return `${Math.floor(abs / 86400000)} 天${suffix}`;
  if (abs < 31536000000) return `${Math.floor(abs / 2592000000)} 个月${suffix}`;
  return `${Math.floor(abs / 31536000000)} 年${suffix}`;
}

/**
 * 时长格式化。用于「运行时长」「已运行 00:01:23」这类展示。
 *
 * - < 1 秒        → `820 毫秒`
 * - < 60 秒       → `12.3 秒`
 * - < 1 小时      → `03:07`（分:秒）
 * - >= 1 小时     → `02:03:07`（时:分:秒）
 * - >= 1 天       → 追加 `3 天 02:03:07`
 */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "—";
  if (ms < 1000) return `${Math.round(ms)} 毫秒`;

  const totalSeconds = Math.floor(ms / 1000);
  const days = Math.floor(totalSeconds / 86400);
  const hours = Math.floor((totalSeconds % 86400) / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (days > 0)
    return `${days} 天 ${pad2(hours)}:${pad2(minutes)}:${pad2(seconds)}`;
  if (totalSeconds < 3600) return `${pad2(minutes)}:${pad2(seconds)}`;
  return `${pad2(hours)}:${pad2(minutes)}:${pad2(seconds)}`;
}

/** 带毫秒的计时器文本，用于终端标签页的运行时长 */
export function formatClock(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "00:00:00";
  const total = Math.floor(ms / 1000);
  return `${pad2(Math.floor(total / 3600))}:${pad2(Math.floor((total % 3600) / 60))}:${pad2(total % 60)}`;
}

/**
 * 字节数格式化（1024 进制）。
 *
 * ```
 * 0        → 0 B
 * 512      → 512 B
 * 2048     → 2.0 KB
 * 1048576  → 1.0 MB
 * ```
 */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "—";
  if (n < 1024) return `${Math.round(n)} B`;

  const units = ["KB", "MB", "GB", "TB", "PB"];
  let value = n / 1024;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[idx]}`;
}

/** 千分位数字，如 `1234567` → `1,234,567` */
export function formatNumber(n: number, fractionDigits = 0): string {
  if (!Number.isFinite(n)) return "—";
  return n.toLocaleString("zh-CN", {
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  });
}

/** 百分比，如 `0.1234` → `12.3%`；传 0~1 之间的小数 */
export function formatPercent(ratio: number, digits = 1): string {
  if (!Number.isFinite(ratio)) return "—";
  return `${(ratio * 100).toFixed(digits)}%`;
}

/**
 * 退出码转中文说明。终端标签页底部状态栏直接显示它。
 *
 * - 0        → `退出码 0（成功）`
 * - 非 0     → `退出码 N（失败）`
 * - null     → `尚未退出`
 * - Windows 上约定：0xC000013A = 0x800000FC 附近的强制终止也算「已终止」
 */
export function formatExitCode(code: number | null, status?: string): string {
  if (status === "killed") return "已终止";
  if (code === null || code === undefined) return "尚未退出";
  if (code === 0) return "退出码 0（成功）";
  // 常见的 0xC0000135（缺少 DLL）等高位退出码转成可读形式
  const unsigned = code >>> 0;
  if (unsigned >= 0xc0000000 && unsigned <= 0xc000ffff) {
    return `退出码 ${code}（异常终止 0x${unsigned.toString(16).toUpperCase()}）`;
  }
  return `退出码 ${code}（失败）`;
}

/** 路径中间省略，保留首尾两段，用于窄栏位显示长路径 */
export function ellipsisPath(path: string, maxLen = 42): string {
  if (!path) return "";
  if (path.length <= maxLen) return path;
  const keep = Math.floor((maxLen - 3) / 2);
  if (keep < 4) return `…${path.slice(-(maxLen - 1))}`;
  return `${path.slice(0, keep)}…${path.slice(-keep)}`;
}
