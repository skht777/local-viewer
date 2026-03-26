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

### 前提チェック

実行前に Git 管理下か確認する:

```bash
git rev-parse --is-inside-work-tree
```

失敗した場合は「Git 管理外のため review モードは使用不可。ファイルを指定して exec モードを使用してください」とユーザーに伝える。

### ベースブランチの検出

`--base` 使用時、ブランチ名はハードコードせず自動検出する:

```bash
git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@'
```

### コマンド

```bash
# 未コミット変更（デフォルト）
codex review --uncommitted "<レビュー指示>" -o /tmp/codex-review-output.md

# ベースブランチとの差分（ブランチ名は自動検出）
codex review --base <detected-branch> "<レビュー指示>" -o /tmp/codex-review-output.md

# 特定コミットの変更
codex review --commit <SHA> "<レビュー指示>" -o /tmp/codex-review-output.md
```

ユーザーが対象を指定しない場合は `--uncommitted` をデフォルトとする。

## Mode B: exec モード（ファイル指定）

特定ファイルを `/tmp` にコピーし、codex exec でレビューする。

### 手順

1. `mktemp -d` で一意な作業ディレクトリを作成
2. 指定ファイルをディレクトリ構造を保持してコピー
3. `codex exec --skip-git-repo-check` を実行
4. 出力を Read ツールで読み取りユーザーに提示
5. 作業ディレクトリをクリーンアップ

```bash
# 1. 一意な作業ディレクトリを作成
workdir=$(mktemp -d /tmp/codex-review-XXXXXX)
outfile=$(mktemp /tmp/codex-review-output-XXXXXX.md)

# 2. コピー（ディレクトリ構造保持）
for f in <files>; do
  mkdir -p "$workdir/$(dirname "$f")"
  cp "$f" "$workdir/$f"
done

# 3. 実行（/tmp は Git 管理外のため --skip-git-repo-check 必須）
codex exec -C "$workdir" \
  -s read-only \
  --skip-git-repo-check \
  "<レビュー指示>" \
  -o "$outfile"

# 4. Read ツールで $outfile を読む
# 5. クリーンアップ
rm -rf "$workdir" "$outfile"
```

## レビュー指示のデフォルト

ユーザーが具体的な指示を出さない場合:

```
以下のコードをレビューしてください。
- コードの品質、バグ、セキュリティリスクを指摘
- 改善提案を具体的に提示
```

## 出力

- `-o $outfile` でキャプチャ（`mktemp` で生成した一意なパス）
- Read ツールで読み取り、ユーザーに提示
- 長い場合は要約を先に出し、詳細は求められたら提示
