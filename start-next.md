# Start here next

Read [AGENTS.md](AGENTS.md) and every file in [.agent/rules/](.agent/rules/) first. After every step below, update the documentation before the next step (change log: `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/windows-wsl-change-log.md`), script anything done by hand, and report only what was verified. Run everything inside WSL2. Do not remove or replace the owner's containers or images, and say what will happen before any image rebuild.

## Current state and in-progress tasks (2026-11-14, end of session)

**In progress: moving the Oracle Home to Ubuntu's disk** (`~/oracle_home_14c`, not the repo folder; see `oracle-home-location.md`).
- Done: the copy has all 46,736 files and all 404 symbolic links; `frmweb` and the other programs are executable; owner is 1000:1000 (`podman unshare chown`); 377 Windows `Zone.Identifier` files removed from the copy.
- Last `--verify` **FAILED**: the file counts and links match, but total bytes differ (copy 6,141,025,443; original 6,141,047,460), and the original's byte count kept moving between runs, so the original was still being written to. `forms-server-14c` was force-stopped to freeze it (exit 137). **Re-run `--verify` now that it is stopped**; if it still fails, list the differing files before changing anything.
- Not done, owner's approval needed: set `OFW_ORACLE_HOME` in `.env`, recreate `forms-server-14c` on the new folder, and record the boot time. Until then the Forms container still mounts the repo copy.
- `start_testbed.py --stop-forms` added: graceful stop with a 60-second wait; it prints manual steps if the container will not stop and never kills processes.
- `restore_oracle_home_attributes.sh` is written but **not run** (the manual steps above did the same job); keep it for future copies and test it before relying on it.
- `apply_forms14c_config.sh` is kept: it reads `OFW_ORACLE_HOME` and is how the configuration is re-applied to a rebuilt Oracle Home.
- The Windows install docs now say to put the Oracle Home on Ubuntu's disk before installing. Stack and change-log docs are updated. Nothing is committed yet.

**Step 0, every session:** read `architecture.md`, then in the WSL shell run `python3 start_testbed.py --status` (read-only). If it says "Podman runtime directory missing", ask the user to run `python3 start_testbed.py --no-forms` (or the full command) in the Ubuntu shell: it recreates `/run/user/<uid>` and prompts for the WSL sudo password, which an agent cannot enter. Throwaway `podman run --rm` containers (conversions, schema extraction) need that directory but not the running `forms-server-14c`.

**Schema work (2026-11-14):** Oracle's Forms 14.1.2 schema is `forms-14c.xsd`, in the `assets/` of `compiling-and-running-oracle-forms` and `converting-oracle-artifacts` (identical; none in `docs/`). Built and documented: `validate_forms_xml.py` (`xml-validation.md`) and `extract_forms_schema.py` (`forms-xml-schema.md`). Still to do: run `extract_forms_schema.py` once (14c has no `getformsxmlschema.sh`; the generator is the `SchemaGenerator` class, its arguments are unverified, and Podman must be up); wire the 12c schema the owner pasted (`assets/12c/forms.xsd` in the compiling skill, copy it as a `12c` version in both skills' `assets/`) and compare it with 14c.

State (2026-11-14): conversion paths from `plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/test_conversions.py` (Windows 11, everything in WSL2, Forms 14.1.2 image):

| Type | Path | Status |
|---|---|---|
| Forms | FMB to XML, XML to FMB, compile FMB to FMX | PASS |
| Menus | MMB to XML, XML to MMB, compile MMB to MMX | PASS |
| Libraries | PLL to PLD | PASS |
| Libraries | PLD to PLL | BLOCKED: `frmcmp_batch` writes no `.pll` from `.pld` text; `Forms2XML` rejects libraries. Unresolved |
| OLB | OLB to XML, XML to OLB | PASS |
| Reports | XML to RDF, RDF to XML | The owner reported `2 PASS` after the image gained `libnsl2`; the agent saw only that last line. Confirm (below) |

Goal: every path for all five types works. One path is blocked (PLD to PLL); the report paths need confirming.

## 1. Reports: confirm XML to RDF and RDF to XML

No rebuild is needed: the current `localhost/forms-converter-14c:latest` already has `libnsl2` (`rpm -q libnsl2` printed `libnsl2-1.2.0-2.20180605git4a062cf.el8.x86_64`; someone rebuilt it, not the agent). The old image is tagged `backup-20261114` and still backs the running `forms-server-14c`. To do:
1. Run `python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/test_conversions.py --only reports` and read the table (not just the last line). `apply_forms14c_config.sh` step 5b (Reports `prefs.ora` and printer files) was added by someone else and is the likely reason it passes; find out and record it.
2. Run `convert_rpt.py` end to end on a real `.rdf` (the one the test produced), including the `.rdf` to XML direction, and record the result in the change log and `references/reports-conversion.md`.
3. Do not remove or recreate `forms-server-14c` or `oracle23ai-lite` without the owner's approval (settings saved in the git-ignored `temp/image-backup/forms-server-14c.inspect.json`).

