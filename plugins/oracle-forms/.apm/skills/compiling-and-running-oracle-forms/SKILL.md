---
name: compiling-and-running-oracle-forms
description: "Validates Forms, menu, library, and report XML against Oracle's schema, compiles Oracle Forms (.fmb -> .fmx) headlessly via frmcmp_batch, and runs forms live on macOS/Windows desktops via Forms Standalone Launcher (FSAL). Use when checking hand-written form XML before converting it, compiling forms, testing database SQL*Net binding, or running forms live."
---

# Compiling And Running Oracle Forms

Validate the XML you write, compile it in the Forms 14c container, and open the form with FSAL. Needs the testbed from the `setting-up-oracle-testbed` skill running (containers up, `.env` present). On Windows run everything from the Ubuntu WSL shell.

## Contents

- [Constraints](#constraints)
- [Quick start](#quick-start)
- [Workflow](#workflow)
- [Scripts and assets](#scripts-and-assets)
- [Verification](#verification)

## Constraints

1. Validate XML **before** converting it to a binary module: a schema check takes seconds, a failed XML2Forms or compile takes minutes ([references/xml-validation.md](references/xml-validation.md)).
2. Compile with database logon for PL/SQL that references schema objects; `--offline` (`logon=no`) can fail on unresolved objects. Credentials come from the git-ignored `.env`, never from the command line or docs.
3. FSAL needs Java 17 or 21 (WSL's OpenJDK 21 works; Windows Java 26 fails with `java/applet/AppletStub`).
4. Never test by clicking Save & launch in the Rust app (it rewrites the form's XML); use `launch_fsal.py`.
5. A schema pass means "schema-valid", not "will compile"; a compile or launch success is a pipeline result, not visual or behavioral parity.

## Quick start

From the repo root (paths below are relative to this skill's `scripts/` folder):

```bash
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/validate_forms_xml.py samples/ofw_demo_employees_fmb.xml
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/compile_form.py samples/test_db.fmb
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/launch_fsal.py ofw_demo_employees.fmx --interactive
```

## Workflow

1. **Validate** each XML (form, menu, object library, report): `validate_forms_xml.py <files>`. Pick the schema with `--version 14c|auto|<name>` or `--schema FILE`. Fix errors using the table in `references/xml-validation.md`.
2. **Convert** XML to a binary module with XML2Forms (see the `converting-oracle-artifacts` skill), then **compile** it: `compile_form.py <file.fmb>` (`--type menu`, `--offline`, `-u <logon>` as needed).
3. **Run** it: `run_form.py <file>` does convert (if XML), compile (if FMB), and launch in one step (`--compile-from-env --interactive`, `--offline`), or `launch_fsal.py <form>.fmx --interactive` for an already compiled form.
4. Confirm the servlet first: `python3 start_testbed.py --status` (port 7001).

## Scripts and assets

| File | Purpose |
|---|---|
| `scripts/validate_forms_xml.py`, `scripts/ValidateXml.java` | Schema validation with xmllint, lxml, or Java in a container; `--version`, `--schema`, `--list-versions` |
| `scripts/compile_form.py` | `frmcmp_batch` in the Forms container with TNS binding and error parsing (form, menu, library) |
| `scripts/run_form.py` | Prepare, convert, compile, and launch FSAL in one step; used by the Rust app's Preview in Oracle |
| `scripts/launch_fsal.py` | Open a compiled form through the Forms servlet with FSAL |
| `assets/forms-14c.xsd`, `assets/reports.dtd` | Oracle's Forms 14.1.2 XML Schema and the Reports DTD (script inputs, not for reading whole) |
| `assets/12c/forms.xsd` | The owner's 12c Forms schema, dropped here; **not yet wired** as a `12c` version, so `--version 12c` fails until it is |

## Verification

```bash
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/validate_forms_xml.py samples/*.xml    # 9 pass, 13 fail on 2026-11-14 (older 12c-style fixtures)
```

Verified in WSL: the validator with xmllint, `compile_form.py` (`test_db.fmb`, `ofw_sales.fmb`), `launch_fsal.py`. **Not verified:** the validator's `java` backend, and `--version 12c`. Eval cases: `evals/evals.json`.
