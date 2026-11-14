# Validating Forms, menu, library, and report XML before converting

Validate hand-written or generated XML against Oracle's own schema **before** converting it to a binary module (XML2Forms) and compiling it. A schema check takes a second or two; a failed XML2Forms or compile takes minutes and gives worse messages.

```bash
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/validate_forms_xml.py <file.xml>...
```

The script is `scripts/validate_forms_xml.py`; its header lists every option. Run it from the repo root, from the Ubuntu WSL shell on Windows.

## What it validates, by artifact

| Artifact | XML form | Validated against | Notes |
|---|---|---|---|
| Form (`.fmb`) | `<Module xmlns="http://xmlns.oracle.com/Forms"><FormModule ...>` | `assets/forms-<version>.xsd` | Default version `14c` |
| Menu (`.mmb`) | `<Module ...><MenuModule ...>` | the same Forms schema | |
| Object library (`.olb`) | `<Module ...><ObjectLibrary ...>` | the same Forms schema | |
| Report (`.rdf`) | `<report DTDVersion="..." ...>` | `assets/reports.dtd` (a DTD, not an XSD) | Reports XML has a DOCTYPE and no namespace |
| PL/SQL library (`.pll`) | none | nothing to validate | `.pll` has no XML form; `Forms2XML` rejects libraries |

The root element picks the schema, so you can pass a mixed list of files.

## Choosing the schema version

| Option | Meaning |
|---|---|
| `--version 14c` | Default. Uses `assets/forms-14c.xsd` (Oracle Forms 14.1.2). Use it for anything that will go through the 14c XML2Forms |
| `--version auto` | Reads `<Module version="...">`: `1410xxxxx` means 14c, `12.2.1.19.0` or `122010400` means 12c. If that version's schema is not in `assets/`, it falls back to 14c and prints that it did |
| `--version <name>` | Any `assets/forms-<name>.xsd`. An unknown name stops with exit code 2 and lists what exists |
| `--schema FILE` | Validate against any XSD file (overrides `--version`) |
| `--list-versions` | Show the versions in `assets/` |

Only `14c` is included. The 12c schema is not; to add a version, extract the schema from that version's Oracle Home with `converting-oracle-artifacts/scripts/extract_forms_schema.py` and save it as `assets/forms-<version>.xsd`.

**The declared Module version does not decide which schema a file passes.** The 14c schema only requires `version` to be a number (`xs:long`). Oracle 14c writes `141020000`, but `ofw_sales_fmb.xml` (declared `122010400`) and `test_db_fmb.xml` pass the 14c schema. A dotted version such as `12.2.1.19.0` fails because it is not a number.

## Backends

`--backend auto` uses the first available:

| Backend | Needs | Notes |
|---|---|---|
| `xmllint` | `libxml2-utils` (Ubuntu: `sudo apt-get install -y libxml2-utils`; macOS has it) | Fastest. Verified on Windows (WSL) |
| `lxml` | the Python package `lxml` | Not installed in the verified WSL |
| `java` | Podman running and the `forms-converter-14c` image | Runs `scripts/ValidateXml.java` with the JDK 17 in a throwaway container. **Not verified** (the Podman runtime was down when this was written); only files inside the repo can be validated this way |

Exit codes: `0` all valid, `1` some invalid or malformed, `2` no backend, no such schema version, or a usage error.

## Reading the errors

| Message | Cause | Fix |
|---|---|---|
| `attribute 'version': '12.2.1.19.0' is not a valid value of the atomic type 'xs:long'` | A dotted 12c version | Use the number, `141020000` for 14c |
| `attribute 'XPosition': The attribute 'XPosition' is not allowed` (also `CanvasName` on `Graphics`, `LOVName` on `Item`, `DistanceBetweenRecords` on `Block`) | The attribute is not valid on that element in the 14c schema | Check the schema for where it belongs (see below) |
| `Element 'ProgramUnit': Character content is not allowed, because the content type is empty` | The code is written as element text; the schema defines it as the attribute `ProgramUnitText` (checked in `forms-14c.xsd`) | Move the code into the attribute |
| `attribute 'DataType': The attribute 'DataType' is not allowed` on `ModuleParameter` or `RecordGroupColumn` | Legacy names; the schema uses `ParameterDataType` and `ColumnDataType` | Rename |
| `Element '...FutureUnknownElement': This element is not expected` | A deliberate parser-test marker in `orders_fmb.xml` | Not a real form element; ignore in that fixture |
| `Element report does not carry attribute DTDVersion` | A report missing the required `DTDVersion` | Add `DTDVersion="9.0.4.0.0"` (as the other sample reports have) |

To find the right element or attribute name, search `assets/forms-14c.xsd` (about 168 KB; do not read it whole). It is a script input, not a document for reading into context, which is why it lives in `assets/` and this page lives in `references/`.

## Where the schema lives (asset or reference?)

The schema is an **asset**: a data file that a script reads, not text for an agent to read. This page and `converting-oracle-artifacts/references/forms-xml-schema.md` are the **references**. Two copies, byte-identical (the schema is no longer in `docs/`) (SHA-256 `57b113f4...`):

| Copy | Purpose |
|---|---|
| `converting-oracle-artifacts/assets/forms-14c.xsd` | Owned by the converting skill; `extract_forms_schema.py` compares against and updates it |
| `compiling-and-running-oracle-forms/assets/forms-14c.xsd` | Used by `validate_forms_xml.py`; a skill must be self-contained when packaged |

`extract_forms_schema.py --update` refreshes both together.

## Verified results

`validate_forms_xml.py samples/*.xml` on 2026-11-14 (WSL, `xmllint`, 14c): **9 passed, 13 failed**.

- **Pass (9):** `ofw_demo_employees_fmb`, `ofw_emp_menu_mmb`, `ofw_sales_fmb`, `simple_module_fmb`, `test_db_fmb`, and four reports: `demo_employees`, `department_salaries`, `ofw_employee_directory`, `orders_summary`.
- **Fail (13):** twelve older Forms fixtures (`claims`, `customers`, `departments_employees`, `dupes`, `entry`, `mainmenu`, `master_details`, `objects_olb`, `orders`, `picker`, `styles`, `toolbar`) plus the report `parameterized_items_report` (no `DTDVersion`). Seven of the twelve fail only on the dotted `12.2.1.19.0` version (`claims`, `dupes`, `entry`, `mainmenu`, `objects_olb`, `styles`, `toolbar`); `customers`, `departments_employees`, `master_details`, `orders`, and `picker` also have wrong attribute or element names.
- These fixtures are used by the Rust parser tests and are deliberately 12c-style or legacy. They are not wrong in the Rust workbench; they are not valid 14c XML for XML2Forms. The Rust app's Preview in Oracle builds a normalized `oracle_compatible/` copy for such files.

**Not verified:** the `java` backend, validation of report XML through `lxml`, and whether Oracle's own `XMLValidator` class (`forms/templates/scripts/frmxmlv.sh` in the Oracle Home) gives the same answers as `xmllint`. That Oracle validator could be run in a container as a second opinion; its arguments were not checked.
