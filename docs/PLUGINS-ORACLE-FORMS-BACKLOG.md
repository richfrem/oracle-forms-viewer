# Architecture & Backlog: `plugins/oracle-forms` APM Plugin

This document defines the architectural specification and implementation backlog for packaging the Oracle Forms & Reports toolchain into an **APM-native agent plugin** (`plugins/oracle-forms/`) using `authoring-plugins`, `authoring-skills`, and `packaging-with-apm`.

---

## 1. Executive Summary & Objective

Through our reverse-engineering and verification milestones, we solved the end-to-end containerized setup for:
1. **Oracle Database 23ai Free** (multi-arch native ARM64/x86_64).
2. **Oracle Forms & Reports 14c** on Apple Silicon (Rosetta 2) and Windows (WSL2 / WSLg).
3. **Automated Runtime Relinking** (`genshlib` building 16 `.so.0` shared libraries).
4. **WebLogic 14.1.2 Forms Server Domain Provisioning** (`create_forms_domain.py`, `boot.properties`, Java 17 NPE patches).
5. **Headless Compilation** (`frmcmp_batch` compiling `.fmb` $\rightarrow$ `.fmx` with database SQL*Net binding).
6. **Bidirectional Forms XML Conversion** (`Forms2XML` & `XML2Forms`).
7. **Native Desktop Execution via FSAL** (live database queries rendered to Mac/Windows desktop).

Packaging these capabilities into `plugins/oracle-forms` allows any AI coding agent or developer across any machine to deploy, compile, convert, and inspect Oracle Forms with a single natural language instruction.

---

## 2. Streamlined APM Plugin Architecture (3 Consolidated Skills)

Rather than fragmenting into many micro-skills (which causes routing confusion and prompt overhead), the plugin follows the **"Fewer Skills, Richer References"** principle:

```text
plugins/oracle-forms/
├── apm.yml                                # APM package manifest
├── .claude-plugin/
│   └── plugin.json                        # Claude / Copilot plugin discovery manifest
├── README.md                              # Plugin overview, capabilities, and quick start
└── .apm/
    ├── agents/
    │   └── oracle-forms-engineer.md       # Autonomous Forms & Reports specialist
    └── skills/
        ├── setting-up-oracle-testbed/     # 1. Unified Environment & Container Setup
        │   ├── SKILL.md                   # Interactive setup wizard & router
        │   ├── scripts/
        │   │   ├── provision_testbed.py   # Spins up 23ai + 14c containers
        │   │   ├── relink_runtime.sh      # genshlib runner for 16 shared libraries
        │   │   ├── init_database.sh       # Provisions $DB_USER, tables, & TNS entries
        │   │   └── health_check.py        # Probes 7001 AdminServer & frmservlet/admin
        │   └── references/
        │       ├── database-23ai-setup.md # Multi-arch DB container configuration
        │       ├── forms-14c-container.md # Containerfile.14c and volume mounts
        │       ├── genshlib-relinking.md  # Shared object creation via gcc
        │       ├── weblogic-domain.md     # Domain creation, boot.properties, Java 17 NPE fix
        │       ├── tns-configuration.md   # tnsnames.ora FREEPDB1 alias
        │       └── troubleshooting.md     # Comprehensive 9-error matrix with permanent fixes
        │
        ├── converting-oracle-artifacts/   # 2. Unified Conversion Engine
        │   ├── SKILL.md                   # Router for all conversion operations
        │   ├── scripts/
        │   │   ├── convert_fmb.py         # Forms (.fmb, .mmb, .olb <-> XML)
        │   │   ├── convert_rpt.py         # Reports (.rdf, .rpt -> XML / REX)
        │   │   └── convert_pll.py         # Libraries (.pll <-> .pld plain-text)
        │   └── references/
        │       ├── forms-xml-schema.md    # forms-14c.xsd validation and version rules
        │       ├── reports-conversion.md  # rwconverter.sh options and flags
        │       └── plsql-libraries.md     # Procedure Builder and PL/SQL compiler rules
        │
        └── compiling-and-running-oracle-forms/ # 3. Headless Compilation & Live Execution
            ├── SKILL.md                   # Router for compilation & live desktop launching
            ├── scripts/
            │   ├── compile_form.py        # frmcmp_batch runner with TNS & error parsing
            │   └── launch_fsal.py         # Desktop FSAL launcher with URL password encoding
            └── references/
                ├── compiler-flags.md      # Logon=NO, Compile_All, Module_Type options
                └── fsal-parameters.md     # Standaloneapp parameters and DPI scaling
```

