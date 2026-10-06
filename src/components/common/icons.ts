/**
 * 内联 SVG 图标数据表。
 *
 * 为什么单独抽一个 .ts：
 * - `Icon.vue` 用 `<script setup>` 没法导出常量，而预设编辑器的「图标选择器」
 *   需要拿到完整图标名清单；
 * - env.d.ts 把 `*.vue` 声明成只有 default 导出，具名导入会类型报错。
 *
 * 全部图标统一 24×24 视框、`fill="none"` + `stroke="currentColor"`，
 * 描边圆角交给外层 svg 的 stroke-linecap / stroke-linejoin。
 */

/** 图标名 → SVG 内部片段（不含最外层 <svg>） */
export const ICON_PATHS: Record<string, string> = {
  terminal:
    '<path d="M3.5 5.5h17a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1h-17a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1z"/><path d="M7 9.5l3 2.5-3 2.5"/><path d="M12.5 15h4.5"/>',
  folder:
    '<path d="M3 7.5A1.5 1.5 0 0 1 4.5 6h4.2a1.5 1.5 0 0 1 1.06.44L11.4 8h8.1A1.5 1.5 0 0 1 21 9.5v8A1.5 1.5 0 0 1 19.5 19h-15A1.5 1.5 0 0 1 3 17.5z"/>',
  "folder-open":
    '<path d="M3.2 10V7.5A1.5 1.5 0 0 1 4.7 6h4.2a1.5 1.5 0 0 1 1.06.44L11.4 8H19a1.5 1.5 0 0 1 1.47 1.76l-.3 1.24"/><path d="M3.3 10h17.2a1 1 0 0 1 .97 1.24l-1.63 5.7A2 2 0 0 1 17.9 18.5H4.6a1.6 1.6 0 0 1-1.56-2z"/>',
  "folder-plus":
    '<path d="M3 7.5A1.5 1.5 0 0 1 4.5 6h4.2a1.5 1.5 0 0 1 1.06.44L11.4 8h8.1A1.5 1.5 0 0 1 21 9.5v8A1.5 1.5 0 0 1 19.5 19h-15A1.5 1.5 0 0 1 3 17.5z"/><path d="M12 11.2v5.2M9.4 13.8h5.2"/>',
  inbox:
    '<path d="M3.5 12.5h4l1.3 2.6h6.4l1.3-2.6h4"/><path d="M5.7 5.5h12.6a1 1 0 0 1 .95 1.32l2.05 6.2a1 1 0 0 1 .05.28V18a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3.5 18v-4.7a1 1 0 0 1 .05-.28l2.05-6.2a1 1 0 0 1 .95-1.32z"/>',
  search: '<circle cx="11" cy="11" r="6.5"/><path d="M15.8 15.8L20 20"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  edit: '<path d="M4 20.2h4.2L19.3 9.1a2.12 2.12 0 0 0-3-3L5.2 17.2z"/><path d="M14.6 6.4l3 3"/>',
  trash:
    '<path d="M3.8 6.6h16.4"/><path d="M9 6.6V4.4h6v2.2"/><path d="M5.9 6.6l.94 12.42A1.6 1.6 0 0 0 8.44 20.5h7.12a1.6 1.6 0 0 0 1.6-1.48L18.1 6.6"/><path d="M10.2 10.4v6.4M13.8 10.4v6.4"/>',
  copy: '<rect x="8.6" y="8.6" width="11.4" height="11.4" rx="2"/><path d="M5.4 15.4h-.9a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1h9.9a1 1 0 0 1 1 1v.9"/>',
  star: '<path d="M12 3.6l2.53 5.12 5.65.82-4.09 3.99.97 5.63L12 16.52l-5.06 2.64.97-5.63-4.09-3.99 5.65-.82z"/>',
  "star-filled":
    '<path d="M12 3.6l2.53 5.12 5.65.82-4.09 3.99.97 5.63L12 16.52l-5.06 2.64.97-5.63-4.09-3.99 5.65-.82z" fill="currentColor"/>',
  play: '<path d="M7.8 5.4l10.6 6.6-10.6 6.6z"/>',
  "play-circle":
    '<circle cx="12" cy="12" r="8.6"/><path d="M10 8.4l6 3.6-6 3.6z"/>',
  stop: '<rect x="6.2" y="6.2" width="11.6" height="11.6" rx="1.6"/>',
  square: '<rect x="4.4" y="4.4" width="15.2" height="15.2" rx="2"/>',
  x: '<path d="M6.2 6.2l11.6 11.6M17.8 6.2L6.2 17.8"/>',
  check: '<path d="M4.8 12.6l4.6 4.6L19.2 6.8"/>',
  "chevron-down": '<path d="M6 9.5l6 6 6-6"/>',
  "chevron-right": '<path d="M9.5 6l6 6-6 6"/>',
  "chevron-left": '<path d="M14.5 6l-6 6 6 6"/>',
  "chevron-up": '<path d="M6 14.5l6-6 6 6"/>',
  "arrow-right": '<path d="M4.5 12h15"/><path d="M13.4 6l6 6-6 6"/>',
  settings:
    '<circle cx="12" cy="12" r="2.9"/><path d="M12 3.2v2.4M12 18.4v2.4M20.8 12h-2.4M5.6 12H3.2M18.2 5.8l-1.7 1.7M7.5 16.5l-1.7 1.7M18.2 18.2l-1.7-1.7M7.5 7.5L5.8 5.8"/>',
  sun: '<circle cx="12" cy="12" r="4.1"/><path d="M12 2.6v2.2M12 19.2v2.2M21.4 12h-2.2M4.8 12H2.6M18.6 5.4l-1.6 1.6M7 17l-1.6 1.6M18.6 18.6L17 17M7 7L5.4 5.4"/>',
  moon: '<path d="M20.2 14.6A8.6 8.6 0 0 1 9.4 3.8a8.6 8.6 0 1 0 10.8 10.8z"/>',
  list: '<path d="M8.5 6.2H20M8.5 12H20M8.5 17.8H20"/><path d="M4.4 6.2h.02M4.4 12h.02M4.4 17.8h.02" stroke-width="2.4"/>',
  grid: '<rect x="3.4" y="3.4" width="7.2" height="7.2" rx="1.6"/><rect x="13.4" y="3.4" width="7.2" height="7.2" rx="1.6"/><rect x="3.4" y="13.4" width="7.2" height="7.2" rx="1.6"/><rect x="13.4" y="13.4" width="7.2" height="7.2" rx="1.6"/>',
  clock: '<circle cx="12" cy="12" r="8.6"/><path d="M12 7.2V12l3.2 2"/>',
  calendar:
    '<rect x="3.4" y="5" width="17.2" height="15.6" rx="2"/><path d="M3.4 10h17.2M8 3.2v3.6M16 3.2v3.6"/>',
  workflow:
    '<rect x="2.8" y="3.4" width="6.4" height="5" rx="1.5"/><rect x="14.8" y="15.6" width="6.4" height="5" rx="1.5"/><path d="M6 8.4v6.4a3 3 0 0 0 3 3h5.8"/>',
  shield:
    '<path d="M12 3l7.2 2.9v5.5c0 4.3-2.9 8.1-7.2 9.6-4.3-1.5-7.2-5.3-7.2-9.6V5.9z"/>',
  "alert-triangle":
    '<path d="M10.6 4.4L2.9 17.4A1.6 1.6 0 0 0 4.3 20h15.4a1.6 1.6 0 0 0 1.4-2.6L13.4 4.4a1.6 1.6 0 0 0-2.8 0z"/><path d="M12 9.4v4.2M12 16.8h.02"/>',
  "alert-circle":
    '<circle cx="12" cy="12" r="8.6"/><path d="M12 7.4v5.2M12 16.2h.02"/>',
  info: '<circle cx="12" cy="12" r="8.6"/><path d="M12 16.6V11M12 7.6h.02"/>',
  "check-circle":
    '<circle cx="12" cy="12" r="8.6"/><path d="M8 12.3l2.8 2.8L16.2 9.4"/>',
  refresh:
    '<path d="M20.2 12a8.2 8.2 0 1 1-2.6-6"/><path d="M20.4 3.8v4.6h-4.6"/>',
  download:
    '<path d="M12 4v10.2"/><path d="M7.8 10.8L12 15l4.2-4.2"/><path d="M4.4 17.2v1.2A1.6 1.6 0 0 0 6 20h12a1.6 1.6 0 0 0 1.6-1.6v-1.2"/>',
  upload:
    '<path d="M12 15V4.8"/><path d="M7.8 9L12 4.8 16.2 9"/><path d="M4.4 17.2v1.2A1.6 1.6 0 0 0 6 20h12a1.6 1.6 0 0 0 1.6-1.6v-1.2"/>',
  "more-horizontal":
    '<path d="M5.6 12h.02M12 12h.02M18.4 12h.02" stroke-width="2.6"/>',
  "dots-vertical":
    '<path d="M12 5.6h.02M12 12h.02M12 18.4h.02" stroke-width="2.6"/>',
  "grip-vertical":
    '<path d="M9 6h.02M15 6h.02M9 12h.02M15 12h.02M9 18h.02M15 18h.02" stroke-width="2.6"/>',
  pin: '<path d="M9.2 3.4h5.6l-.8 5.2 3.2 3.2H6.8l3.2-3.2z"/><path d="M12 11.8v8.8"/>',
  "pin-filled":
    '<path d="M9.2 3.4h5.6l-.8 5.2 3.2 3.2H6.8l3.2-3.2z" fill="currentColor"/><path d="M12 11.8v8.8"/>',
  filter: '<path d="M3.4 5.4h17.2l-6.7 7.8V19.2l-3.8 1.4v-7.4z"/>',
  "panel-left":
    '<rect x="3.4" y="4.4" width="17.2" height="15.2" rx="2"/><path d="M9.4 4.4v15.2"/>',
  "external-link":
    '<path d="M14 3.8h6.2V10"/><path d="M20.2 3.8L11.6 12.4"/><path d="M18 14.4V18a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h3.6"/>',
  history:
    '<path d="M3.4 9.4A8.6 8.6 0 1 1 3 13.8"/><path d="M3.2 4.4v5.2h5.2"/><path d="M12 7.6V12l3.2 2"/>',
  layers:
    '<path d="M12 3.2l8.6 4.3-8.6 4.3-8.6-4.3z"/><path d="M3.4 12l8.6 4.3L20.6 12"/><path d="M3.4 16.2l8.6 4.3 8.6-4.3"/>',
  zap: '<path d="M13.6 2.4L4.4 13.6h6.2l-.6 8 9.2-11.2h-6.2z"/>',
  globe:
    '<circle cx="12" cy="12" r="8.6"/><path d="M3.4 12h17.2"/><path d="M12 3.4a13.4 13.4 0 0 1 0 17.2 13.4 13.4 0 0 1 0-17.2z"/>',
  code: '<path d="M9 7.2L3.8 12 9 16.8"/><path d="M15 7.2L20.2 12 15 16.8"/>',
  database:
    '<ellipse cx="12" cy="5.8" rx="7.6" ry="3"/><path d="M4.4 5.8v6.1c0 1.66 3.4 3 7.6 3s7.6-1.34 7.6-3V5.8"/><path d="M4.4 11.9v6.3c0 1.66 3.4 3 7.6 3s7.6-1.34 7.6-3v-6.3"/>',
  package:
    '<path d="M20.6 8.2v7.6a1.5 1.5 0 0 1-.79 1.3l-6.94 3.8a1.5 1.5 0 0 1-1.54 0l-6.94-3.8a1.5 1.5 0 0 1-.79-1.3V8.2a1.5 1.5 0 0 1 .79-1.3l6.94-3.8a1.5 1.5 0 0 1 1.54 0l6.94 3.8a1.5 1.5 0 0 1 .79 1.3z"/><path d="M3.6 7.4L12 12.2l8.4-4.8"/><path d="M12 12.2v8.6"/>',
  cpu: '<rect x="6.4" y="6.4" width="11.2" height="11.2" rx="1.6"/><rect x="9.9" y="9.9" width="4.2" height="4.2" rx="1"/><path d="M9.4 3.4v3M14.6 3.4v3M9.4 17.6v3M14.6 17.6v3M3.4 9.4h3M3.4 14.6h3M17.6 9.4h3M17.6 14.6h3"/>',
  "hard-drive":
    '<rect x="2.8" y="4.8" width="18.4" height="6" rx="2"/><rect x="2.8" y="13.2" width="18.4" height="6" rx="2"/><path d="M6.6 7.8h.02M6.6 16.2h.02" stroke-width="2.4"/>',
  rocket:
    '<path d="M12 2.8c3.6 1.9 5.6 5.3 5.6 9.3L12 17.4l-5.6-5.3c0-4 2-7.4 5.6-9.3z"/><circle cx="12" cy="10" r="1.9"/><path d="M8.1 16.4c-1.3.7-2.1 2.1-2.3 3.7 1.7-.3 3.1-1 3.8-2.1"/><path d="M15.9 16.4c1.3.7 2.1 2.1 2.3 3.7-1.7-.3-3.1-1-3.8-2.1"/>',
  bookmark:
    '<path d="M6.4 3.4h11.2a1 1 0 0 1 1 1V21l-6.6-4.2L5.4 21V4.4a1 1 0 0 1 1-1z"/>',
  save: '<path d="M5 3.4h10.8L20.6 8.2V20a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V4.4a1 1 0 0 1 1-1z"/><path d="M8 3.4v6h7"/><path d="M8 21v-6h8v6"/>',
  power: '<path d="M12 3.4v7.8"/><path d="M17.3 6.4a7.6 7.6 0 1 1-10.6 0"/>',
  "maximize-2":
    '<path d="M4.4 9.4V4.4a1 1 0 0 1 1-1h5"/><path d="M13.6 3.4h5a1 1 0 0 1 1 1v5"/><path d="M19.6 14.6v5a1 1 0 0 1-1 1h-5"/><path d="M10.4 20.6h-5a1 1 0 0 1-1-1v-5"/>',
  "minimize-2":
    '<path d="M4.4 14.6h4.6a1 1 0 0 1 1 1v4.4"/><path d="M10 3.4V8a1 1 0 0 1-1 1H4.4"/><path d="M14.4 10h4.6a1 1 0 0 1 1 1v4.6"/><path d="M20 9.4V4.4a1 1 0 0 0-1-1h-4.6"/>',
  eye: '<path d="M2.4 12S6 5.4 12 5.4 21.6 12 21.6 12 18 18.6 12 18.6 2.4 12 2.4 12z"/><circle cx="12" cy="12" r="3.1"/>',
  "eye-off":
    '<path d="M3.8 3.8l16.4 16.4"/><path d="M9.9 5.9A9.6 9.6 0 0 1 12 5.4c6 0 9.6 6.6 9.6 6.6a17.4 17.4 0 0 1-3.4 4.2"/><path d="M6.3 7.8A17.2 17.2 0 0 0 2.4 12S6 18.6 12 18.6c1 0 1.9-.1 2.7-.4"/><path d="M9.9 9.9a3.1 3.1 0 0 0 4.2 4.2"/>',
  keyboard:
    '<rect x="2.4" y="5.8" width="19.2" height="12.4" rx="2"/><path d="M6.4 9.4h.02M9.8 9.4h.02M13.2 9.4h.02M16.6 9.4h.02M6.4 12.4h.02M9.8 12.4h.02M13.2 12.4h.02M16.6 12.4h.02M8 15.4h8" stroke-width="1.9"/>',
  bug: '<rect x="8" y="7" width="8" height="12.6" rx="4"/><path d="M8 11.4H4.4M8 15.2H4.4M16 11.4h3.6M16 15.2h3.6M9.4 7.2L8 4.4M14.6 7.2L16 4.4M12 7V3.6"/>',
  flame:
    '<path d="M12 20.8c3.6 0 6.4-2.6 6.4-6 0-4.5-4.5-6.5-4-12-3 1.5-5 4-5 6.5 0 1-1 1.5-1.6.6-.4-.6-.4-1.4-.3-2.1-1.8 2-2 4.5-2 7 0 3.4 2.9 6 6.5 6z"/>',
  snowflake:
    '<path d="M12 2.6v18.8M3.8 7.3l16.4 9.4M20.2 7.3L3.8 16.7"/><path d="M9.4 5.2L12 7.8l2.6-2.6M9.4 18.8l2.6-2.6 2.6 2.6"/>',
  "trending-up":
    '<path d="M3.4 16.8l6-6 4 4 7.2-7.2"/><path d="M14.8 7.6h5.8v5.8"/>',
  link: '<path d="M10.2 13.8a4.1 4.1 0 0 0 5.8 0l2.6-2.6a4.1 4.1 0 0 0-5.8-5.8L11.6 6.6"/><path d="M13.8 10.2a4.1 4.1 0 0 0-5.8 0l-2.6 2.6a4.1 4.1 0 0 0 5.8 5.8l1.2-1.2"/>',
  "file-text":
    '<path d="M6 3.4h7.4L19 9v11.6a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4.4a1 1 0 0 1 1-1z"/><path d="M13 3.4V9h6"/><path d="M8.6 13h7M8.6 16.4h7"/>',
  "sort-asc":
    '<path d="M4.4 6.6h9.2M4.4 12h6.2M4.4 17.4h3.2"/><path d="M17 5.4v12"/><path d="M13.8 14.4L17 17.6l3.2-3.2"/>',

  // ---- 预设编辑器专用的一组「业务语义」图标 ----
  "git-branch":
    '<circle cx="6.6" cy="5.2" r="2.4"/><circle cx="6.6" cy="18.8" r="2.4"/><circle cx="17.4" cy="12" r="2.4"/><path d="M6.6 7.6v8.8"/><path d="M17.4 14.4v.6a4 4 0 0 1-4 4H9"/>',
  server:
    '<rect x="3.2" y="4" width="17.6" height="6.4" rx="1.8"/><rect x="3.2" y="13.6" width="17.6" height="6.4" rx="1.8"/><path d="M6.8 7.2h.02M6.8 16.8h.02" stroke-width="2.4"/>',
  wrench:
    '<path d="M15.2 3.4a5.4 5.4 0 0 0-5 7.4L4 17l3 3 6.2-6.2a5.4 5.4 0 0 0 6.8-6.9l-3.1 3.1-3-.6-.6-3 3.1-3.1a5.4 5.4 0 0 0-1.2.1z"/>',
  gear: '<circle cx="12" cy="12" r="3.2"/><path d="M12 2.8l1.1 2.3 2.5-.5 1 2.4 2.4 1-.5 2.5 1.8 1.9-1.8 1.9.5 2.5-2.4 1-1 2.4-2.5-.5L12 21.2l-1.1-2.3-2.5.5-1-2.4-2.4-1 .5-2.5L3.7 12l1.8-1.9-.5-2.5 2.4-1 1-2.4 2.5.5z"/>',
  chart:
    '<path d="M4 20V4"/><path d="M4 20h16"/><rect x="7.4" y="12" width="3" height="5" rx="1"/><rect x="12.4" y="8.4" width="3" height="8.6" rx="1"/><rect x="17.4" y="10.4" width="3" height="6.6" rx="1"/>',
  brush:
    '<path d="M17.4 3.6l3 3-8.2 8.2-3-3z"/><path d="M9.2 11.8L6.6 14.4a3.2 3.2 0 0 0-.9 2.2v1.8H4a1.6 1.6 0 0 1 0-3.2h1.8a3.2 3.2 0 0 0 2.2-.9z"/>',
  box: '<path d="M20.6 8.2v7.6a1.5 1.5 0 0 1-.79 1.3l-6.94 3.8a1.5 1.5 0 0 1-1.54 0l-6.94-3.8a1.5 1.5 0 0 1-.79-1.3V8.2a1.5 1.5 0 0 1 .79-1.3l6.94-3.8a1.5 1.5 0 0 1 1.54 0l6.94 3.8a1.5 1.5 0 0 1 .79 1.3z"/><path d="M3.6 7.4L12 12.2l8.4-4.8"/><path d="M12 12.2v8.6"/>',
  "box-open":
    '<path d="M3.4 9.2L5 4.6a1.5 1.5 0 0 1 1.4-1h11.2a1.5 1.5 0 0 1 1.4 1l1.6 4.6"/><path d="M3.4 9.2h17.2l-1.3 9.4a1.6 1.6 0 0 1-1.6 1.3H6.3a1.6 1.6 0 0 1-1.6-1.3z"/><path d="M8.6 9.2l1.5 3.4M15.4 9.2l-1.5 3.4M3.4 9.2h17.2"/>',
  coffee:
    '<path d="M4 8.4h13v5.8a5 5 0 0 1-5 5H9a5 5 0 0 1-5-5z"/><path d="M17 10h1.6a2.4 2.4 0 0 1 0 4.8H17"/><path d="M7.4 3.4v2.2M10.6 3.4v2.2M13.8 3.4v2.2"/>',
  music:
    '<path d="M9 18V5.6l10-2v12.4"/><circle cx="6.4" cy="18" r="2.6"/><circle cx="16.4" cy="16" r="2.6"/>',
  camera:
    '<path d="M3.4 8.6a1.6 1.6 0 0 1 1.6-1.6h2.2l1.4-2.4h6.8l1.4 2.4H19a1.6 1.6 0 0 1 1.6 1.6v9a1.6 1.6 0 0 1-1.6 1.6H5a1.6 1.6 0 0 1-1.6-1.6z"/><circle cx="12" cy="12.6" r="3.4"/>',
  lock: '<rect x="4.4" y="10.4" width="15.2" height="10" rx="2"/><path d="M8 10.4V7.8a4 4 0 0 1 8 0v2.6"/><path d="M12 14.2v2.6"/>',
  key: '<circle cx="8" cy="8" r="4.4"/><path d="M11.2 11.2L20.4 20.4"/><path d="M17 17l2-2"/><path d="M14.4 14.4l2-2"/>',
  "key-round":
    '<circle cx="7.6" cy="15.6" r="3.8"/><path d="M10.4 12.8L19.6 3.6"/><path d="M16.4 6.8l2 2"/><path d="M13.8 9.4l2 2"/>',
  bell: '<path d="M6.4 10a5.6 5.6 0 0 1 11.2 0c0 4 1.6 5.4 1.6 5.4H4.8S6.4 14 6.4 10z"/><path d="M10 18.6a2.2 2.2 0 0 0 4 0"/>',
  "app-window":
    '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 8.4h18"/><path d="M6.4 6.2h.02M8.8 6.2h.02" stroke-width="2.2"/>',
  compass:
    '<circle cx="12" cy="12" r="8.6"/><path d="M15.4 8.6l-2 5.4-5.4 2 2-5.4z"/>',
};

