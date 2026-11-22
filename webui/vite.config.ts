import { defineConfig } from "vite";
import solidPlugin from "vite-plugin-solid";
import UnoCSS from "unocss/vite";

export default defineConfig({
  plugins: [UnoCSS(), solidPlugin()],
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
        ws: true,
      },
    },
  },
  build: {
    target: "esnext",
    outDir: "dist",
    emptyOutDir: true,
  },
});
