---
name: viewer-design-system
description: >
  Local Content Viewer 固有のデザインシステム。React/Tailwind コンポーネントの作成・修正、
  TopPage・BrowsePage・FileBrowser・FileCard・MountPointCard・ViewerTabs・DirectoryTree・
  CgToolbar・MangaToolbar・SearchBar 等の UI 変更時に適用。
  frontend-design プラグインの汎用ガイダンスを、コンテンツビューワーの文脈にフィルタリングする。
---

# Local Content Viewer デザインシステム

## 設計哲学

このアプリは**コンテンツビューワー**である。
UI は脇役であり、画像・動画・PDF というコンテンツが常に主役でなければならない。

### 3つの原則

1. **コンテンツファースト**: ビューワー画面では UI を最小限にし、コンテンツ表示面積を最大化する
2. **静かな上質感**: 派手な装飾ではなく、素材の質感・微細なディテールで品質を伝える
3. **機能美**: すべての視覚要素に機能的理由がある。装飾だけの要素は加えない

### 画面ごとの美学レベル

| 画面 | 方針 | UI の存在感 |
|------|------|------------|
| トップページ | ブランド感を出せる唯一の場所。装飾的表現を許容 | 高 |
| ブラウズ画面 | サムネイルグリッドが主役。UI は枠組みに徹する | 中 |
| ビューワー画面 | UI は可能な限り消える。ツールバーは半透明、操作しない時は非表示 | 低 |

---

## 1. Typography（書体戦略）

### フォントスタック

Docker コンテナはバックエンド(Python)のみ。フロントエンドはユーザーのブラウザで表示されるため、
OS ネイティブフォントが有効に機能する。ウェブフォントの外部呼び出しは不要。

**UI テキスト** (`--font-sans`):
`"SF Pro Text", "Segoe UI", system-ui, -apple-system, sans-serif, "Apple Color Emoji", "Segoe UI Emoji"`

**等幅** (`--font-mono`):
`"SF Mono", "Cascadia Code", "Consolas", ui-monospace, monospace`

### タイポグラフィスケール

- トップページ見出し: `text-4xl font-light tracking-tight`
- セクションラベル: `text-sm font-medium uppercase tracking-wider text-gray-500`
- ファイル名: `text-sm truncate`（既存維持）
- メタデータ（サイズ、件数、ページ番号）: `text-xs font-mono tabular-nums text-gray-500`
- ツールバーラベル: `text-xs text-gray-400`

### 禁止

- Inter, Roboto, Space Grotesk を指定しない（AI の統計的デフォルト）
- Google Fonts CDN リンクを挿入しない
- 明朝体・セリフ体を使わない

---

## 2. Color & Theme（色彩・テーマ体系）

### サーフェス階層（5段階）

均一な gray-800/900 ではなく、z-index と意味に応じた階層を持たせる。
純粋な gray ではなく、僅かに青を含んだダーク — 画像の色彩が引き立つ。

| レベル | 用途 | 色 | クラス |
|--------|------|-----|--------|
| Base | 最背面、App ルート | `#0a0a0f` | `bg-surface-base` |
| Ground | コンテンツ領域背景 | `#111118` | `bg-surface-ground` |
| Card | カード、サイドバー | `#1a1a24` | `bg-surface-card` |
| Raised | ホバー、ドロップダウン | `#24243a` | `bg-surface-raised` |
| Overlay | モーダル、ツールバー | `#2a2a3d` | `bg-surface-overlay` |

> **注意**: Base は現行 gray-900 (`#111827`) より暗い。
> 実適用時に「暗すぎる」と感じたら値を調整すること。

### テキスト階層

| 階層 | 用途 | クラス |
|------|------|--------|
| Primary | 見出し、重要テキスト | `text-gray-50` |
| Secondary | 通常テキスト | `text-gray-300` |
| Tertiary | 補助テキスト、ラベル | `text-gray-500` |
| Muted | 無効状態、プレースホルダ | `text-gray-600` |

### アクセントカラー

- 現行: `blue-500` / `blue-600`（フォーカスリング、アクティブ状態）
- 変更する場合は全コンポーネントで一括変更すること（部分的変更禁止）

### 禁止

- 紫 / バイオレットのグラデーションを使わない
- 虹色・マルチカラーのグラデーションを使わない
- 白背景を使わない（常にダークテーマ）
- ライトテーマ切替機能を導入しない

---

## 3. Motion（モーション・トランジション）

### 基本方針

framer-motion は未導入。CSS トランジション + CSS アニメーションのみで実現する。
注意を引くためではなく、**状態変化を滑らかに伝える**ためにモーションを使う。

### トランジション体系

| 用途 | duration | easing | Tailwind |
|------|----------|--------|----------|
| 色変化（ホバー、フォーカス） | 150ms | ease-out | `transition-colors duration-150` |
| 表示/非表示（フェード） | 200ms | ease-in-out | `transition-opacity duration-200` |
| サイズ変化（展開/折りたたみ） | 250ms | ease-out | `transition-all duration-250` |
| 位置移動（スライドイン） | 300ms | ease-out | カスタム easing |

### 許容するアニメーション

- フェードイン: 新しいコンテンツの出現（ドロップダウン、検索結果）
- スケール: カードホバー時の微細な拡大 (`hover:scale-[1.02]`)
- スライドイン: サイドバー、ツールバーの出入り

### トップページ限定

