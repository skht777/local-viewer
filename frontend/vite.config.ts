/// <reference types="vitest/config" />
// oxlint-disable-next-line import/no-nodejs-modules -- ビルド時アセット複製のため Node fs を使用
import { cpSync, existsSync } from "node:fs";
import { defineConfig } from "vite";
import type { Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { VitePWA } from "vite-plugin-pwa";

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
  plugins: [
    react(),
    tailwindcss(),
    copyPdfAssets(),
    // oxlint-disable-next-line new-cap -- VitePWA は vite-plugin-pwa の公式エクスポート関数（ファクトリ）
    VitePWA({
      registerType: "autoUpdate",
      workbox: {
        runtimeCaching: [
          // サムネイル: CacheFirst (immutable URL でキャッシュ自動更新)
          {
            urlPattern: /^\/api\/thumbnail\//,
            handler: "CacheFirst",
            options: {
              cacheName: "thumbnails",
              expiration: {
                maxEntries: 2000,
                // 30日
                maxAgeSeconds: 30 * 24 * 60 * 60,
              },
              cacheableResponse: { statuses: [0, 200] },
            },
          },
          // その他 API: NetworkFirst (動的データ)
          {
            urlPattern: /^\/api\//,
            handler: "NetworkFirst",
            options: {
              cacheName: "api",
              expiration: { maxAgeSeconds: 5 * 60 },
              cacheableResponse: { statuses: [0, 200] },
            },
          },
        ],
        navigateFallback: "/index.html",
        navigateFallbackDenylist: [/^\/api\//],
      },
      // PWA インストール不要、キャッシュのみが目的
      manifest: false,
    }),
  ],
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
    alias: {
      // テスト環境では virtual:pwa-register をスタブに差し替え
      "virtual:pwa-register": "./tests/__mocks__/pwa-register.ts",
    },
  },
});
