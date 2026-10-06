/**
 * 终端配色方案表。
 *
 * 这些是**终端主题数据**（xterm 的 ANSI 调色板），不是界面主题色，
 * 所以这里写死十六进制；界面本身的颜色仍然全部走 CSS 变量。
 */

import type { ITheme } from "@xterm/xterm";

/** 配色方案 ID → xterm 主题 */
export const TERMINAL_THEMES: Record<string, ITheme> = {
  /** CmdDeck 默认暗色：深灰底 + 青绿强调 */
  "cmddeck-dark": {
    background: "#0d1117",
    foreground: "#d7dce3",
    cursor: "#2dd4bf",
    cursorAccent: "#0d1117",
    selectionBackground: "rgba(45, 212, 191, 0.28)",
    black: "#1b2027",
    red: "#f85149",
    green: "#3fb950",
    yellow: "#d29922",
    blue: "#58a6ff",
    magenta: "#bc8cff",
    cyan: "#39c5cf",
    white: "#b1bac4",
    brightBlack: "#6e7b8a",
    brightRed: "#ff7b72",
    brightGreen: "#56d364",
    brightYellow: "#e3b341",
    brightBlue: "#79c0ff",
    brightMagenta: "#d2a8ff",
    brightCyan: "#56d4dd",
    brightWhite: "#f0f6fc",
  },

  /** 亮色方案：白底，适合亮环境下贴代码 */
  "cmddeck-light": {
    background: "#ffffff",
    foreground: "#24292f",
    cursor: "#0f766e",
    cursorAccent: "#ffffff",
    selectionBackground: "rgba(15, 118, 110, 0.22)",
    black: "#24292f",
    red: "#cf222e",
    green: "#116329",
    yellow: "#4d2d00",
    blue: "#0969da",
    magenta: "#8250df",
    cyan: "#1b7c83",
    white: "#6e7781",
    brightBlack: "#57606a",
    brightRed: "#a40e26",
    brightGreen: "#1a7f37",
    brightYellow: "#633c01",
    brightBlue: "#218bff",
    brightMagenta: "#a475f9",
    brightCyan: "#3192aa",
    brightWhite: "#8c959f",
  },

  /** Solarized Dark：低对比护眼，长时间盯着不累 */
  "solarized-dark": {
    background: "#002b36",
    foreground: "#93a1a1",
    cursor: "#93a1a1",
    cursorAccent: "#002b36",
    selectionBackground: "rgba(38, 139, 210, 0.35)",
    black: "#073642",
    red: "#dc322f",
    green: "#859900",
    yellow: "#b58900",
    blue: "#268bd2",
    magenta: "#d33682",
    cyan: "#2aa198",
    white: "#eee8d5",
    brightBlack: "#586e75",
    brightRed: "#cb4b16",
    brightGreen: "#586e75",
    brightYellow: "#657b83",
    brightBlue: "#839496",
    brightMagenta: "#6c71c4",
    brightCyan: "#93a1a1",
    brightWhite: "#fdf6e3",
  },

  /** Matrix 绿：纯黑底荧光绿，老电影里的那种 */
  "matrix-green": {
    background: "#000000",
    foreground: "#33ff33",
    cursor: "#00ff00",
    cursorAccent: "#000000",
    selectionBackground: "rgba(0, 255, 0, 0.28)",
    black: "#0a3d0a",
    red: "#ff3b30",
    green: "#00ff41",
    yellow: "#c8ff00",
    blue: "#00b3ff",
    magenta: "#ff00ff",
    cyan: "#00ffcc",
    white: "#b8ffb8",
    brightBlack: "#1f7a1f",
    brightRed: "#ff6b6b",
    brightGreen: "#7CFC00",
    brightYellow: "#e8ff8a",
    brightBlue: "#66d9ff",
    brightMagenta: "#ff8aff",
    brightCyan: "#8affd6",
    brightWhite: "#eaffea",
  },
};

/** 配色方案下拉框用的选项（含中文说明） */
export const COLOR_SCHEME_OPTIONS: { label: string; value: string }[] = [
  { label: "CmdDeck 暗色（默认）", value: "cmddeck-dark" },
  { label: "CmdDeck 亮色", value: "cmddeck-light" },
  { label: "Solarized 暗色", value: "solarized-dark" },
  { label: "Matrix 荧光绿", value: "matrix-green" },
];

/** 按 ID 取配色方案，取不到时回退到默认暗色 */
export function resolveTerminalTheme(scheme: string): ITheme {
  return TERMINAL_THEMES[scheme] ?? TERMINAL_THEMES["cmddeck-dark"];
}

/** 搜索高亮配色（addon-search 的 decorations 参数必须是 #RRGGBB，不能用 rgba） */
export interface SearchDecoration {
  matchBackground: string;
  matchBorder: string;
  activeMatchBackground: string;
  activeMatchBorder: string;
}

/** 各配色方案对应的搜索高亮配色 */
export const SEARCH_DECORATIONS: Record<string, SearchDecoration> = {
  "cmddeck-dark": {
    matchBackground: "#8a6d1f",
    matchBorder: "#d29922",
    activeMatchBackground: "#d29922",
    activeMatchBorder: "#f0c674",
  },
  "cmddeck-light": {
    matchBackground: "#fff2a8",
    matchBorder: "#d4a72c",
    activeMatchBackground: "#e3b341",
    activeMatchBorder: "#9a6700",
  },
  "solarized-dark": {
    matchBackground: "#586e75",
    matchBorder: "#93a1a1",
    activeMatchBackground: "#b58900",
    activeMatchBorder: "#ffd966",
  },
  "matrix-green": {
    matchBackground: "#0d5c0d",
    matchBorder: "#00ff41",
    activeMatchBackground: "#00ff41",
    activeMatchBorder: "#b8ffb8",
  },
};

/** 取搜索高亮配色 */
export function resolveSearchDecoration(scheme: string): SearchDecoration {
  return SEARCH_DECORATIONS[scheme] ?? SEARCH_DECORATIONS["cmddeck-dark"];
}
