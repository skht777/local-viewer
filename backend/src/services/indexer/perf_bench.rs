//! Phase 0 baseline bench: 2 文字 search 性能測定
//!
//! ## 目的
//!
//! `plan-search-2char-bigram` の abort gate 判定に使う。
//! 100k entries × 2 mount で 2 文字 LIKE フォールバックの P95 が既に
//! 閾値内なら bigram index 導入 (Phase 1) を skip する。
//!
//! ## 実行
//!
//! ```bash
//! cargo test --release --lib services::indexer::perf_bench \
//!     -- --ignored --nocapture
//! ```
//!
//! ## 受入基準
//!
//! 100k entries で以下をすべて満たす場合 Phase 1+ skip:
//! - 6 シナリオ全てで P95 < 30 ms
//!   - 2-char JP `"写真"` / ASCII `"ab"` / ASCII `"AB"` × scope=None / scope=mount
//! - `EXPLAIN QUERY PLAN` に SCAN entries が出現しても wall-clock が閾値内なら OK
//!
//! ## 仕様
//!
//! - 合成 corpus は log-normal 近似（8 サンプル平均で中心極限定理）で name / path 長を分布させる
//! - 乱数は decade LCG で seed 42 固定 → 再現可能
//! - 30 サンプル × 6 シナリオ、median / P95 を出力
//! - 終了時に `PRAGMA page_count * page_size` で DB サイズを記録

// bench 出力は `--nocapture` 前提。通常ビルド時は `#[cfg(test)]` で除外されるため
// 本番ロギング規約 (`print_stderr` 禁止) を緩和する
#![allow(
    clippy::print_stderr,
    reason = "bench の --nocapture 出力用、#[cfg(test)] 限定"
)]

use std::time::{Duration, Instant};

use rusqlite::{Connection, params};

use super::{Indexer, SearchOrder, SearchParams};

// --- 乱数 (再現性のため内製 LCG) ---

/// 線形合同法 (Knuth の MMIX 定数) による決定的疑似乱数
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    fn next_range(&mut self, min: usize, max_exclusive: usize) -> usize {
        debug_assert!(max_exclusive > min);
        min + (self.next_u64() as usize) % (max_exclusive - min)
    }

    fn next_f64_01(&mut self) -> f64 {
        // 上位 53 bit を [0, 1) に写像
        (self.next_u64() >> 11) as f64 / ((1_u64 << 53) as f64)
    }
}

// --- 合成 corpus ---

/// name / path セグメントに使用する文字集合 (ASCII 英数 + 日本語頻出)
const CHARS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', '_', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '写',
    '真', '動', '画', '音', '楽', '仕', '事', '私', '的', '資', '料', '映', '像', '旅', '行', '記',
    '録', '家', '族',
];

/// 近似対数正規分布でセグメント長をサンプル
///
/// - `mean_ln`: 対数スケール中央値 (例: ln(20) ≈ 3.0)
/// - `sigma`: 対数スケール標準偏差 (例: 0.5)
/// - 8 サンプル和 + 中心極限定理で N(0,1) を近似
fn sample_log_normal(rng: &mut Lcg, mean_ln: f64, sigma: f64, min: usize, max: usize) -> usize {
    let mut sum = 0.0_f64;
    for _ in 0..8 {
        sum += rng.next_f64_01();
    }
    // 8 個の uniform[0,1] 和 → 平均 4.0、分散 8/12 の近似正規分布
    let z = (sum - 4.0) / (8.0_f64 / 12.0_f64).sqrt();
    let val = (mean_ln + sigma * z).exp();
    let clamped = val.clamp(min as f64, max as f64);
    // f64 → usize は as_ で切り捨て。min/max 範囲内なので安全
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "clamp 済みの f64 → usize キャスト"
    )]
    {
        clamped as usize
    }
}

/// 指定長の name セグメントを生成
fn gen_segment(rng: &mut Lcg, target_len: usize) -> String {
    (0..target_len)
        .map(|_| CHARS[(rng.next_u64() as usize) % CHARS.len()])
        .collect()
}

