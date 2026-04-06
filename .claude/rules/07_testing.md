# テスト戦略

## テスト哲学
- 和田卓人氏(`t_wada`)の「動作するきれいなコード」に倣い、TDD の **Red → Green → Refactor** サイクルを小刻みに回す。
- 成果物よりフィードバック速度を優先。失敗するテストを書いてから最小実装、最後に安全なリファクタリングを行う。
- 実装と同等にテストの可読性を重視し、意図が読める名前と前処理/後処理の簡潔さを担保する。

## TDD サイクル
Red → Green → Refactor を小刻みに回す:
1. **Red**: 失敗するテストを先に書く
2. **Green**: テストを通す最小限の実装
3. **Refactor**: コードを整理（テストは通ったまま）

成果物よりフィードバック速度を優先する。

## バックエンド — Rust (cargo test + rstest)
- テストディレクトリ: `backend/tests/` (統合テスト) + 各モジュール内 `#[cfg(test)]` (ユニットテスト)
- HTTP 統合テスト: `tower::ServiceExt::oneshot` を基本に Router テストを書く。必要なら `axum-test` は補助的に検討してよい
- テスト用フィクスチャ: `backend/tests/fixtures/` にサンプルファイル配置
- パラメータ化テスト: `rstest` クレート
- スナップショットテスト: `insta` クレート (JSON レスポンス比較)
- 一時ディレクトリ: `tempfile` クレート

### テストカテゴリ
- **API テスト**: 各エンドポイントの正常系・異常系 (`tower::ServiceExt::oneshot`)
- **サービステスト**: path_security, node_registry, archive, indexer のユニットテスト
- **セキュリティ回帰テスト**: traversal, symlink, zip bomb, 壊れたアーカイブ
### テスト実行
```bash
cd backend && cargo test                    # 全テスト
cd backend && cargo test -- --nocapture     # 標準出力付き
cd backend && cargo test test_node_registry # 特定モジュール
```

## フロントエンド (Vitest + Testing Library)
- テストディレクトリ: `frontend/tests/`
- 環境: jsdom
- テストファイル命名: `*.test.ts` / `*.test.tsx`

### テストカテゴリ
- **ストア/フックのユニットテスト**
- **コンポーネントレンダリングテスト**

## テストの原則
- 1テスト1アサーション概念（複数assertは許容するが、1つの振る舞いを検証）
- テスト名は日本語で振る舞いを記述: `test_存在しないnode_idで404を返す`
- モックよりフィクスチャを優先する
- テストの可読性は実装コードと同等に重視する
