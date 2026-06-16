//! Report generation and output formatting

use crate::metrics::BenchmarkSummary;
use tracing::info;

// ─────────────────────── M0 pipeline profiling (docs/feat/m0-pipeline-baseline/) ───────────────────────

/// Reset the validator's M0 measurement window.
pub async fn m0_reset(validator_url: &str) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/m0/reset", validator_url.trim_end_matches('/'));
    let resp = reqwest::Client::new().post(&url).send().await?;
    anyhow::ensure!(resp.status().is_success(), "reset HTTP {}", resp.status());
    Ok(())
}

/// Fetch the validator's per-stage M0 report (jsonl, one stage per line).
pub async fn m0_fetch(validator_url: &str) -> anyhow::Result<String> {
    let url = format!("{}/api/v1/m0/report", validator_url.trim_end_matches('/'));
    let resp = reqwest::Client::new().get(&url).send().await?;
    anyhow::ensure!(resp.status().is_success(), "report HTTP {}", resp.status());
    Ok(resp.text().await?)
}

/// One stage's stats parsed from the validator's jsonl report.
#[derive(serde::Deserialize)]
struct M0Stage {
    stage: String,
    count: u64,
    mean_ns: u64,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
    max_ns: u64,
}

/// Canonical pipeline order for rendering (stages absent from the report are skipped).
const M0_ORDER: &[&str] = &[
    "ingress", "reserve", "prep", "route", "dispatch", "tee", "submit",
    "fold_wait", "fold_work", "vote", "apply_wait", "apply_work", "commit",
];

/// Render the per-stage p50/p95/p99 breakdown as a table, in pipeline order,
/// flagging the serial-wait stages (apply_wait / fold_wait) that drive the
/// A1 / A2 decision (design D6).
pub fn print_m0_report(jsonl: &str) {
    let mut stages: Vec<M0Stage> = jsonl
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<M0Stage>(l).ok())
        .collect();

    if stages.is_empty() {
        info!("M0: report is empty (no samples — was load applied with m0-profiling on?)");
        return;
    }

    // Sort into canonical pipeline order.
    stages.sort_by_key(|s| {
        M0_ORDER.iter().position(|n| *n == s.stage).unwrap_or(usize::MAX)
    });

    let us = |ns: u64| ns as f64 / 1000.0;
    info!("╔══════════════════════════════════════════════════════════╗");
    info!("║              M0 PER-STAGE LATENCY (µs)                   ║");
    info!("╚══════════════════════════════════════════════════════════╝");
    info!("{:<12} {:>8} {:>10} {:>10} {:>10} {:>10}  {}", "stage", "count", "p50", "p95", "p99", "max", "");
    for s in &stages {
        let flag = if s.stage == "apply_wait" || s.stage == "fold_wait" {
            "  <- serial wait (A1/A2 signal)"
        } else {
            ""
        };
        info!(
            "{:<12} {:>8} {:>10.1} {:>10.1} {:>10.1} {:>10.1}{}",
            s.stage, s.count, us(s.p50_ns), us(s.p95_ns), us(s.p99_ns), us(s.max_ns), flag
        );
        let _ = s.mean_ns; // mean available in jsonl; p-values are the decision signal
    }
}