/// 合成 entry を count 件生成して batch 挿入する
///
/// - `mount_ids` は `mount_scope_range` invariant (16 桁 lowercase hex) を満たすこと
/// - `relative_path` 形式: `{mount_id}/{dir1}/{dir2}/.../{name}`
/// - kind は 20% directory / 80% image の比率
fn populate_corpus(indexer: &Indexer, count: usize, mount_ids: &[&str]) {
    let mut rng = Lcg::new(42);
    let conn = indexer.connect().unwrap();
    conn.execute("BEGIN", []).unwrap();
    for i in 0..count {
        let mount_id = mount_ids[i % mount_ids.len()];
        let depth = rng.next_range(1, 4);
        let mut parts: Vec<String> = Vec::with_capacity(depth + 2);
        parts.push(mount_id.to_string());
        for _ in 0..depth {
            let seg_len = sample_log_normal(&mut rng, 2.5, 0.4, 3, 20);
            parts.push(gen_segment(&mut rng, seg_len));
        }
        let name_len = sample_log_normal(&mut rng, 3.0, 0.5, 4, 80);
        let name = gen_segment(&mut rng, name_len);
        parts.push(name.clone());
        let rel_path = parts.join("/");
        let kind = if i % 5 == 0 { "directory" } else { "image" };
        conn.execute(
            "INSERT OR REPLACE INTO entries \
             (relative_path, name, kind, size_bytes, mtime_ns) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![rel_path, name, kind, 1024_i64, 1_000_000_000_i64],
        )
        .unwrap();
    }
    conn.execute("COMMIT", []).unwrap();
}

// --- 測定 ---

/// 単一シナリオの P50 / P95 / サンプル平均
#[derive(Debug)]
struct Measurement {
    label: String,
    samples: Vec<Duration>,
    median: Duration,
    p95: Duration,
    mean: Duration,
    hits: usize,
}

impl Measurement {
    fn print(&self) {
        eprintln!(
            "  [{:<40}] median={:>6.2}ms p95={:>6.2}ms mean={:>6.2}ms hits={}",
            self.label,
            self.median.as_secs_f64() * 1000.0,
            self.p95.as_secs_f64() * 1000.0,
            self.mean.as_secs_f64() * 1000.0,
            self.hits
        );
    }
}

/// 指定クエリを `samples` 回実行して P50/P95 を算出
fn measure(
    indexer: &Indexer,
    label: &str,
    query: &str,
    scope_prefix: Option<&str>,
    samples: usize,
) -> Measurement {
    // warm-up 3 回（初回の statement cache 構築・DB ページ読み込みを除外）
    for _ in 0..3 {
        let _ = indexer
            .search(&SearchParams {
                query,
                kind: None,
                limit: 50,
                offset: 0,
                scope_prefix,
                order: SearchOrder::Relevance,
            })
            .unwrap();
    }

    let mut timings = Vec::with_capacity(samples);
    let mut last_hits = 0;
    for _ in 0..samples {
        let start = Instant::now();
        let (hits, _) = indexer
            .search(&SearchParams {
                query,
                kind: None,
                limit: 50,
                offset: 0,
                scope_prefix,
                order: SearchOrder::Relevance,
            })
            .unwrap();
        timings.push(start.elapsed());
        last_hits = hits.len();
    }
    timings.sort_unstable();

    let median = timings[samples / 2];
    // P95: 95th percentile (sample size 30 → index 28)
    let p95_idx = ((samples as f64) * 0.95).ceil() as usize - 1;
    let p95 = timings[p95_idx.min(samples - 1)];
    let sum: Duration = timings.iter().sum();
    let mean = sum / samples as u32;

    Measurement {
        label: label.to_string(),
        samples: timings,
        median,
        p95,
        mean,
        hits: last_hits,
    }
}

/// EXPLAIN QUERY PLAN の結果を 1 行文字列にまとめる
fn explain_query_plan(
    conn: &Connection,
    sql: &str,
    bind: &[&dyn rusqlite::types::ToSql],
) -> String {
    let eq_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&eq_sql).unwrap();
    let rows = stmt
        .query_map(bind, |row| {
            // EXPLAIN QUERY PLAN: id, parent, notused, detail
            row.get::<_, String>(3)
        })
        .unwrap();
    rows.filter_map(Result::ok).collect::<Vec<_>>().join(" | ")
}

// --- シナリオ ---

