# UI/デザイン設定

## 基本原則
1. **コンポーネント優先**: 抽象化されたUIコンポーネントを使用する。直接HTMLタグの羅列を避ける
2. **ミニマリズム**: 簡素なUI/UX、機能美を追求。不要な装飾を排除する
3. **CSSファイル禁止**: Tailwind CSS v4 ユーティリティクラスのみ使用
4. **className 抑制**: 複数クラスが長くなる場合はコンポーネント化を検討

## カラー
- ダーク固定（テーマ切替なし）
- ベース: `bg-surface-base`, `text-white`
- `@theme` トークン（surface-base, surface-card 等）で管理（直接色コード指定を避ける）
- Tailwind v4 カラーパレットは50刻みのみ使用（bg-gray-750 等は禁止）

## タイポグラフィ
- Tailwind のフォントサイズクラスを使用
- フォントサイズ・ウェイトの直接指定を避ける

## スペーシング・レイアウト
- Tailwind の spacing ユーティリティで統一
- 大きめの余白を確保し、詰め込みすぎない
- ビューワー画面はコンテンツを最大限に表示する

## レスポンシブ
- **デスクトップ最適化 + モバイル対応**
- 主要動作環境はデスクトップ (≥1024px) だが、スマートフォン (375px 以上) でも操作可能であることを必須とする
- ブレイクポイント: Tailwind v4 デフォルト (`sm:640 / md:768 / lg:1024 / xl:1280`)。`lg:` をデスクトップ層境界として一貫運用 (base は ≥375px のモバイル想定)
- 横画面短小スマホ向けの個別調整は arbitrary variant `[@media(orientation:landscape)_and_(max-height:500px)]:` を使用
- safe-area (notch): `viewport-fit=cover` + `@theme` の `--spacing-safe-*` トークン経由で `pt-safe-top` 等を適用
- タッチターゲット: 操作要素は 44×44px 以上 (Apple HIG)。`py-2.5` ベースで確保
- レイアウト由来の横スクロールは 375px 環境で発生させない (画像/PDF 原寸表示時のコンテンツ overflow は例外、`overflow-x-auto` 等で明示)
- 新規 npm 依存 (framer-motion / hammerjs 等) は導入禁止。タッチは標準 PointerEvent API で実装

## TopPage
- マルチマウントポイント対応: `GET /api/mounts` から取得した一覧をカード形式で表示
- 各マウントポイントカードにラベル名とパス情報を表示

## デザインシステム参照
- 詳細なデザイン方針・コンポーネントパターンは `viewer-design-system` スキルを参照
- カスタムカラートークン (`surface-base`, `surface-card` 等) は `index.css` の `@theme` で定義
