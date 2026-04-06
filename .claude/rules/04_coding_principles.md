# コーディング規約

## 命名規則

### Rust (backend)
- モジュール / ファイル: `snake_case.rs`
- 構造体 / 列挙型 / トレイト: `PascalCase`
- 関数 / 変数 / フィールド: `snake_case`
- 定数: `UPPER_SNAKE_CASE`
- 型パラメータ: `PascalCase` 1文字 (`T`, `E`) またはセマンティック名
- ライフタイム: `'a`, `'b` (短い名前)
- Boolean 変数: `is_`, `has_`, `should_`, `can_` プレフィックス

### TypeScript (frontend)
- コンポーネント / ページ: `PascalCase.tsx`
- フック: `use` プレフィックス + `camelCase.ts` (例: `useViewerState.ts`)
- ストア: `camelCase.ts` (例: `viewerStore.ts`)
- ユーティリティ: `camelCase.ts`
- 定数: `UPPER_SNAKE_CASE`
- Boolean 変数: `is`, `has`, `should`, `can` プレフィックス
- Props 型: `interface XxxProps` (type alias ではなく interface)

## 関数設計
- 単一責任原則: 1つの関数は1つのことだけ行う
- Early Return を積極的に活用し、ネストを浅く保つ
- ガード節で異常系を先に処理する
- 引数は3つ以下を目安。多い場合はオブジェクトにまとめる

## ファイル設計
- 1ファイル最大500行（空行・コメント除外）を目安に分割を検討
- 1コンポーネント1ファイル
- エクスポートは名前付きエクスポートを基本とする（default export はページのみ）

## フォーマット

### Rust (backend)
- rustfmt デフォルト (4-space indent, 100 文字行)
- `use` 順序: std → external crates → crate-internal (rustfmt 自動)
- `clippy::pedantic` を基本とし、許容する lint は `#[allow()]` で明示
- エラー型は `thiserror` で定義。`anyhow` はテストとバイナリエントリポイントのみ
- `unwrap()` / `expect()` はテストコード以外では禁止。`?` 演算子で伝播
- `clone()` の安易な使用を避け、借用で解決できないか検討

### TypeScript
- double quotes, semicolons, 2-space indent (oxfmt)
- import順序: external → internal (oxlint import plugin)