/** 图标名 → 中文名，用于预设编辑器的图标选择器 tooltip */
export const ICON_LABEL: Record<string, string> = {
  terminal: "终端",
  folder: "文件夹",
  "folder-open": "已展开文件夹",
  "folder-plus": "新建文件夹",
  inbox: "收件箱",
  search: "搜索",
  plus: "新增",
  edit: "编辑",
  trash: "删除",
  copy: "复制",
  star: "收藏（空心）",
  "star-filled": "收藏（实心）",
  play: "运行",
  "play-circle": "播放",
  stop: "停止",
  square: "方块",
  x: "关闭",
  check: "勾选",
  "chevron-down": "下拉",
  "chevron-right": "右箭头",
  "chevron-left": "左箭头",
  "chevron-up": "上箭头",
  "arrow-right": "向右",
  settings: "设置",
  sun: "浅色",
  moon: "深色",
  list: "列表",
  grid: "网格",
  clock: "时钟",
  calendar: "日历",
  workflow: "工作流",
  shield: "盾牌",
  "alert-triangle": "警告三角",
  "alert-circle": "警告圆",
  info: "信息",
  "check-circle": "成功圆",
  refresh: "刷新",
  download: "下载",
  upload: "上传",
  "more-horizontal": "更多（横）",
  "dots-vertical": "更多（竖）",
  "grip-vertical": "拖动手柄",
  pin: "置顶",
  "pin-filled": "置顶（实心）",
  filter: "筛选",
  "panel-left": "侧栏",
  "external-link": "外部链接",
  history: "历史",
  layers: "分层",
  zap: "闪电",
  globe: "网络",
  code: "代码",
  database: "数据库",
  package: "包",
  cpu: "CPU",
  "hard-drive": "硬盘",
  rocket: "火箭",
  bookmark: "书签",
  save: "保存",
  power: "电源",
  "maximize-2": "最大化",
  "minimize-2": "最小化",
  eye: "显示",
  "eye-off": "隐藏",
  keyboard: "键盘",
  bug: "调试",
  flame: "火焰",
  snowflake: "雪",
  "trending-up": "上升趋势",
  link: "链接",
  "file-text": "文本文件",
  "sort-asc": "升序排列",
  "git-branch": "分支",
  server: "服务器",
  wrench: "扳手",
  gear: "设置",
  chart: "图表",
  brush: "画笔",
  box: "箱子",
  "box-open": "开箱",
  coffee: "咖啡",
  music: "音乐",
  camera: "相机",
  lock: "锁",
  key: "钥匙",
  "key-round": "圆头钥匙",
  bell: "铃铛",
  "app-window": "窗口",
  compass: "指南针",
};

