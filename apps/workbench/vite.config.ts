import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

// Tauri 约定：固定端口 + 不清屏（dev 日志可见），构建目标对齐 WebView2
export default defineConfig({
  plugins: [preact()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "chrome105",
    minify: "esbuild",
  },
});