トップページのみ、マウントポイントカードの順次フェードインを許容:
- `animate-fade-in-up` クラス（`@theme` で定義済み）
- 各カードに `style={{ animationDelay: calc(index * 80ms) }}` でスタガー

### 禁止

- 300ms を超えるトランジションを使わない（ビューワーでは速さが正義）
- バウンス、スプリング、弾性エフェクトを使わない
- 画像やサムネイルにモーションを加えない（コンテンツの邪魔）
- framer-motion を導入しない
- パララックスを使わない

---

## 4. Backgrounds & Surfaces（背景・サーフェス）

### カードスタイル

フラットな `bg-gray-800 rounded-lg` から、微細な深度を加える方向。

**サムネイルカード (FileCard)**:
- 背景: `bg-surface-card rounded-lg ring-1 ring-white/5`
- ホバー: `hover:ring-white/10 hover:scale-[1.02] transition-all duration-150`
- 選択状態: `ring-2 ring-blue-500/50 bg-blue-500/10`

**マウントポイントカード (TopPage)**:
- 背景: `bg-surface-card rounded-2xl ring-1 ring-white/5 shadow-lg shadow-black/20`
- ホバー: `hover:shadow-xl hover:ring-white/10`

> 上記は参考例。厳密な仕様ではなく、方向性として参照すること。

### ボーダー戦略

- ハードボーダー (`border-gray-700`) → ソフトリング (`ring-1 ring-white/5`) に段階移行
- `ring-white/5` は背景色に左右されず、一貫した微かな区切りを生む

### ツールバー・ヘッダー

- ブラウズ画面のヘッダー: 現行 `bg-gray-800 border-b border-gray-700` を維持
- ビューワーのオーバーレイツールバー: `bg-black/50 backdrop-blur-md`（現行 `bg-black/60`）

### 禁止

- グラデーション背景を多用しない（トップページのアクセント以外）
- ノイズテクスチャやパターン背景を追加しない（画像閲覧の邪魔）
- ドロップシャドウを多用しない（ダークテーマでは効果が薄い）
- ガラスモーフィズムを全面適用しない（ビューワーツールバーのみに限定）

---

## 5. コンポーネントパターン集

以下は方向性を示す参考例。具体クラスは実装時の判断に委ねる。

### ボタン

**プライマリ（アクション）**:
`bg-blue-600 text-white rounded-lg px-4 py-2 text-sm font-medium
 hover:bg-blue-500 active:scale-[0.98] transition-all duration-150`

**ゴースト（ツールバー）**:
`text-gray-400 hover:text-white hover:bg-white/10 rounded-lg px-2 py-1.5
 transition-colors duration-150`

**トグル（active 状態）**:
`bg-blue-600/20 text-blue-400 ring-1 ring-blue-500/30`

### 入力フィールド

`bg-surface-ground rounded-lg px-4 py-2.5 text-white placeholder-gray-600
 ring-1 ring-white/10 focus:ring-2 focus:ring-blue-500/50 transition-all duration-150`

### ドロップダウン

`bg-surface-raised rounded-lg ring-1 ring-white/10 shadow-xl shadow-black/40`

### タブ

- アクティブ: `border-b-2 border-blue-500 text-white`
- 非アクティブ: `text-gray-500 hover:text-gray-300 transition-colors duration-150`

### ツリーノード

- アクティブ: `bg-blue-600/10 text-white border-l-2 border-blue-500`
- ホバー: `hover:bg-white/5`

---

## 6. 禁止リスト（AI 収束パターン回避）

AI が統計的中心に収束しやすいパターンを明示的に禁止する。

### フォント
- Inter, Roboto, Space Grotesk を指定しない
- Google Fonts CDN を使わない

### カラー
- 紫グラデーション、白背景を使わない
- テーマ切替機能を導入しない

### レイアウト
- ヒーローセクションを作らない（ビューワーアプリには不要）
- フッターを作らない
- 余計なマージンやパディングを加えない（コンテンツ面積を減らさない）

### モーション
- framer-motion を導入しない
- パララックス、ホバー回転を使わない

### 装飾
- アイコンライブラリを新たに導入しない（現在の絵文字 + テキストで十分）
- blob 形状や装飾的 SVG を追加しない

---

## 7. frontend-design プラグインとの使い分け

### 役割分担

- **frontend-design プラグイン**: デザインテクニックの引き出し（インプット源）
- **本スキル (viewer-design-system)**: このアプリの美学フィルター（採用/却下の判断基準）

### 適用優先順位

1. 本スキルの禁止リストに該当 → **却下**
2. 本スキルのパターン集に一致 → **本スキルに従う**
3. 本スキルに記載なし → **frontend-design の提案を検討**、ただし設計哲学に照らして判断

### 典型的な判断例

| frontend-design の提案 | 判断 | 理由 |
|-------------------------|------|------|
| 意外なフォントペアリング | 却下 | ウェブフォント不可、システムフォントスタック固定 |
| スタガードアニメーション | トップページのみ許可 | ビューワー画面では不要 |
| グラデーション背景 | 限定的に許可 | トップページのアクセントのみ |
| グリッド破壊レイアウト | 却下 | サムネイルグリッドは規則正しさが重要 |
| テクスチャ・ノイズ | 却下 | 画像閲覧の邪魔 |
| 視覚的深度（shadow, ring） | 採用 | サーフェス階層の表現に有効 |
| 非対称レイアウト | 却下 | ブラウザ型UIには不適切 |
| CSS 変数によるテーマ管理 | 採用 | @theme トークンと合致 |
