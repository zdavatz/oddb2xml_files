//! Scraper that regenerates the data CSVs in the oddb2xml_files repository.
//!
//! Currently supported source: `weleda` — the Weleda Arzneimittel-Verzeichnis
//! (https://medical.weleda.ch/de/arzneimittel/verzeichnis), which backs
//! `weleda_arzneimittel.csv` (consumed by oddb2xml's chapter-70 / SL recovery).
//!
//! Usage:
//!   scraper --update weleda
//!
//! The directory is behind a login, so `--update weleda` asks for the session
//! cookie (PHPSESSID=...) on stdin. The cookie is used only for the HTTP
//! requests of this run and is never written to disk.

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::io::{self, Write};
use std::path::Path;

mod weleda;

/// Column order of weleda_arzneimittel.csv — must stay exactly as oddb2xml expects.
pub const WELEDA_HEADER: [&str; 11] = [
    "id",
    "name",
    "darreichung",
    "status",
    "verweis",
    "artikelnummer",
    "pharmacode",
    "ean",
    "abgabekategorie",
    "zulassungsnummer",
    "csl",
];

#[derive(Parser, Debug)]
#[command(
    name = "scraper",
    about = "Regenerate data CSVs in oddb2xml_files (currently: weleda)"
)]
struct Cli {
    /// Data source to update. Supported: weleda
    #[arg(long, value_name = "SOURCE")]
    update: String,

    /// Output CSV path. Defaults to weleda_arzneimittel.csv (auto-detected in
    /// the current dir or the parent dir when run from weleda_scraper/).
    #[arg(long)]
    out: Option<String>,

    /// Session cookie, e.g. "PHPSESSID=abc123". If omitted you are prompted.
    /// A bare value without '=' is treated as a PHPSESSID.
    #[arg(long)]
    cookie: Option<String>,

    /// Base site URL.
    #[arg(long, default_value = "https://medical.weleda.ch")]
    base: String,

    /// Language path segment (de or fr).
    #[arg(long, default_value = "de")]
    lang: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.update.as_str() {
        "weleda" => {
            let cookie = resolve_cookie(cli.cookie.as_deref())?;
            let out = cli
                .out
                .clone()
                .unwrap_or_else(|| default_weleda_out().to_string());
            weleda::update(&cli.base, &cli.lang, &cookie, &out)
        }
        other => bail!("unknown --update source {other:?} (supported: weleda)"),
    }
}

/// Pick the CSV oddb2xml_files keeps at its repo root, whether the binary is run
/// from the repo root or from the weleda_scraper/ sub-directory.
fn default_weleda_out() -> &'static str {
    for candidate in ["weleda_arzneimittel.csv", "../weleda_arzneimittel.csv"] {
        if Path::new(candidate).exists() {
            return candidate;
        }
    }
    "weleda_arzneimittel.csv"
}

/// Return a Cookie header value. From --cookie if given, otherwise prompt on
/// stdin. Never stored anywhere. An empty answer means "no cookie" (the request
/// still goes out, in case the directory is reachable without a session).
fn resolve_cookie(flag: Option<&str>) -> Result<String> {
    if let Some(c) = flag {
        return Ok(normalize_cookie(c));
    }
    eprint!(
        "Paste the medical.weleda.ch session cookie (e.g. PHPSESSID=...), \
         or press Enter to try without one: "
    );
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("reading cookie from stdin")?;
    Ok(normalize_cookie(line.trim()))
}

fn normalize_cookie(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() || raw.contains('=') {
        raw.to_string()
    } else {
        format!("PHPSESSID={raw}")
    }
}

/// Single shared HTTP GET helper. Sends a desktop User-Agent and the session
/// cookie (when present) and returns the response body as a String.
pub fn http_get(url: &str, cookie: &str) -> Result<String> {
    let mut req = ureq::get(url).header(
        "User-Agent",
        "Mozilla/5.0 (oddb2xml_files-scraper; +https://github.com/zdavatz/oddb2xml_files)",
    );
    if !cookie.is_empty() {
        req = req.header("Cookie", cookie);
    }
    let mut resp = req.call().with_context(|| format!("GET {url}"))?;
    resp.body_mut()
        .read_to_string()
        .with_context(|| format!("reading body of {url}"))
}
