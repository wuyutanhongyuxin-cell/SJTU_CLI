//! `sjtu jwc today / week / next` clap 解析自检。

use clap::Parser;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    sub: sjtu_cli::cli::jwc::JwcSub,
}

#[test]
fn today_parses_with_no_args() {
    let _ = Cli::parse_from(["sjtu", "today"]);
}

#[test]
fn today_parses_with_grid_flag() {
    let _ = Cli::parse_from(["sjtu", "today", "--grid"]);
}

#[test]
fn week_parses_with_zs_and_xnm() {
    let _ = Cli::parse_from(["sjtu", "week", "--zs", "14", "--xnm", "2025"]);
}

#[test]
fn week_rejects_zs_0() {
    let r = Cli::try_parse_from(["sjtu", "week", "--zs", "0"]);
    assert!(r.is_err(), "--zs 0 应被 clap range 拒绝");
}

#[test]
fn week_rejects_zs_19() {
    let r = Cli::try_parse_from(["sjtu", "week", "--zs", "19"]);
    assert!(r.is_err(), "--zs 19 应被 clap range 拒绝");
}

#[test]
fn next_parses_with_within_and_limit() {
    let _ = Cli::parse_from(["sjtu", "next", "--within", "7", "--limit", "20"]);
}

#[test]
fn next_rejects_within_32() {
    let r = Cli::try_parse_from(["sjtu", "next", "--within", "32"]);
    assert!(r.is_err(), "--within 32 应被 clap range 拒绝");
}
