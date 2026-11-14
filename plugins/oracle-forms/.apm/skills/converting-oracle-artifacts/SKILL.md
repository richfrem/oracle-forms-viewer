---
name: converting-oracle-artifacts
description: "Converts Oracle Forms (.fmb, .mmb, .olb to and from XML), PL/SQL libraries (.pll to .pld), and Reports (.rdf and XML) with Oracle's own tools in the Forms 14c container image, and extracts the Forms XML schema from the install. Use when extracting form ASTs, decompiling binary forms, converting XML back to binary forms or menus, exporting library source, inspecting reports, or checking which conversion paths work."
---

# Converting Oracle Artifacts

Runs Oracle's own conversion tools inside the Forms 14c container image (`forms-converter-14c:latest`, Oracle Home mounted at `/oracle_home_14c`). Works on macOS, Linux, WSL, and Windows PowerShell (through WSL). On Windows run everything inside WSL2.

## Contents

- [Constraints](#constraints)
- [Quick start](#quick-start)
- [Workflow](#workflow)
- [Scripts](#scripts)
- [Verified conversion paths](#verified-conversion-paths)
- [Verification](#verification)

## Constraints

1. Needs the testbed image and Oracle Home from `setting-up-oracle-testbed`; check with `convert_fmb.py --check`.
2. Test on copies under git-ignored `temp/`, never in `samples/`.
3. In 14c the `bin/*.sh` wrappers are templates; the scripts call the Java classes and binaries directly. Details and limits: [references/conversion-notes.md](references/conversion-notes.md).
4. Validate hand-written XML before XML to binary (`validate_forms_xml.py`, `compiling-and-running-oracle-forms` skill). Do not call a path working unless `test_conversions.py` shows PASS.

## Quick start

From the repo root:

```bash
python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/convert_fmb.py --check
python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/test_conversions.py
```

## Workflow

1. **Binary to XML:** `convert_fmb.py form.fmb` (also `.mmb`, `.olb`; output `<name>_<ext>.xml`); `convert_pll.py lib.pll` (to `lib.pld`); `convert_rpt.py report.rdf` (to `report_rdf.xml`).
2. **XML to binary:** run XML2Forms in the container (`oracle.forms.util.xmltools.XML2Forms`); `test_conversions.py` shows the exact command per type.
3. **Check the schema** when a conversion rejects XML: `references/forms-xml-schema.md` (what it is, where it came from) and `extract_forms_schema.py` (re-extract and compare).
4. **Test every path** with `test_conversions.py` (`--only forms|menus|libraries|olb|reports`, `--offline`) and report its table.

## Scripts

| Script | Purpose |
|---|---|
| `converter_runtime.py` | Shared: image name, Oracle Home path (`OFW_ORACLE_HOME`), Podman command for macOS, WSL, Windows, and the `--check` probe. `OFW_CONVERTER_IMAGE=forms-converter:latest` selects the 12c image |
| `convert_fmb.py`, `convert_pll.py`, `convert_rpt.py` | Forms/menus/object libraries, PL/SQL libraries, and Reports conversions |
| `test_conversions.py` | Runs every path for the five types and prints a PASS, FAIL, BLOCKED, SKIP table |
| `extract_forms_schema.py` | Runs Oracle's `SchemaGenerator` in a container, compares the result with `assets/forms-14c.xsd`, `--update` refreshes the copies. **Not verified:** the generator's arguments (`--usage`) |

## Verified conversion paths

Windows 11 in WSL2, 14.1.2 image, 2026-11-14: 9 PASS, 2 BLOCKED (PLD to PLL; XML to RDF, since fixed by the `libnsl2` image rebuild, owner reported 2 PASS), 1 SKIP. macOS not re-run.

| Type | Path | Status |
|---|---|---|
| Forms | FMB to XML, XML to FMB, compile FMB to FMX | PASS |
| Menus | MMB to XML, XML to MMB, compile MMB to MMX | PASS |
| Libraries | PLL to PLD | PASS |
| Libraries | PLD to PLL | BLOCKED: no command-line route on 14c |
| OLB | OLB to XML, XML to OLB (14c-format XML) | PASS |
| Reports | XML to RDF, RDF to XML | Reported PASS by the owner; re-run to confirm |

## Verification

Run `test_conversions.py` after any change to the converter scripts or the image and report the table it prints. Eval cases: `evals/evals.json`.
