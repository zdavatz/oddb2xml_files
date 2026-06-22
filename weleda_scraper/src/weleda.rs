//! Weleda Arzneimittel-Verzeichnis scraper.
//!
//! Two-step scrape:
//!   1. paginated listing  (?page=N&per-page=50)  -> id, name, Arzneiform,
//!      Grösse, Kategorie, Art.Nr., availability (signal aria-label = status)
//!   2. per-product detail (/details/drug/{id})   -> Pharmacode, EAN-Code,
//!      Zulassungsnummer, CSL-Code, or a "→ siehe <other>" cross-reference
//!      (verweis), which carries no pharma fields.
//!
//! The output `weleda_arzneimittel.csv` is REPLACED with exactly the products
//! currently listed on the site (delisted products are dropped). The on-disk
//! format (column order, UTF-8, CRLF, quote-only-when-necessary) is preserved
//! so oddb2xml keeps reading it unchanged.

use crate::{http_get, WELEDA_HEADER};
use anyhow::{Context, Result};
use scraper::{ElementRef, Html, Selector};
use std::collections::BTreeMap;
use std::path::Path;

/// One product as collected from the listing page.
#[derive(Debug, Clone)]
struct Listing {
    id: String,
    name: String,
    arzneiform: String,
    groesse: String,
    kategorie: String,
    artikelnummer: String,
    status: String,
}

/// Fields only available on the per-product detail page.
#[derive(Debug, Default, Clone)]
struct Detail {
    pharmacode: String,
    ean: String,
    zulassungsnummer: String,
    csl: String,
    verweis: String,
}

/// A fully assembled CSV row, in the column order of `WELEDA_HEADER`.
#[derive(Debug, Clone)]
struct Row {
    id: String,
    name: String,
    darreichung: String,
    status: String,
    verweis: String,
    artikelnummer: String,
    pharmacode: String,
    ean: String,
    abgabekategorie: String,
    zulassungsnummer: String,
    csl: String,
}

impl Row {
    fn fields(&self) -> [&str; 11] {
        [
            &self.id,
            &self.name,
            &self.darreichung,
            &self.status,
            &self.verweis,
            &self.artikelnummer,
            &self.pharmacode,
            &self.ean,
            &self.abgabekategorie,
            &self.zulassungsnummer,
            &self.csl,
        ]
    }
}