---

## 3. Detailed Skill Specifications

### Skill 1: `managing-oracle-forms-testbed`
* **Trigger phrases**: *"start oracle forms server"*, *"provision forms 14c container"*, *"check testbed health"*, *"relink forms runtime"*.
* **Capabilities**:
  * Runs Podman commands to spin up `oracle23ai-lite` and `forms-server-14c`.
  * Verifies the container runtime (Rosetta 2 translation on Apple Silicon; native x86-64 inside WSL2 on Windows, where the Rust app runs too).
  * Executes `genshlib` inside the container if shared libraries are missing.
  * Ensures `tnsnames.ora`, `default.env`, and `setUserOverrides.sh` are synchronized.
  * Runs pre-flight health checks against `http://localhost:7001/forms/frmservlet/admin`.

### Skill 2: `compiling-oracle-forms`
* **Trigger phrases**: *"compile form <file.fmb>"*, *"build fmx"*, *"compile menu <file.mmb>"*.
* **Capabilities**:
  * Dispatches `frmcmp_batch` inside `forms-server-14c`.
  * Automatically injects `ORACLE_TERM=vt220`, `FORMS_PATH`, and `TNS_ADMIN`.
  * Supports offline compilation (`logon=no`) or live database compilation (`userid=$DB_USER/$DB_PASSWORD@FREEPDB1`).
  * Parses stdout and compiler logs: highlights specific compilation errors (e.g. `FRM-30174 Check Box values must be distinct`, `PLS-00201 identifier must be declared`) and suggests exact line fixes.

### Skill 3: `converting-oracle-forms-xml`
* **Trigger phrases**: *"convert fmb to xml"*, *"convert xml to fmb"*, *"inspect forms xml"*.
* **Capabilities**:
  * Calls official `Forms2XML` and `XML2Forms` Java tools via `Containerfile.14c`.
  * Automatically sanitizes hand-crafted XML: transforms dotted versions (`12.2.1.19.0`) to schema-compliant numeric longs (`141020000`).
  * Validates XML against `docs/forms-14c.xsd` before calling the Oracle engine.

### Skill 4: `converting-oracle-reports`
* **Trigger phrases**: *"convert report <file.rdf>"*, *"export report to xml"*, *"decompile report to text"*.
* **Capabilities**:
  * Wraps `convert_rpt.py` and `rwconverter.sh`.
  * Supports format conversions: `XML`, `REX` (ASCII source script), and `RDF`.

### Skill 5: `running-forms-standalone-launcher`
* **Trigger phrases**: *"run form <file.fmx>"*, *"open fsal"*, *"launch form on mac"*.
* **Capabilities**:
  * Generates the proper FSAL launch command with URL-encoded passwords (`%23` for `#`).
  * Checks that Java 17+ or 25 is installed on the host.
  * Launches the desktop window asynchronously and monitors session establishment (`AdminServer.formsapp.N`).

---

## 4. Backlog Implementation Phases

| Phase | Milestone | Deliverables | Sibling Skills Used |
| :--- | :--- | :--- | :--- |
| **Phase 1** | Plugin Scaffolding | Create `plugins/oracle-forms/` skeleton, `apm.yml`, `.claude-plugin/plugin.json`, `README.md`. | `authoring-plugins` (`scaffold_plugin.py`) |
| **Phase 2** | Testbed & Compilation Skills | Scaffold `managing-oracle-forms-testbed` and `compiling-oracle-forms`. Port all shell and Python scripts into self-contained skill directories. | `authoring-skills` |
| **Phase 3** | Conversion & Runner Skills | Scaffold `converting-oracle-forms-xml`, `converting-oracle-reports`, and `running-forms-standalone-launcher`. | `authoring-skills` |
| **Phase 4** | Sub-Agent Definition | Scaffold `.apm/agents/oracle-forms-engineer.md` with bounded tool permissions and persona. | `authoring-subagents` |
| **Phase 5** | APM Audit & Packaging | Run `scripts/audit_plugin_structure.py` and compile into APM package. Verify installation with `apm-cli`. | `packaging-with-apm` |

## Backlog (from UI review)

- Standalone `.mmb` inspector canvas (menu tree) instead of stale form canvas.
- Remove `overflow-x` on `.top-bar`/`.canvas-toolbar`; compact header controls at 1280px.
- Filter `<New_Item>` placeholder menu items in binary `.mmb` extraction.
- Cache-busting / `no-store` headers for embedded UI assets.
