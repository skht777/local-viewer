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

### 前提チェック + ベースブランチ検出

1回の Bash 呼び出しで前提チェックとベースブランチ検出をまとめて実行する:

```bash
git rev-parse --is-inside-work-tree && git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@'
```

- `git rev-parse` が失敗した場合は「Git 管理外のため review モードは使用不可。ファイルを指定して exec モードを使用してください」とユーザーに伝える
- `--base` 使用時、ブランチ名は上記で自動検出した値を使う

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

### ファイル配置の判定

指定ファイルの絶対パスを確認し、以下のルールで Case A / Case B を判定する:

- 全ファイルが「プロジェクトルート配下」または「`~/.claude` 配下」→ **Case A**
- それ以外のパスを含む → **Case B**

### Case A: プロジェクト内 or `~/.claude` 内（主要ケース）

コピー不要。`--add-dir ~/.claude` でプロジェクトと `~/.claude` 両方にアクセス可能。

```bash
codex exec -C "$(git rev-parse --show-toplevel)" \
  --add-dir ~/.claude \
  -s read-only \
  "<レビュー指示>" \
  -o /tmp/codex-review-output.md
```

- `--skip-git-repo-check` 不要（プロジェクトは Git 管理下）
- Bash 呼び出し: 1回のみ

### Case B: 外部ファイルを含む

全ファイルを `/tmp` にコピーし、codex exec でレビューする。

```bash
# 1. 一意な作業ディレクトリを作成
workdir=$(mktemp -d /tmp/codex-review-XXXXXX)

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
  -o /tmp/codex-review-output.md

# 4. Read ツールで出力ファイルを読む
# 5. クリーンアップ（引数で削除対象を指定）
.claude/scripts/codex-review-cleanup.sh "$workdir" /tmp/codex-review-output.md
```

## レビュー指示のデフォルト

ユーザーが具体的な指示を出さない場合:

```
以下のコードをプロジェクト規約に基づきレビューしてください。

## レビュー観点

1. アーキテクチャ・依存関係
   - レイヤー間の依存方向（Backend: routers→services→infrastructure、Frontend: pages→components→hooks→stores）
   - path_security を経由しないファイルアクセスの有無

2. セキュリティ
   - パストラバーサル（resolve() + ROOT_DIR 検証）
   - ユーザー入力のパス・コマンドへの直接組み込み
   - シークレットのハードコード
   - アーカイブ操作の安全性

3. コーディング規約
   - 命名規則（Python: snake_case、TypeScript: PascalCase/camelCase）
   - 単一責任原則・Early Return・ガード節
   - ファイル500行超過

4. コメント駆動
   - 複雑なロジック前の why/what コメントの有無

5. バグ・エッジケース
   - 未処理エラー、null/undefined の見落とし
   - 非同期処理の競合・リソースリーク
   - CPU バウンド処理によるイベントループブロック

6. 改善提案
   - 各指摘に重要度（Critical/Warning/Info）と修正例を付与
```

## 出力

- `-o /tmp/codex-review-output.md` でキャプチャ
- Read ツールで読み取り、ユーザーに提示
- 長い場合は要約を先に出し、詳細は求められたら提示
