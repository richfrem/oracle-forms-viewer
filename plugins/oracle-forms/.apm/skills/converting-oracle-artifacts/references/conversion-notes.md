# Conversion notes: Windows and WSL, known limits

Details behind the converting skill's `SKILL.md`. Results are from `test_conversions.py` on Windows 11 with everything in WSL2, Forms 14.1.2 image, 2026-11-14. macOS was not re-run.

## Contents

- [Windows and WSL notes](#windows-and-wsl-notes)
- [Known limits](#known-limits)
- [Schema and validation](#schema-and-validation)

## Windows and WSL notes

- Run inside WSL (`wsl -d Ubuntu`) from the repo root. The scripts also work from Windows PowerShell; they call Podman through `wsl -d <distro>` (`OFW_WSL_DISTRO`, default `Ubuntu`).
- On Windows and WSL, `podman run` uses `--user root`. Rootless Podman maps the container's `oracle` user to a UID that cannot write on the Windows drive (`/mnt/c`); container root maps to your user.
- `convert_fmb.py` runs Forms2XML as a Java class with `-Djava.library.path=/oracle_home_14c/lib` and `FORMS_API_TK_BYPASS=TRUE`. In 14c, `bin/frmf2xml.sh` and the other `bin/*.sh` batch scripts are templates with `$$placeholder$$` values, not runnable files, so the scripts call the Java class and the binaries directly.
- `bin/rwconverter.sh` is also a template; call `bin/rwconverter` directly.
- Keep test files under git-ignored `temp/`, never in `samples/`.
- The Oracle Home location is a setting: `OFW_ORACLE_HOME` (environment, then `.env`) overrides the default `oracle-reference-stack/oracle_home_14c`; `converter_runtime.py` reads it.

## Known limits

- **PLD to PLL:** on 14c, `frmcmp_batch` prints nothing and writes no `.pll` when given a `.pld` text file, even a real Oracle-exported one. `Forms2XML` rejects libraries (`File ... must be a FormModule, ObjectLibrary or MenuModule`), so there is no XML route. No command-line path was found; Forms Builder was not tried. `samples/utils.pld` is plain PL/SQL text, the same kind of file `frmcmp_batch script=yes` exports.
- **XML to OLB:** the older 12c sample XML is rejected by 14c. 14c-format XML (from OLB to XML) converts back fine.
- **Reports:** `bin/rwconverter` needs `libnsl.so.2` (the image has `libnsl2` since the rebuild). The owner reported `2 PASS` for the report paths; the agent saw only that last line, so re-run `test_conversions.py --only reports` to confirm. See `setting-up-oracle-testbed/references/reports-conversion.md`.
- **Not tested:** XLIFF, `.rpt`, and `.rex` conversions.

## Schema and validation

The Forms XML schema is `forms-14c.xsd` in this skill's `assets/` (see [forms-xml-schema.md](forms-xml-schema.md) for provenance and extraction with `extract_forms_schema.py`). Validate XML before converting it with the `compiling-and-running-oracle-forms` skill's `validate_forms_xml.py`.
