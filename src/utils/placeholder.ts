/**
 * 占位符工具。
 *
 * 预设的命令里可以写 `{{name}}`、`{{路径}}`、`{{port-1}}` 这类占位符，
 * 运行前由弹窗收集参数，再在这里替换成真实值。
 *
 * 统一正则：`\{\{\s*([\w\u4e00-\u9fa5\-\.]+)\s*\}\}`
 * - `\w` 覆盖 a-z / A-Z / 0-9 / _
 * - `\u4e00-\u9fa5` 覆盖常用中文
 * - 额外允许 `-` 和 `.`，方便 `{{log-dir}}`、`{{v1.2}}` 这种写法
 */

/** 捕获组：单个占位符键名 */
const PLACEHOLDER_GROUP = "[\\w\\u4e00-\\u9fa5\\-\\.]+";

/** 带 g 标志的全局正则（模块级共享，用完必须重置 lastIndex） */
const PH_GLOBAL = new RegExp(`\\{\\{\\s*(${PLACEHOLDER_GROUP})\\s*\\}\\}`, "g");

/** 非全局版本，用于 hasPlaceholder 这类只要布尔值的场景 */
const PH_TEST = new RegExp(`\\{\\{\\s*${PLACEHOLDER_GROUP}\\s*\\}\\}`);

/** `${key}` 单花括号形式 */
const DOLLAR_GLOBAL = /\$\{\s*([\w\u4e00-\u9fa5\-.]+)\s*\}/g;

/**
 * 抽取文本中全部占位符键名。
 *
 * @param text 命令文本（程序与参数可以拼在一起传）
 * @returns 去重后的键名列表，**保持首次出现的顺序**
 */
export function extractPlaceholders(text: string): string[] {
  if (!text) return [];
  const out: string[] = [];
  const seen = new Set<string>();
  PH_GLOBAL.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = PH_GLOBAL.exec(text)) !== null) {
    const key = m[1];
    if (key && !seen.has(key)) {
      seen.add(key);
      out.push(key);
    }
    // 理论上不会零长度匹配，这里防御一下防止死循环
    if (m.index === PH_GLOBAL.lastIndex) PH_GLOBAL.lastIndex += 1;
  }
  return out;
}

/**
 * 用给定参数替换占位符。
 *
 * 规则：
 * - `args` 中存在该键则替换为对应值
 * - 没提供值的键**原样保留** `{{key}}`，让用户一眼看出还缺哪个参数
 * - 同时支持 `${key}` 单花括号写法
 *
 * @param text 原始命令文本
 * @param args 参数字典
 */
export function applyPlaceholders(
  text: string,
  args: Record<string, string>,
): string {
  if (!text) return "";
  const map: Record<string, string> = {};
  for (const [k, v] of Object.entries(args ?? {})) {
    if (typeof v === "string") map[k] = v;
  }

  // 双花括号
  let out = text.replace(PH_GLOBAL, (whole, rawKey: string) => {
    const key = rawKey.trim();
    return Object.prototype.hasOwnProperty.call(map, key) ? map[key] : whole;
  });

  // 单花括号
  DOLLAR_GLOBAL.lastIndex = 0;
  out = out.replace(DOLLAR_GLOBAL, (whole, rawKey: string) => {
    const key = rawKey.trim();
    return Object.prototype.hasOwnProperty.call(map, key) ? map[key] : whole;
  });

  return out;
}

/** 文本中是否存在 `{{key}}` 占位符 */
export function hasPlaceholder(text: string): boolean {
  if (!text) return false;
  return PH_TEST.test(text);
}

/** 前端推断的输入控件类型 */
export type GuessInputType =
  "text" | "number" | "path" | "folder" | "file" | "select" | "password";

/** 常见英文键名 → 中文标签的兜底映射，让运行参数弹窗更像中文软件 */
const LABEL_DICT: Record<string, string> = {
  name: "名称",
  user: "用户名",
  username: "用户名",
  host: "主机",
  port: "端口",
  ip: "IP 地址",
  path: "路径",
  dir: "目录",
  folder: "目录",
  file: "文件",
  url: "地址",
  branch: "分支",
  env: "环境",
  profile: "配置档",
  message: "备注信息",
  msg: "消息",
  count: "数量",
  timeout: "超时时间",
  service: "服务名",
  database: "数据库",
  version: "版本号",
  tag: "标签",
  key: "键",
  value: "值",
};

/**
 * 根据键名推断出「中文标签 + 合适的输入控件类型」。
 *
 * 运行参数弹窗用它自动生成表单，用户不必手动为每个 `{{xxx}}` 调控件。
 */
export function guessPlaceholderMeta(key: string): {
  label: string;
  inputType: GuessInputType;
} {
  const k = key.toLowerCase();

  // 端口 / 数量 / 序号 / 耗时
  if (
    /^(port|idx|index|no|num|number|count|size|limit|offset|timeout|ms|secs|seconds)$/.test(
      k,
    )
  ) {
    return { label: LABEL_DICT[k] ?? key, inputType: "number" };
  }
  // 目录类
  if (/(^|_)(dir|dirs|path|folder|directory|workspace)$/.test(k)) {
    return { label: LABEL_DICT[k] ?? key, inputType: "folder" };
  }
  // 文件类
  if (/(^|_)(file|filename|log|logfile|exe|bin|script)$/.test(k)) {
    return { label: LABEL_DICT[k] ?? key, inputType: "file" };
  }
  // 密码类
  if (/(pass|pwd|secret|token)$/.test(k) || /key$/.test(k)) {
    return { label: LABEL_DICT[k] ?? key, inputType: "password" };
  }
  // 环境选择
  if (/env$/.test(k) || /profile$/.test(k)) {
    return { label: LABEL_DICT[k] ?? key, inputType: "select" };
  }
  return { label: LABEL_DICT[k] ?? key, inputType: "text" };
}

/**
 * 从「程序 + 参数数组」里抽取全部占位符键名。
 * 编辑面板里程序与参数是分栏存储的，需要一个合并入口。
 */
export function extractFromParts(program: string, args: string[]): string[] {
  return extractPlaceholders([program ?? "", ...(args ?? [])].join(" "));
}

/**
 * 把 `{{key}}` 渲染成更易读的 `<key>` 单行形式，用于列表摘要。
 * 同时压掉换行与多余空白，保证一行显示。
 */
export function toReadableForm(text: string): string {
  if (!text) return "";
  return text
    .replace(/\r?\n/g, " ")
    .replace(
      new RegExp(`\\{\\{\\s*(${PLACEHOLDER_GROUP})\\s*\\}\\}`, "g"),
      "<$1>",
    )
    .replace(/\s{2,}/g, " ")
    .trim();
}
