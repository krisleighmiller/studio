import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig({
  // Relative base so Electron loadFile() works from dist/
  base: "./",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: {
      ignored: ["**/host/target/**", "**/electron/**"],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
