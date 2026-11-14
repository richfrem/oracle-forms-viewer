# Start here next

Read [AGENTS.md](AGENTS.md) and every file in [.agent/rules/](.agent/rules/) first. After every step below, update the documentation before the next step (change log: `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/windows-wsl-change-log.md`), script anything done by hand, and report only what was verified. Run everything inside WSL2. Do not remove or replace the owner's containers or images, and say what will happen before any image rebuild.

State at the end of PR #26 (`main` at `efe10ef`): conversion test results from `plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/test_conversions.py` (Windows 11, everything in WSL2, Forms 14.1.2 image):

| Type | Path | Status |
|---|---|---|
| Forms | FMB to XML, XML to FMB, compile FMB to FMX | PASS |
| Menus | MMB to XML, XML to MMB, compile MMB to MMX | PASS |
| Libraries | PLL to PLD | PASS |
| Libraries | PLD to PLL | BLOCKED: `frmcmp_batch` writes no `.pll` from `.pld` text; `Forms2XML` rejects libraries. Unresolved |
| OLB | OLB to XML, XML to OLB | PASS |
| Reports | XML to RDF | BLOCKED: `rwconverter` needs `libnsl.so.2`, missing from the image |
| Reports | RDF to XML | SKIP: needs the `.rdf` from the step above |

Goal: every path for all five types works. Three paths are left.

## 1. Reports: XML to RDF and RDF to XML

Cause: `bin/rwconverter` fails with `error while loading shared libraries: libnsl.so.2`. `Containerfile.14c` already lists `libnsl2` (merged), but the image `forms-converter-14c:latest` was built before that and has not been rebuilt.

Plan (tell the owner before doing the rebuild):
1. Build the new image under a temporary tag, with a minimal build context so the 6 GB of installers are not sent: copy `Containerfile.14c` and `installer/jdk-17.0.12_linux-x64_bin.tar.gz` into `temp/image-build/`, then `podman build --platform linux/amd64 -t localhost/forms-converter-14c:new -f temp/image-build/Containerfile.14c temp/image-build` (inside WSL, with `XDG_RUNTIME_DIR=/run/user/$(id -u)`). A rebuild was started once and stopped by the owner; nothing was built.
2. Test with the new image only (a throwaway container): `python3 .../test_conversions.py --only reports`, with `OFW_CONVERTER_IMAGE=localhost/forms-converter-14c:new`. Also run `health_check.py`.
3. Only if both pass, retag `new` to `latest`. The old image is already tagged `localhost/forms-converter-14c:backup-20261114`, so rollback is one command.
4. Do not remove or recreate `forms-server-14c` or `oracle23ai-lite`. `forms-server-14c` keeps running on the old image until the owner chooses to recreate it. Its full run settings are saved in the git-ignored `temp/image-backup/forms-server-14c.inspect.json` (command, mounts `oracle_home_14c` to `/oracle_home_14c` and the repo to `/work`, network `oracle-net`, ports 7001 and 9001).
5. If `rwconverter` then needs more libraries, record each one in the change log and the Containerfile.
6. `convert_rpt.py` has never run a successful report conversion; verify it end to end, including the `.rdf` to XML direction. The `.rdf` comes from step 2's XML to RDF output (`samples/orders_summary_report.xml` is the input the test uses).

## 2. Libraries: PLD to PLL

Facts: `frmcmp_batch module=lib.pld module_type=library ...` (tried with `script=no`, `compile_all=yes`, and both) prints nothing and writes no `.pll`. `Forms2XML` says `File lib.pll must be a FormModule, ObjectLibrary or MenuModule`, so there is no XML route. The `.pld` that `frmcmp_batch script=yes` exports is plain PL/SQL source text (the same kind of file as `samples/utils.pld`).

Not yet tried:
- Forms Builder (`frmbld`, works through WSLg; command in `ORACLE-FORMS-BUILDER-GUI.md`): create or open a library, paste the program units from the `.pld`, save as `.pll`. If this works, script it.
- Search Oracle's 14c documentation and `forms/templates` for any batch library-import utility.
- `frmcmp_batch` options not yet tried (read its `help=yes` output).
If nothing works, document that PLD to PLL needs Forms Builder (or is unsupported), update the test script's wording, and keep it as a known limit.

## 3. Other open items

- **Run `oracle-reference-stack/apply_forms14c_config.sh` on a clean copy of the Oracle Home** (not the live one) and compare with the working setup. It was written from the verified manual steps and has never been run end to end.
- **XML Source tab:** restart the app to see the new full-XML default, and fix the bug where selecting one item shows two lines (selecting `EMP.FIRST_NAME` showed lines 23 to 24, which include `EMP_ID`; the start line looks one too early; look in `crates/oracle-forms-xml` for where line numbers are recorded).
- **Oracle download steps:** `windows-podman-setup.md` section 4 and `DEPENDENCIES.md` 2.4b list part numbers, file names, sizes, and destinations, but not the click-by-click eDelivery steps. The owner did the downloads and needs to supply them.
- **Not recorded originally:** the exact Ubuntu Java package name (`openjdk-21-jre` is a guess; WSL runs OpenJDK 21.0.9).
- **Rust 1.100 prints 8 manifest lint warnings** (redundant `homepage`, unused `tracing`) while AGENTS.md says zero warnings. Needs the owner's decision.
- **Not tested:** XLIFF, `.rpt`, `.rex` conversions; MMB/OLB conversions on macOS; whether the `--user root` container rule is needed inside WSL.
- Stale local branches `docs/windows-podman-setup` and `feat/windows-wsl-all-in-wsl` still exist; ask before deleting.
- **Run Reports on Fusion Middleware (like Forms):** Investigate running Oracle Reports (`.rdf` / `_report.xml`) on the Fusion Middleware 14c server (Reports Server / `rwservlet`) from the workbench, similar to how Forms launches via FSAL.
