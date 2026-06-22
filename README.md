oddb2xml_files
==============

Die Daten wurden aus den folgenden Files extrahiert:

* Non-Pharma, gemeldet von der Pharmafirma.
* [Betäubungsmittel](http://www.swissmedic.ch/produktbereiche/00447/00536/index.html?lang=de&download=NHzLpZeg7t,lnp6I0NTU042l2Z6ln1acy4Zn4Z2qZpnO2Yuq2Z6gpJCDdH1,fWym162epYbg2c_JjKbNoKSn6A--&.pdf).
* [LPPV](http://www.lppa.ch/frn/textes/LPPA_07_2013.pdf).

Scraper
-------

`weleda_arzneimittel.csv` wird mit dem Rust-Tool in [`weleda_scraper/`](weleda_scraper/)
aus dem Weleda Arzneimittel-Verzeichnis (<https://medical.weleda.ch/de/arzneimittel/verzeichnis>)
neu generiert:

```bash
cd weleda_scraper && cargo build --release
./target/release/scraper --update weleda   # fragt nach dem Session-Cookie (PHPSESSID)
```

Details (Optionen, Cookie, Format) siehe [`weleda_scraper/README.md`](weleda_scraper/README.md).
Die Datei wird von [oddb2xml](https://github.com/zdavatz/oddb2xml) (`lib/oddb2xml/weleda_sl.rb`)
zur Laufzeit von hier (master) geladen.
