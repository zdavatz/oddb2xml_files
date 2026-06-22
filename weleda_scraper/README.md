# oddb2xml_files scraper

A small Rust CLI that regenerates the data CSVs kept in this repository.

Currently supported source: **`weleda`** — the Weleda Arzneimittel-Verzeichnis
(<https://medical.weleda.ch/de/arzneimittel/verzeichnis>), which backs
[`weleda_arzneimittel.csv`](../weleda_arzneimittel.csv). That file is consumed
by [oddb2xml](https://github.com/zdavatz/oddb2xml)'s chapter-70 / SL recovery
(`lib/oddb2xml/weleda_sl.rb`).

## Build

```bash
cd weleda_scraper
cargo build --release
```

## Update weleda_arzneimittel.csv

The directory is behind a login, so the scraper asks for the session cookie on
stdin. **The cookie is used only for this run's HTTP requests and is never
written to disk.**

```bash
# from the repository root (so the default --out path resolves to ./weleda_arzneimittel.csv):
./weleda_scraper/target/release/scraper --update weleda
# Paste the medical.weleda.ch session cookie (e.g. PHPSESSID=...) when prompted.
```

### Getting the cookie

Log in at <https://medical.weleda.ch> in your browser, then copy the
`PHPSESSID` cookie value (DevTools → Application/Storage → Cookies). You can
paste either `PHPSESSID=<value>` or just the bare `<value>`.

### Options

| flag | default | meaning |
|------|---------|---------|
| `--update <SOURCE>` | — | data source; currently only `weleda` |
| `--out <PATH>` | auto (`weleda_arzneimittel.csv`, in `.` or `..`) | output CSV path |
| `--cookie <COOKIE>` | prompt | session cookie, to avoid the stdin prompt |
| `--base <URL>` | `https://medical.weleda.ch` | site base URL |
| `--lang <de\|fr>` | `de` | language path segment |

## What it does

1. Walks the paginated listing (`?page=N&per-page=50`) collecting, per product:
   `id` (from `…/details/drug/{id}`), `name`, Arzneiform, Grösse, Kategorie
   (→ `abgabekategorie`), Art.Nr. (`artikelnummer`) and availability (the signal
   `aria-label`, used as `status`). Cross-reference rows ("X → siehe Y", which
   carry no Art.Nr. and reuse the target's id) are skipped.
2. Fetches each product's detail page (`…/details/drug/{id}`) for `pharmacode`,
   `ean`, `zulassungsnummer`, `csl`. A pure pointer entry (no pharma data)
   instead records its `verweis` target.
3. **Replaces** `weleda_arzneimittel.csv` with exactly the products currently
   listed (delisted products are dropped). The on-disk format is preserved
   byte-for-byte: same column order, UTF-8, CRLF line endings, quote-only-when-
   necessary, sorted by numeric `id`. The site's `–` "no value" placeholder is
   written as an empty field.

The run prints a diff summary (added / removed / updated / unchanged) to stderr
before writing.

## CSV columns

```
id,name,darreichung,status,verweis,artikelnummer,pharmacode,ean,abgabekategorie,zulassungsnummer,csl
```
