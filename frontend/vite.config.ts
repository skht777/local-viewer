/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { VitePWA } from "vite-plugin-pwa";

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
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