/** 全部图标名（保证唯一顺序） */
export const ICON_NAMES: string[] = Object.keys(ICON_PATHS).filter(
  (n) => ICON_PATHS[n] !== "",
);

/** 未识别图标时的占位方块 */
export const FALLBACK_ICON = "square";

/**
 * 图标选择器用的精选清单。
 * 预设图标的实际取值范围比通用图标小得多，给用户几十个就够了。
 */
export const ICON_PICKER_NAMES: string[] = [
  "terminal",
  "zap",
  "rocket",
  "flame",
  "code",
  "database",
  "package",
  "cpu",
  "hard-drive",
  "server",
  "globe",
  "download",
  "upload",
  "git-branch",
  "wrench",
  "gear",
  "chart",
  "trending-up",
  "brush",
  "box",
  "box-open",
  "coffee",
  "music",
  "camera",
  "compass",
  "file-text",
  "folder",
  "folder-open",
  "folder-plus",
  "inbox",
  "bookmark",
  "link",
  "layers",
  "grid",
  "list",
  "shield",
  "lock",
  "key",
  "key-round",
  "bell",
  "bug",
  "clock",
  "calendar",
  "star",
  "star-filled",
  "pin",
  "power",
  "snowflake",
  "app-window",
];

/**
 * 图标名 → 中文标签。未知图标回退成图标名本身。
 */
export function iconLabel(name: string): string {
  return ICON_LABEL[name] ?? name;
}
