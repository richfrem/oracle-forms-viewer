---
name: converting-oracle-artifacts
description: "Converts Oracle Forms (.fmb, .mmb, .olb to and from XML), PL/SQL libraries (.pll to .pld), and Reports (.rdf and XML) with Oracle's own tools in the Forms 14c container image. Use when extracting form ASTs, decompiling binary forms, converting XML back to binary forms or menus, exporting library source, inspecting reports, or checking which conversion paths work."
---

# Converting Oracle Artifacts

Runs Oracle's own conversion tools inside the Forms 14c container image (`forms-converter-14c:latest`, Oracle Home mounted at `/oracle_home_14c`). It works on macOS, Linux, WSL, and Windows PowerShell (through WSL). On Windows, run everything inside WSL2 (`wsl -d Ubuntu`), like the rest of the testbed.

## Contents

- [Quick start](#quick-start)
- [Scripts](#scripts)
- [Verified conversion paths](#verified-conversion-paths)
- [Windows and WSL notes](#windows-and-wsl-notes)
- [Known limits](#known-limits)

## Quick start

Run from the repo root. The first command checks that Podman and the image are ready; the second runs every conversion path and prints a table:

```bash
python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/convert_fmb.py --check
python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/test_conversions.py
```

Convert one file (the output goes next to the input unless `-o` is given):

```bash
python3 .../scripts/convert_fmb.py form.fmb        # also .mmb and .olb -> <name>_<ext>.xml
python3 .../scripts/convert_pll.py lib.pll         # -> lib.pld (PL/SQL source text)
python3 .../scripts/convert_rpt.py report.rdf      # -> report_rdf.xml (needs libnsl2 in the image)
```

## Scripts

| Script | Purpose |
|---|---|
| `converter_runtime.py` | Shared settings: image name, Oracle Home path in the container, the Podman command for macOS, WSL, or Windows, and the `--check` readiness probe. Set `OFW_CONVERTER_IMAGE=forms-converter:latest` to use the older 12c image (its Oracle Home path is `/oracle_home`). |
| `convert_fmb.py` | `.fmb`, `.mmb`, `.olb` to XML (Forms2XML). |
| `convert_pll.py` | `.pll` to `.pld`. |
| `convert_rpt.py` | `.rdf` to XML or REX (`rwconverter`). |
| `test_conversions.py` | Runs every path for the five types and prints a PASS, FAIL, BLOCKED, or SKIP table. Options: `--only forms|menus|libraries|olb|reports`, `--offline`. Test files go in the git-ignored `temp/conversion-tests/`. |

Paths that have no script yet (XML to FMB, MMB, and OLB; menu compile; XML to RDF) run as direct container commands inside `test_conversions.py`. XML to binary uses `oracle.forms.util.xmltools.XML2Forms` from `jlib/frmxmltools.jar`.

## Verified conversion paths

Tested on Windows 11 with everything inside WSL2, Forms 14.1.2 image, 2026-11-14, with `test_conversions.py` (9 PASS, 0 FAIL, 2 BLOCKED, 1 SKIP). macOS was not re-run for these results.

| Type | Path | Status | Notes |
|---|---|---|---|
| Forms | FMB to XML | PASS | `convert_fmb.py` |
| Forms | XML to FMB | PASS | XML2Forms |
| Forms | Compile FMB to FMX | PASS | `compile_form.py` in the compiling skill; needs the database unless `--offline` |
| Menus | MMB to XML | PASS | `convert_fmb.py` |
| Menus | XML to MMB | PASS | XML2Forms |
| Menus | Compile MMB to MMX | PASS | `frmcmp_batch module_type=menu batch=yes` |
| Libraries | PLL to PLD | PASS | `convert_pll.py`; needs `batch=yes` on 14c |
| Libraries | PLD to PLL | BLOCKED | See Known limits |
| OLB | OLB to XML | PASS | `convert_fmb.py` |
| OLB | XML to OLB | PASS | With 14c-format XML (see Known limits) |
| Reports | XML to RDF | BLOCKED | `rwconverter` needs `libnsl.so.2` (see Known limits) |
| Reports | RDF to XML | SKIP | Needs an `.rdf`, which the step above produces |

## Windows and WSL notes

- Run inside WSL (`wsl -d Ubuntu`) from the repo root. The scripts also work from Windows PowerShell; they call Podman through `wsl -d <distro>` (`OFW_WSL_DISTRO`, default `Ubuntu`).
- On Windows and WSL, `podman run` uses `--user root`. Rootless Podman maps the container's `oracle` user to a UID that cannot write on the Windows drive (`/mnt/c`); container root maps to your user.
- `convert_fmb.py` runs Forms2XML as a Java class with `-Djava.library.path=/oracle_home_14c/lib` and `FORMS_API_TK_BYPASS=TRUE`. In 14c, `bin/frmf2xml.sh` and the other `bin/*.sh` batch scripts are templates with `$$placeholder$$` values, not runnable files, so the scripts call the Java class and the binaries directly.
- `bin/rwconverter.sh` is also a template; call `bin/rwconverter` directly.
- Keep test files under git-ignored `temp/`, never in `samples/`.

## Known limits

- **PLD to PLL:** on 14c, `frmcmp_batch` prints nothing and writes no `.pll` when given a `.pld` text file, even a real Oracle-exported one. `Forms2XML` rejects libraries (`File ... must be a FormModule, ObjectLibrary or MenuModule`), so there is no XML route. No command-line path was found; Forms Builder was not tried. `samples/utils.pld` is plain PL/SQL text, the same kind of file `frmcmp_batch script=yes` exports.
- **XML to OLB:** the older 12c sample XML is rejected by 14c. 14c-format XML (from OLB to XML) converts back fine.
- **Reports:** `bin/rwconverter` fails with `error while loading shared libraries: libnsl.so.2`. `Containerfile.14c` now lists `libnsl2`, but the image has to be rebuilt before the report paths can run. Not yet verified after a rebuild.
- **Not tested:** XLIFF, `.rpt`, and `.rex` conversions.
- The 14c `rwconverter` path inside `convert_rpt.py` is untested until the image is rebuilt.

## Verification

Run `test_conversions.py` after any change to the converter scripts or the image, and report the table it prints. Do not call a path working unless it shows PASS.
