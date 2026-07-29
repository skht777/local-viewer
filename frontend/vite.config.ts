/// <reference types="vitest/config" />
// oxlint-disable-next-line import/no-nodejs-modules -- ビルド時アセット複製のため Node fs を使用
import { cpSync, existsSync } from "node:fs";
import { defineConfig } from "vite";
import type { Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// pdfjs-dist の cmaps / standard_fonts を public/ に複製する
// - cMapUrl / standardFontDataUrl はハッシュ無しの固定ファイル名を要求するため、
//   Vite のアセットパイプライン (ハッシュ付与) ではなく public/ 経由で配信する
// - dev / build 両方で buildStart に走り、未複製時のみコピー (gitignored)
function copyPdfAssets(): Plugin {
  return {
    name: "copy-pdf-assets",
    buildStart() {
      const pairs: [string, string][] = [
        ["node_modules/pdfjs-dist/cmaps", "public/pdfjs/cmaps"],
        ["node_modules/pdfjs-dist/standard_fonts", "public/pdfjs/standard_fonts"],
      ];
      for (const [src, dest] of pairs) {
        if (!existsSync(dest) && existsSync(src)) {
          cpSync(src, dest, { recursive: true });
        }
      }
    },
  };
}

export default defineConfig({
  // PWA (Service Worker) は撤去済み:
  // - runtimeCaching は urlPattern が絶対 URL に一致せず一度も発火していなかった
  // - キャッシュ戦略は HTTP キャッシュ (ETag + ?v={modified_at} 版数付き URL) に一本化
  plugins: [react(), tailwindcss(), copyPdfAssets()],
  server: {
    host: true,
    proxy: {
      "/api": {
        target: `http://localhost:${process.env.VITE_API_PORT || "8000"}`,
        changeOrigin: true,
      },
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: "./tests/setup.ts",
  },
});
