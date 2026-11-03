import { defineConfig } from "vite";
import { readFileSync } from "node:fs";

const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf-8"));

// На телефоне dev-сервер открывается по адресу компьютера в локальной сети:
// Tauri передаёт его в TAURI_DEV_HOST при `tauri android dev`.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  define: { __APP_VERSION__: JSON.stringify(pkg.version) },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "chrome105",
    outDir: "dist",
    emptyOutDir: true,
  },
});
