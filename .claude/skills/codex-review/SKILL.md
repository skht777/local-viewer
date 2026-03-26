---
name: codex-review
description: >
  codex CLI を使ったコードレビュー。「codex でレビューして」「codex review」等の
  指示で発動。git diff ベースのレビュー（codex review）と、
  特定ファイル指定のレビュー（codex exec）を文脈に応じて使い分ける。
allowed-tools: Read, Grep, Glob, Bash
---

# codex-review

codex CLI を使ってコードレビューを実行する。

## モード判定

ユーザーの指示から適切なモードを選択する:

- **review モード**: ファイル指定なし、「変更」「diff」「コミット」等 → `codex review`
- **exec モード**: レビュー対象ファイルが明示されている → `codex exec`

## Mode A: review モード（git diff ベース）

git の変更差分をレビューする。

```bash
# 未コミット変更（デフォルト）
codex review --uncommitted "<レビュー指示>" -o /tmp/codex-review-output.md

# ベースブランチとの差分
codex review --base main "<レビュー指示>" -o /tmp/codex-review-output.md

# 特定コミットの変更
codex review --commit <SHA> "<レビュー指示>" -o /tmp/codex-review-output.md
```

ユーザーが対象を指定しない場合は `--uncommitted` をデフォルトとする。

## Mode B: exec モード（ファイル指定）

特定ファイルを `/tmp` にコピーし、codex exec でレビューする。

### 手順

1. `/tmp/codex-review/` を準備（既存なら削除して再作成）
2. 指定ファイルをディレクトリ構造を保持してコピー
3. `codex exec` を実行
4. 出力を Read ツールで読み取りユーザーに提示
5. `/tmp/codex-review/` をクリーンアップ

```bash
# 1. 準備
rm -rf /tmp/codex-review && mkdir -p /tmp/codex-review

# 2. コピー（ディレクトリ構造保持）
for f in <files>; do
  mkdir -p "/tmp/codex-review/$(dirname "$f")"
  cp "$f" "/tmp/codex-review/$f"
done

# 3. 実行
codex exec -C /tmp/codex-review \
  -s read-only \
  "<レビュー指示>" \
  -o /tmp/codex-review-output.md

# 4. Read ツールで /tmp/codex-review-output.md を読む
# 5. クリーンアップ
rm -rf /tmp/codex-review
```

## レビュー指示のデフォルト

ユーザーが具体的な指示を出さない場合:

```
以下のコードをレビューしてください。
- コードの品質、バグ、セキュリティリスクを指摘
- 改善提案を具体的に提示
```

## 出力

- `-o /tmp/codex-review-output.md` でキャプチャ
- Read ツールで読み取り、ユーザーに提示
- 長い場合は要約を先に出し、詳細は求められたら提示
