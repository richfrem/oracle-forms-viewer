# Oracle Reports: what is available and how to run it

Reports (`.rdf`, `.rpt`, `.rex`, and Reports XML) can be handled two ways. Use the first when you need Oracle's own output, the second when you do not want any Oracle software.

## 1. Oracle's `rwconverter` (Forms 14c install, in the container)

The 14c "Forms and Reports Deployment" install includes the Reports tools: `bin/rwconverter`, `rwbuilder`, `rwserver`, `rwrun`. (The older 12c "Standalone Forms Builder" install did **not**: it excludes Reports, and the 12c "Forms and Reports Deployment" type can only be installed on top of an existing Fusion Middleware Infrastructure with WebLogic; otherwise it stops with `INST-07468`. See [legacy-12c.md](legacy-12c.md).)

What to know:
- `bin/rwconverter.sh` is an unfilled template (`$$domain_home$$`). **Call `bin/rwconverter` directly**, as `convert_rpt.py` does.
- It needs `libnsl.so.2`, which the image gets from the `libnsl2` package in `Containerfile.14c` (rebuild the image if `rwconverter` fails with `error while loading shared libraries: libnsl.so.2`).
- `apply_forms14c_config.sh` step 5b creates the Reports preference and printer files `rwconverter` expects (`tools/admin/prefs.ora`, `guicommon/tk/admin/uiprint.txt` and `uiscreenprint.txt`). That step was added to the script by someone other than the agent; why and what it fixed were not recorded, so treat it as **not verified**.
- Run it through the skill: `python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/convert_rpt.py report.rdf` (to `report_rdf.xml`, or `-f rex`), or test every path with `python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/test_conversions.py --only reports`.
- XML to RDF and back: `test_conversions.py` runs `rwconverter stype=xmlfile ... dtype=rdffile` and `stype=rdffile ... dtype=xmlfile`. The two report paths were reported as `2 PASS` by the owner after the image gained `libnsl2`; the agent saw only that last line, not the table, so confirm by running it.
- Validate report XML before converting: `validate_forms_xml.py` checks it against `reports.dtd` (Oracle's Reports DTD; Reports XML has a DOCTYPE and no XSD).

## 2. The native Rust engine (no Oracle software)

A binary Oracle Reports file is not an alien format: `.rdf` uses the same `ROS 60050` container as Forms (`.fmb`, `.mmb`, `.olb`). The Rust crates (`oracle-forms-binary`, `oracle-forms-cli`) read `.rdf` files with no JVM, no WebLogic, and no Oracle runtime. Program units, parameter triggers (`BEFOREPFORM`, `AFTERPFORM`), and formula columns (`CF_*`) are extracted directly:

```bash
cargo run -p oracle-forms-server                                           # the web app
cargo run -p oracle-forms-cli -- inspect /path/to/report.rdf
cargo run -p oracle-forms-cli -- dump-sql /path/to/report.rdf -o ./extracted-sql
cargo run -p oracle-forms-cli -- report inspect samples/ofw_employee_directory_report.xml
cargo run -p oracle-forms-cli -- report run samples/ofw_employee_directory_report.xml --backend mock -o ./report.html
```

The native engine covers the modeled data definition and an HTML preview; it does not reproduce the full Oracle paper layout.

## Reports Server checks (WebLogic)

If a Reports server component is deployed (`WLS_REPORTS` on port 9001 in a full domain), see the Reports section of [forms-and-reports-testing-guide.md](forms-and-reports-testing-guide.md): ping, job queue, a PDF run, and headless `rwconverter` conversion.