struct ScenarioConfig {
    corpus_size: usize,
    samples: usize,
    mount_ids: [&'static str; 2],
}

#[allow(
    clippy::too_many_lines,
    reason = "bench は測定 + 出力を一貫して行うため分割メリットが薄い"
)]
fn run_scenario(config: &ScenarioConfig) {
    eprintln!(
        "\n=== corpus={} entries × {} mounts, samples={} ===",
        config.corpus_size,
        config.mount_ids.len(),
        config.samples
    );

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let indexer = Indexer::new(tmp.path().to_str().unwrap());
    indexer.init_db().unwrap();

    let populate_start = Instant::now();
    populate_corpus(&indexer, config.corpus_size, &config.mount_ids);
    eprintln!("  populate: {:.2}s", populate_start.elapsed().as_secs_f64());

    // DB サイズ (PRAGMA page_count * page_size)
    let conn = indexer.connect().unwrap();
    let page_count: i64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap();
    let page_size: i64 = conn
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .unwrap();
    eprintln!(
        "  db_size: {:.2} MB ({} pages × {} bytes)",
        (page_count * page_size) as f64 / (1024.0 * 1024.0),
        page_count,
        page_size
    );

    // EXPLAIN QUERY PLAN 出力
    let (scope_lo, scope_hi) = crate::services::path_keys::prefix_scope_range(config.mount_ids[0]);
    let like_pattern = "%ab%";
    eprintln!("\n  --- EXPLAIN QUERY PLAN (2-char LIKE fallback) ---");
    let sql_no_scope = "SELECT relative_path, name, kind, size_bytes FROM entries WHERE 1=1 AND (name LIKE ?1 ESCAPE '\\' OR relative_path LIKE ?1 ESCAPE '\\') LIMIT ?2 OFFSET ?3";
    let plan = explain_query_plan(&conn, sql_no_scope, &[&like_pattern, &50_i64, &0_i64]);
    eprintln!("  scope=None : {plan}");

    let sql_scope = "SELECT relative_path, name, kind, size_bytes FROM entries WHERE 1=1 AND (name LIKE ?1 ESCAPE '\\' OR relative_path LIKE ?1 ESCAPE '\\') AND relative_path >= ?2 AND relative_path < ?3 LIMIT ?4 OFFSET ?5";
    let plan = explain_query_plan(
        &conn,
        sql_scope,
        &[&like_pattern, &scope_lo, &scope_hi, &50_i64, &0_i64],
    );
    eprintln!("  scope=mount: {plan}");

    // 6 シナリオ × samples 回
    eprintln!("\n  --- Measurements ---");
    let queries = [
        ("写真", "jp-hiragana-2"),
        ("ab", "ascii-lower-2"),
        ("AB", "ascii-upper-2"),
    ];
    let mut measurements = Vec::new();
    for (q, tag) in &queries {
        let m_none = measure(
            &indexer,
            &format!("{tag} scope=None"),
            q,
            None,
            config.samples,
        );
        m_none.print();
        measurements.push(m_none);

        let m_scope = measure(
            &indexer,
            &format!("{tag} scope=mount"),
            q,
            Some(config.mount_ids[0]),
            config.samples,
        );
        m_scope.print();
        measurements.push(m_scope);
    }

    // abort gate 判定
    let threshold_ms = 30.0_f64;
    let exceeded: Vec<_> = measurements
        .iter()
        .filter(|m| m.p95.as_secs_f64() * 1000.0 >= threshold_ms)
        .collect();
    eprintln!("\n  --- Abort gate (P95 < {threshold_ms}ms per scenario) ---");
    if exceeded.is_empty() {
        eprintln!(
            "  ALL PASS ({} scenarios) — Phase 1 を skip 可能",
            measurements.len()
        );
    } else {
        eprintln!(
            "  FAIL: {}/{} シナリオが P95 >= {}ms",
            exceeded.len(),
            measurements.len(),
            threshold_ms
        );
        for m in &exceeded {
            eprintln!(
                "    - {}: p95={:.2}ms",
                m.label,
                m.p95.as_secs_f64() * 1000.0
            );
        }
    }

    // 個別サンプル検証（外れ値確認用）
    if let Some(worst) = measurements.iter().max_by_key(|m| m.p95.as_nanos()) {
        let max = worst.samples.last().unwrap();
        let min = worst.samples.first().unwrap();
        eprintln!(
            "\n  最悪シナリオ ({}) min={:.2}ms max={:.2}ms",
            worst.label,
            min.as_secs_f64() * 1000.0,
            max.as_secs_f64() * 1000.0
        );
    }
}

// --- テスト entry points (#[ignore] で通常の cargo test では実行されない) ---

#[test]
#[ignore = "baseline bench (long-running); use --release --ignored"]
fn phase_0_baseline_1k() {
    run_scenario(&ScenarioConfig {
        corpus_size: 1_000,
        samples: 30,
        mount_ids: ["0123456789abcdef", "fedcba9876543210"],
    });
}

#[test]
#[ignore = "baseline bench (long-running); use --release --ignored"]
fn phase_0_baseline_10k() {
    run_scenario(&ScenarioConfig {
        corpus_size: 10_000,
        samples: 30,
        mount_ids: ["0123456789abcdef", "fedcba9876543210"],
    });
}

#[test]
#[ignore = "baseline bench (long-running); use --release --ignored"]
fn phase_0_baseline_100k() {
    run_scenario(&ScenarioConfig {
        corpus_size: 100_000,
        samples: 30,
        mount_ids: ["0123456789abcdef", "fedcba9876543210"],
    });
}
