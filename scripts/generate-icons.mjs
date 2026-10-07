// 从 SVG 生成 Windows 图标；其它平台的中间产物保留在忽略的依赖缓存中。
import { copyFileSync, mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const source = resolve(root, "src-tauri/icons/app-icon.svg");
const output = resolve(root, "node_modules/.cache/cmddeck-icons");
mkdirSync(output, { recursive: true });
const result = spawnSync(
  process.execPath,
  [
    resolve(root, "node_modules/@tauri-apps/cli/tauri.js"),
    "icon",
    source,
    "--output",
    output,
  ],
  { cwd: root, stdio: "inherit" },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
for (const name of ["32x32.png", "128x128.png", "128x128@2x.png", "icon.ico"]) {
  copyFileSync(resolve(output, name), resolve(root, "src-tauri/icons", name));
}
copyFileSync(source, resolve(root, "public/favicon.svg"));