## 2. Libraries: PLD to PLL

Facts: `frmcmp_batch module=lib.pld module_type=library ...` (tried with `script=no`, `compile_all=yes`, and both) prints nothing and writes no `.pll`. `Forms2XML` says `File lib.pll must be a FormModule, ObjectLibrary or MenuModule`, so there is no XML route. The `.pld` that `frmcmp_batch script=yes` exports is plain PL/SQL source text (the same kind of file as `samples/utils.pld`).

Not yet tried:
- Forms Builder (`frmbld`, works through WSLg; command in `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/forms-builder-gui.md`): create or open a library, paste the program units from the `.pld`, save as `.pll`. If this works, script it.
- Search Oracle's 14c documentation and `forms/templates` for any batch library-import utility.
- `frmcmp_batch` options not yet tried (read its `help=yes` output).
If nothing works, document that PLD to PLL needs Forms Builder (or is unsupported), update the test script's wording, and keep it as a known limit.

## 3. Other open items

- **Move the Oracle Home to Ubuntu's disk** (owner's decision, 2026-11-14, scripted, not yet run): follow `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/oracle-home-location.md`: dry run, `--from-copy`, `--verify --against` (fingerprint IDENTICAL and the timed walks), set `OFW_ORACLE_HOME`, recreate `forms-server-14c` with the new mount (owner approves; settings in `temp/image-backup/forms-server-14c.inspect.json`), re-run `python3 start_testbed.py` and record the Forms "took" time against the 15 to 44 minutes before. Then `--backup`. Also update `testing-forms-parity/scripts/test_parity.py`, which still hard-codes the repo Oracle Home path in two places, and look for host paths inside the WebLogic domain files before recreating the container.
- **`apply_forms14c_config.sh` step 5b** (Oracle Reports `prefs.ora` and printer files for `rwconverter`) was added by someone other than the agent; find out who and what they verified, then re-run `test_conversions.py --only reports` against the new `latest` image (it has `libnsl2`).

- **Run `oracle-reference-stack/apply_forms14c_config.sh` on a clean copy of the Oracle Home** (not the live one) and compare with the working setup. It was written from the verified manual steps and has never been run end to end.
- **XML Source tab:** restart the app to see the new full-XML default, and fix the bug where selecting one item shows two lines (selecting `EMP.FIRST_NAME` showed lines 23 to 24, which include `EMP_ID`; the start line looks one too early; look in `crates/oracle-forms-xml` for where line numbers are recorded).
- **Oracle download steps:** `windows-podman-setup.md` section 4 and `references/dependencies.md` 2.4b list part numbers, file names, sizes, and destinations, but not the click-by-click eDelivery steps. The owner did the downloads and needs to supply them.
- **Not recorded originally:** the exact Ubuntu Java package name (`openjdk-21-jre` is a guess; WSL runs OpenJDK 21.0.9).
- **Rust 1.100 prints 8 manifest lint warnings** (redundant `homepage`, unused `tracing`) while AGENTS.md says zero warnings. Needs the owner's decision.
- **Not tested:** XLIFF, `.rpt`, `.rex` conversions; MMB/OLB conversions on macOS; whether the `--user root` container rule is needed inside WSL.
- Stale local branches `docs/windows-podman-setup` and `feat/windows-wsl-all-in-wsl` still exist; ask before deleting.
- **Run Reports on Fusion Middleware (like Forms):** Investigate running Oracle Reports (`.rdf` / `_report.xml`) on the Fusion Middleware 14c server (Reports Server / `rwservlet`) from the workbench, similar to how Forms launches via FSAL.
- **Skills audit (2026-11-14):** all five skills PASS the `authoring-skills` audit in source and installed mode (0 warnings). Re-run it after changing any skill: `python3 ~/.claude/skills/authoring-skills/scripts/audit_skill.py plugins/oracle-forms --all --mode source` (and `--mode installed`).
- **`oracle-reference-stack/compose.yaml`** has a literal database password value (breaks the credentials-in-`.env` rule), an obsolete `version: '3.8'`, and a macOS-only `DISPLAY`; documented in `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/oracle-reference-stack-map.md`, not fixed.
- **`launch_fsal.py` has a database login with a password written in the `-u` default** (breaks the `.env` rule). It is an existing script used on macOS, so changing it changes that flow; owner's decision. `compile_form.py` already reads the logon from `.env`.
- **`run_form.py` has no `--auto-login` flag** (an older `HOW-TO-RUN.md` said so; fixed there).