pub fn update(base: &str, lang: &str, cookie: &str, out_path: &str) -> Result<()> {
    let listings = scrape_listing(base, lang, cookie)?;
    eprintln!("Listing complete: {} products.", listings.len());

    eprintln!("Fetching {} detail pages...", listings.len());
    let mut rows: Vec<Row> = Vec::with_capacity(listings.len());
    for (i, l) in listings.iter().enumerate() {
        let url = format!("{base}/{lang}/arzneimittel/verzeichnis/details/drug/{}", l.id);
        let html = http_get(&url, cookie).with_context(|| format!("detail page id {}", l.id))?;
        let d = parse_detail(&html);
        rows.push(assemble(l, &d));
        if (i + 1) % 50 == 0 || i + 1 == listings.len() {
            eprintln!("  details {}/{}", i + 1, listings.len());
        }
    }

    report_diff(out_path, &rows)?;
    write_csv(out_path, &mut rows)?;
    eprintln!("Wrote {} rows to {out_path}", rows.len());
    Ok(())
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

fn scrape_listing(base: &str, lang: &str, cookie: &str) -> Result<Vec<Listing>> {
    let mut out: Vec<Listing> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    // The site clamps an over-large page number back to the last page (it does
    // not return an empty page), so we stop as soon as a page yields no new id.
    let mut page = 1usize;
    loop {
        let url = format!("{base}/{lang}/arzneimittel/verzeichnis?page={page}&per-page=50");
        let html = http_get(&url, cookie).with_context(|| format!("listing page {page}"))?;
        let entries = parse_listing_page(&html);
        let mut fresh = 0usize;
        for e in entries {
            if seen.insert(e.id.clone()) {
                out.push(e);
                fresh += 1;
            }
        }
        eprintln!("  listing page {page}: {fresh} new (total {})", out.len());
        if fresh == 0 {
            break;
        }
        page += 1;
        if page > 200 {
            eprintln!("  stopping: page guard reached");
            break;
        }
    }
    Ok(out)
}

fn parse_listing_page(html: &str) -> Vec<Listing> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("div.table__tr").unwrap();
    let th_sel = Selector::parse("div.table__th").unwrap();
    let td_sel = Selector::parse("div.table__td").unwrap();
    let link_sel = Selector::parse(r#"a[href*="/details/drug/"]"#).unwrap();
    let signal_sel = Selector::parse("span.signal").unwrap();

    let mut rows = Vec::new();
    for row in doc.select(&row_sel) {
        // Skip the header row (it holds .table__th cells) and any non-product row.
        if row.select(&th_sel).next().is_some() {
            continue;
        }
        let link = match row.select(&link_sel).next() {
            Some(a) => a,
            None => continue,
        };
        let id = match link.value().attr("href").and_then(last_path_segment) {
            Some(id) if !id.is_empty() => id,
            _ => continue,
        };
        let name = collapse_ws(&link.text().collect::<String>());

        let mut arzneiform = String::new();
        let mut groesse = String::new();
        let mut kategorie = String::new();
        let mut artikelnummer = String::new();
        for td in row.select(&td_sel) {
            let divs: Vec<ElementRef> = td
                .children()
                .filter_map(ElementRef::wrap)
                .filter(|e| e.value().name() == "div")
                .collect();
            if divs.len() < 2 {
                continue;
            }
            let label = collapse_ws(&divs[0].text().collect::<String>());
            let label = label.trim_end_matches(':').trim();
            let value = collapse_ws(&divs[1].text().collect::<String>());
            let value = clean_field(&value);
            match label {
                l if l.starts_with("Arzneiform") => arzneiform = value,
                l if l.starts_with("Gr") && l.contains("sse") => groesse = value, // Grösse
                l if l.starts_with("Kategorie") => kategorie = value,
                l if l.starts_with("Art") => artikelnummer = value, // Art.Nr.
                _ => {}
            }
        }

        // Cross-reference rows ("X → siehe Y") render only a Präparat cell whose
        // link points at the *target's* drug id, with no Art.Nr./pharma data.
        // They share an id with the real product row, so skip them (the real row
        // — which carries an Art.Nr. — is the canonical entry for that id).
        if artikelnummer.is_empty() {
            continue;
        }

        let status = row
            .select(&signal_sel)
            .next()
            .and_then(|s| s.value().attr("aria-label"))
            .map(normalize_status)
            .unwrap_or_default();

        rows.push(Listing {
            id,
            name,
            arzneiform,
            groesse,
            kategorie,
            artikelnummer,
            status,
        });
    }
    rows
}

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

fn parse_detail(html: &str) -> Detail {
    let doc = Html::parse_document(html);
    let mut d = Detail::default();

    // A cross-reference product ("→ siehe <other>") links to another drug
    // detail; the link text is the referenced product name. Normal products
    // carry no /details/drug/ link in their content.
    let link_sel = Selector::parse(r#"a[href*="/details/drug/"]"#).unwrap();
    if let Some(a) = doc.select(&link_sel).next() {
        d.verweis = collapse_ws(&a.text().collect::<String>());
    }

    // The pharma fields live in the .grid--2col block as "Label: value" divs.
    let field_sel = Selector::parse("div.grid--2col div").unwrap();
    for div in doc.select(&field_sel) {
        let text = collapse_ws(&div.text().collect::<String>());
        if let Some((label, value)) = text.split_once(':') {
            let value = clean_field(value);
            match label.trim() {
                "Pharmacode" => d.pharmacode = value,
                "EAN-Code" => d.ean = value,
                "Zulassungsnummer" => d.zulassungsnummer = value,
                "CSL-Code" => d.csl = value,
                _ => {}
            }
        }
    }
    d
}

fn assemble(l: &Listing, d: &Detail) -> Row {
    let darreichung = match (l.arzneiform.trim(), l.groesse.trim()) {
        (a, "") => a.to_string(),
        ("", g) => g.to_string(),
        (a, g) => format!("{a} {g}"),
    };
    // `verweis` is only meaningful for pure cross-reference entries that carry
    // no pharma data of their own (e.g. "Solutio Sacchari comp. D3 → siehe ...").
    // A normal product with its own Pharmacode/EAN may also link a "see also"
    // product; the original CSV leaves verweis empty for those, so we do too.
    let is_pointer = d.pharmacode.is_empty() && d.ean.is_empty();
    let verweis = if is_pointer { d.verweis.clone() } else { String::new() };
    Row {
        id: l.id.clone(),
        name: l.name.clone(),
        darreichung,
        status: l.status.clone(),
        verweis,
        artikelnummer: l.artikelnummer.clone(),
        pharmacode: d.pharmacode.clone(),
        ean: d.ean.clone(),
        abgabekategorie: l.kategorie.clone(),
        zulassungsnummer: d.zulassungsnummer.clone(),
        csl: d.csl.clone(),
    }
}

// ---------------------------------------------------------------------------
// CSV output (replace; format-preserving)
// ---------------------------------------------------------------------------

fn write_csv(out_path: &str, rows: &mut [Row]) -> Result<()> {
    // Sort by numeric id ascending (matches the existing file's ordering).
    rows.sort_by_key(|r| (r.id.parse::<u64>().unwrap_or(u64::MAX), r.id.clone()));

    let mut wtr = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .quote_style(csv::QuoteStyle::Necessary)
        .from_path(out_path)
        .with_context(|| format!("opening {out_path} for writing"))?;
    wtr.write_record(WELEDA_HEADER)?;
    for r in rows.iter() {
        wtr.write_record(r.fields())?;
    }
    wtr.flush()?;
    Ok(())
}

/// Print added / removed / updated counts against the current file, for the
/// operator's confidence. Does not affect what gets written.
fn report_diff(out_path: &str, new_rows: &[Row]) -> Result<()> {
    if !Path::new(out_path).exists() {
        eprintln!("No existing {out_path}; writing fresh file.");
        return Ok(());
    }
    let mut old: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut rdr = csv::Reader::from_path(out_path)
        .with_context(|| format!("reading existing {out_path}"))?;
    for rec in rdr.records() {
        let rec = rec?;
        if let Some(id) = rec.get(0) {
            old.insert(id.to_string(), rec.iter().map(|s| s.to_string()).collect());
        }
    }
    let new_ids: std::collections::HashSet<&str> =
        new_rows.iter().map(|r| r.id.as_str()).collect();

    let added = new_rows.iter().filter(|r| !old.contains_key(&r.id)).count();
    let removed = old.keys().filter(|id| !new_ids.contains(id.as_str())).count();
    let updated = new_rows
        .iter()
        .filter(|r| {
            old.get(&r.id)
                .map(|o| o.iter().map(String::as_str).ne(r.fields()))
                .unwrap_or(false)
        })
        .count();
    eprintln!(
        "Diff vs existing: {added} added, {removed} removed (delisted), {updated} updated, \
         {} unchanged.",
        new_rows.len() - added - updated
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn last_path_segment(href: &str) -> Option<String> {
    href.trim_end_matches('/')
        .rsplit('/')
        .next()
        .map(|s| s.to_string())
}

/// The site renders a missing value as a lone dash ("–"/"-"/"—"); the CSV
/// represents that as an empty field. Everything else is whitespace-normalised.
fn clean_field(s: &str) -> String {
    let c = collapse_ws(s);
    match c.as_str() {
        "–" | "-" | "—" => String::new(),
        _ => c,
    }
}

/// Collapse all runs of whitespace (incl. NBSP/newlines) to single spaces, trim.
fn collapse_ws(s: &str) -> String {
    s.replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The availability aria-label is the product status. The live feed renders an
/// empty reason as a trailing " ()" (e.g. "Wieder lieferbar ()"); the CSV omits
/// the empty parens. A non-empty reason is kept verbatim.
fn normalize_status(s: &str) -> String {
    // collapse_ws also turns the NBSP the site puts before "(reason)" into a
    // normal space, matching the CSV's plain-space form.
    let s = collapse_ws(s);
    if let Some(prefix) = s.strip_suffix("()") {
        return prefix.trim().to_string();
    }
    s
}
