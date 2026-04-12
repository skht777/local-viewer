//! Windows Explorer 互換の自然順ソートキー
//!
//! - `natural_sort_key`: ファイル名を比較可能なキーに変換
//! - `encode_sort_key`: `DirIndex` 用の `SQLite` TEXT 比較キー
//!
//! Python 版 `natural_sort.py` / `dir_index.py` の `encode_sort_key` と互換

use std::sync::LazyLock;

use regex::Regex;

// ASCII 数字のみ (Unicode \d ではない) — Python 互換
static SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    // 安全: リテラル正規表現パターンは常に有効
    #[allow(
        clippy::expect_used,
        reason = "リテラルの正規表現パターンは常にコンパイル成功する"
    )]
    Regex::new(r"([0-9]+)").expect("有効なパターン")
});

/// 自然順ソートキーの要素
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NaturalSortPart {
    /// テキスト断片 (小文字化済み)
    Text(String),
    /// 数値断片
    Number(u64),
}

impl PartialOrd for NaturalSortPart {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NaturalSortPart {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
            (Self::Number(a), Self::Number(b)) => a.cmp(b),
            // 同一インデックスでは常に同じバリアント (alternating 構造)
            // 万が一異なる場合: Number < Text (Python の int/str 比較順序に近い)
            (Self::Number(_), Self::Text(_)) => std::cmp::Ordering::Less,
            (Self::Text(_), Self::Number(_)) => std::cmp::Ordering::Greater,
        }
    }
}

/// ファイル名を自然順ソート用のキーに変換する
///
/// 文字列を「テキスト部分」と「数値部分」に分割し、
/// 数値部分を u64 に変換してリスト比較することで
/// file1, file2, file10 の順にソートする。
#[must_use]
pub(crate) fn natural_sort_key(name: &str) -> Vec<NaturalSortPart> {
    let lower = name.to_lowercase();
    let mut parts = Vec::new();
    let mut last_end = 0;

    for m in SPLIT_RE.find_iter(&lower) {
        // マッチ前のテキスト部分
        let text = &lower[last_end..m.start()];
        parts.push(NaturalSortPart::Text(text.to_owned()));

        // 数値部分
        let num: u64 = m.as_str().parse().unwrap_or(u64::MAX);
        parts.push(NaturalSortPart::Number(num));

        last_end = m.end();
    }

    // 末尾のテキスト部分
    parts.push(NaturalSortPart::Text(lower[last_end..].to_owned()));
    parts
}

/// ファイル名を `SQLite` TEXT 比較で自然順になるソートキーに変換する
///
/// 数値部分を 20 桁ゼロ埋め (`u64::MAX` = 20桁と整合)、テキスト部分は小文字化、
/// 要素間を NUL 文字で区切る。
/// 例: "file2.jpg" → "file\x0000000000000000000002\x00.jpg"
#[must_use]
pub(crate) fn encode_sort_key(name: &str) -> String {
    use std::fmt::Write as _;

    let lower = name.to_lowercase();
    let mut result = String::with_capacity(lower.len() + 30);
    let mut last_end = 0;

    for m in SPLIT_RE.find_iter(&lower) {
        if !result.is_empty() {
            result.push('\x00');
        }
        result.push_str(&lower[last_end..m.start()]);
        result.push('\x00');
        // 20 桁ゼロ埋め (u64::MAX = 20桁)
        let _ = write!(result, "{:0>20}", m.as_str());
        last_end = m.end();
    }

    if !result.is_empty() {
        result.push('\x00');
    }
    result.push_str(&lower[last_end..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // natural_sort_key を使ってソートするヘルパー
    fn sort(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| (*s).to_owned()).collect();
        v.sort_by_key(|a| natural_sort_key(a));
        v
    }

    // --- Python テスト 7 件ポート ---

    #[test]
    fn 基本的な数値順でソートされる() {
        assert_eq!(
            sort(&["file1", "file10", "file2"]),
            ["file1", "file2", "file10"]
        );
    }

    #[test]
    fn 複数の数値区間を正しくソートする() {
        assert_eq!(
            sort(&["ch2p10", "ch2p2", "ch10p1"]),
            ["ch2p2", "ch2p10", "ch10p1"]
        );
    }

    #[test]
    fn 大文字小文字を無視してソートする() {
        assert_eq!(sort(&["FileB", "filea"]), ["filea", "FileB"]);
    }

    #[test]
    fn 数値なしは辞書順と同一になる() {
        assert_eq!(
            sort(&["banana", "apple", "cherry"]),
            ["apple", "banana", "cherry"]
        );
    }

    #[test]
    fn 日本語と数値の混在を正しくソートする() {
        assert_eq!(
            sort(&["第1巻", "第10巻", "第2巻"]),
            ["第1巻", "第2巻", "第10巻"]
        );
    }

    #[test]
    fn 数値のみのファイル名をソートする() {
        assert_eq!(sort(&["10", "1", "20", "2"]), ["1", "2", "10", "20"]);
    }

    #[test]
    fn 拡張子付きファイル名を正しくソートする() {
        assert_eq!(
            sort(&["img10.jpg", "img1.jpg", "img2.jpg"]),
            ["img1.jpg", "img2.jpg", "img10.jpg"]
        );
    }

    // --- encode_sort_key ---

    #[test]
    fn encode_sort_keyの基本出力() {
        assert_eq!(
            encode_sort_key("file2.jpg"),
            "file\x0000000000000000000002\x00.jpg"
        );
    }

    #[test]
    fn encode_sort_key数値なし() {
        assert_eq!(encode_sort_key("readme"), "readme");
    }

    #[test]
    fn encode_sort_key先頭が数値() {
        assert_eq!(
            encode_sort_key("10files"),
            "\x0000000000000000000010\x00files"
        );
    }

    // --- 順序整合性 ---

    // encode_sort_key を使ってソートするヘルパー
    fn sort_by_encode(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| (*s).to_owned()).collect();
        v.sort_by_key(|a| encode_sort_key(a));
        v
    }

    #[test]
    fn 十桁超数値でnatural_sort_keyとencode_sort_keyの順序が一致する() {
        let names = &["file9", "file10", "file99999999999", "file100000000000"];
        assert_eq!(sort(names), sort_by_encode(names));
    }

    // --- エッジケース ---

    #[test]
    fn 空文字列のsort_key() {
        let key = natural_sort_key("");
        assert_eq!(key.len(), 1);
        assert_eq!(key[0], NaturalSortPart::Text(String::new()));
    }
}
