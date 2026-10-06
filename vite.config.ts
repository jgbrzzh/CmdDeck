import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath, URL } from "node:url";

// Vite 前端构建配置。
// Tauri 2 会在打包时把 distDir 指向 ../dist，并在编译前注入 Tauri CLI 环境变量。
export default defineConfig({
  plugins: [vue()],

  // 解析别名，@ 指向 src 目录，代码里统一用 `@/xxx` 引入模块
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },

  // Tauri 期望一个固定端口的开发服务器，端口被占用时自动 +1
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: {
      // 监听 src-tauri 目录，后端改动触发 Rust 重编译
      ignored: ["**/src-tauri/**"],
    },
  },

  // 生产构建输出
  build: {
    // Windows 10/11 使用 WebView2（Chromium 内核），可以安全使用 esnext
    target:
      process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: process.env.TAURI_ENV_DEBUG ? false : "oxc",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    outDir: "dist",
    emptyOutDir: true,
    chunkSizeWarningLimit: 2000,
    rolldownOptions: {
      output: {
        manualChunks: (id: string) =>
          id.includes("@xterm")
            ? "xterm"
            : id.includes("node_modules/vue")
              ? "vue"
              : undefined,
      },
    },
  },
});