/// Print benchmark report
pub fn print_report(summary: &BenchmarkSummary) {
    info!("╔══════════════════════════════════════════════════════════╗");
    info!("║                    BENCHMARK RESULTS                     ║");
    info!("╚══════════════════════════════════════════════════════════╝");
    info!("");
    
    // Overview
    info!("┌─────────────────────────────────────────────────────────┐");
    info!("│ OVERVIEW                                                │");
    info!("├─────────────────────────────────────────────────────────┤");
    info!("│ Total Requests:     {:>10}                          │", summary.total_requests);
    info!("│ Successful:         {:>10}                          │", summary.success_count);
    info!("│ Failed:             {:>10}                          │", summary.failure_count);
    info!("│ Timeout:            {:>10}                          │", summary.timeout_count);
    info!("│ Duration:           {:>10.2}s                         │", summary.elapsed_ms as f64 / 1000.0);
    info!("└─────────────────────────────────────────────────────────┘");
    info!("");

    // Performance
    info!("┌─────────────────────────────────────────────────────────┐");
    info!("│ PERFORMANCE                                             │");
    info!("├─────────────────────────────────────────────────────────┤");
    info!("│ ★ TPS (Successful): {:>10.2}                         │", summary.tps);
    info!("│ Success Rate:       {:>10.2}%                        │", summary.success_rate);
    info!("└─────────────────────────────────────────────────────────┘");
    info!("");

    // Latency
    info!("┌─────────────────────────────────────────────────────────┐");
    info!("│ LATENCY (milliseconds)                                  │");
    info!("├─────────────────────────────────────────────────────────┤");
    info!("│ Min:                {:>10.2} ms                       │", summary.latency.min_us as f64 / 1000.0);
    info!("│ Max:                {:>10.2} ms                       │", summary.latency.max_us as f64 / 1000.0);
    info!("│ Mean:               {:>10.2} ms                       │", summary.latency.mean_us / 1000.0);
    info!("│ P50 (Median):       {:>10.2} ms                       │", summary.latency.p50_us as f64 / 1000.0);
    info!("│ P90:                {:>10.2} ms                       │", summary.latency.p90_us as f64 / 1000.0);
    info!("│ P95:                {:>10.2} ms                       │", summary.latency.p95_us as f64 / 1000.0);
    info!("│ P99:                {:>10.2} ms                       │", summary.latency.p99_us as f64 / 1000.0);
    info!("│ P99.9:              {:>10.2} ms                       │", summary.latency.p999_us as f64 / 1000.0);
    info!("└─────────────────────────────────────────────────────────┘");
    info!("");

    // Summary line
    let status_emoji = if summary.success_rate > 99.0 {
        "✅"
    } else if summary.success_rate > 95.0 {
        "⚠️"
    } else {
        "❌"
    };

    info!(
        "{} Final TPS: {:.2} | Success Rate: {:.2}% | P99 Latency: {:.2}ms",
        status_emoji,
        summary.tps,
        summary.success_rate,
        summary.latency.p99_us as f64 / 1000.0
    );
}

/// Generate JSON report
pub fn json_report(summary: &BenchmarkSummary) -> String {
    serde_json::json!({
        "total_requests": summary.total_requests,
        "success_count": summary.success_count,
        "failure_count": summary.failure_count,
        "timeout_count": summary.timeout_count,
        "elapsed_ms": summary.elapsed_ms,
        "tps": summary.tps,
        "success_rate": summary.success_rate,
        "latency": {
            "min_ms": summary.latency.min_us as f64 / 1000.0,
            "max_ms": summary.latency.max_us as f64 / 1000.0,
            "mean_ms": summary.latency.mean_us / 1000.0,
            "p50_ms": summary.latency.p50_us as f64 / 1000.0,
            "p90_ms": summary.latency.p90_us as f64 / 1000.0,
            "p95_ms": summary.latency.p95_us as f64 / 1000.0,
            "p99_ms": summary.latency.p99_us as f64 / 1000.0,
            "p999_ms": summary.latency.p999_us as f64 / 1000.0,
        }
    }).to_string()
}

/// Generate CSV header
#[allow(dead_code)]
pub fn csv_header() -> &'static str {
    "timestamp,total_requests,success_count,failure_count,timeout_count,elapsed_ms,tps,success_rate,latency_min_ms,latency_max_ms,latency_mean_ms,latency_p50_ms,latency_p90_ms,latency_p95_ms,latency_p99_ms,latency_p999_ms"
}

/// Generate CSV row
#[allow(dead_code)]
pub fn csv_row(summary: &BenchmarkSummary) -> String {
    format!(
        "{},{},{},{},{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
        chrono::Utc::now().to_rfc3339(),
        summary.total_requests,
        summary.success_count,
        summary.failure_count,
        summary.timeout_count,
        summary.elapsed_ms,
        summary.tps,
        summary.success_rate,
        summary.latency.min_us as f64 / 1000.0,
        summary.latency.max_us as f64 / 1000.0,
        summary.latency.mean_us / 1000.0,
        summary.latency.p50_us as f64 / 1000.0,
        summary.latency.p90_us as f64 / 1000.0,
        summary.latency.p95_us as f64 / 1000.0,
        summary.latency.p99_us as f64 / 1000.0,
        summary.latency.p999_us as f64 / 1000.0,
    )
}
