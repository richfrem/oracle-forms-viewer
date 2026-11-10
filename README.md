# Oracle Forms Workbench

A comprehensive, pure-native Rust workbench and reverse-engineering toolchain for inspecting, visualizing, querying, and modernizing legacy **Oracle Forms & Reports** applications (`.fmb`, `.mmb`, `.olb`, Forms2XML exports, `.pld`, `.rdf`).

Zero JVM, zero Node.js, and zero Oracle runtime software required for the native engine.

---

## Key Capabilities

- **Pure-Native Visual Viewer**: High-performance streaming parser and 2D canvas renderer for complex forms layouts, coordinate scaling, and visual hierarchy.
- **Direct Database Query Engine (Phase A)**: Query live database backends (Oracle Database 23ai Free `:1521/FREEPDB1`, embedded SQLite with pre-seeded datasets, and offline mock driver) directly from forms modules—no WebLogic or Java required!
- **Interactive Multi-Record Browsing**: Live record flipping (`[◀] Rec 1 of 4 [▶]`) directly on the visual canvas and runtime window frame.
- **Direct Binary ROS 60050 Parser**: Direct byte-stream extraction of triggers, program units, and items from legacy binary `.fmb` files without Oracle Forms Builder.
- **Full CLI Toolkit**: Inspect ASTs, dump SQL triggers and program units, and execute block queries from the terminal.
- **Containerized Ground Truth & APM Skills**: Integrated automation skills for official Forms 12c/14c headless batch compilation (`.fmb` $\to$ `.fmx`), Forms2XML, PLL conversion, and live desktop execution via Forms Standalone Launcher (FSAL).

---

## Screenshots

### Interactive Form Preview

Preview a claims form alongside the object navigator
and property inspector. Interactive Preview supports local edits without database
connections or PL/SQL execution.

![Interactive claims form preview with the runtime frame and property inspector](docs/screenshots/claims-preview.png)

### XML and Property Inspection

Select a form item to inspect its source XML, layout coordinates, data type, and
database-related properties.

![Raw XML slice and property inspector for the selected claim ID field](docs/screenshots/xml-property-inspector.png)

### Live Forms 14c Desktop Execution (FSAL + Oracle 23ai Database)

Run compiled forms (`.fmx`) natively on macOS and Windows via Forms Standalone Launcher (FSAL) backed by Oracle WebLogic 14c and Oracle Database 23ai Free.

![Live Oracle Forms 14c desktop application executing dynamic database query](docs/screenshots/forms-14c-live-db-query-gadget-b.jpg)

---

## Repository Structure

```text
├── Cargo.toml                # Virtual workspace root
├── crates/                   # Pure Rust multi-crate workspace
│   ├── oracle-forms-core/    # Canonical AST models, sidecars, geometry
│   ├── oracle-forms-xml/     # quick-xml streaming parser (XML & PLD)
│   ├── oracle-forms-binary/  # ROS 60050 stream & heuristic binary extractor
│   ├── oracle-forms-db/      # Direct DB query engine (Oracle 23ai, SQLite, Mock)
│   ├── oracle-forms-cli/     # Standalone CLI (inspect, dump-sql, query)
│   └── oracle-forms-server/  # Axum web server with embedded UI assets
├── ui/                       # Vanilla HTML/CSS/JS frontend
├── samples/                  # Curated redistributable sample forms (XML, PLD, FMB)
│
├── plugins/oracle-forms/     # APM Skills & Automation for Oracle Forms & Reports 14c
│   ├── .apm/skills/
│   │   ├── setting-up-oracle-testbed/       # Automated Podman 23ai & Forms 14c setup
│   │   ├── converting-oracle-artifacts/     # Forms2XML, XML2Forms, PLL, and Reports converters
│   │   └── compiling-and-running-oracle-forms/ # Headless compiler & live desktop FSAL runner
│   └── README.md
│
├── oracle-reference-stack/   # Official Oracle 12c & 14c Podman stack + 23ai DB (Track 1 ground truth)
│
├── docs/                     # Architectural specifications & blueprints
│   ├── NATIVE-RUST-WORKBENCH-ARCHITECTURE.md # Active system architecture & implementation status
│   ├── DEPENDENCIES.md                       # System dependencies & Software Bill of Materials (SBOM)
│   ├── rust-workbench-architecture.mmd       # Rust workspace architecture diagram
│   ├── PODMAN-CONVERTER-SETUP-AND-HISTORY.md # Official Oracle 12c container setup
│   ├── podman-architecture.mmd               # Container architecture diagram
│   └── plans/                                # Comprehensive engineering roadmap & RFCs
│
├── temp/                     # [GITIGNORED] Local work files, test binaries, legacy POC
│
├── architecture.md           # System architecture, data pipelines, & crate boundaries
├── AGENTS.md                 # Operational guidelines and constraints for AI agents
└── HOW-TO-RUN.md             # Step-by-step instructions to run the server and CLI
```

---

## Quickstart

### 1. Launch the Native Web Viewer (Port 7070)

```bash
cargo run -p oracle-forms-server
```

Open **[http://127.0.0.1:7070](http://127.0.0.1:7070)** in your browser:
- Select any preloaded sample (`test_db_fmb.xml`, `orders_fmb.xml`, `claims_fmb.xml`, `simple_module.fmb`).
- Or drag & drop your own `.fmb` or `.xml` file to view its 2D canvas, object hierarchy, and PL/SQL triggers.
- **Direct Database Querying**: Select your database backend (`Oracle 23ai (:1521)`, `SQLite`, `Mock`) and click **⚡ Query DB** (or press `Ctrl+F11` / `F8`) to fetch live database records and navigate through rows (`[◀] 1 of 4 [▶]`) directly on the canvas!

### 2. Inspect and Query Forms from the Command Line

```bash
# Query live database records for a block (connects to Oracle 23ai Free :1521)
OFW_DB_USER=forms_user OFW_DB_PASSWORD=... cargo run -p oracle-forms-cli -- query samples/test_db_fmb.xml --backend oracle-thin

# Query offline using the mock driver
cargo run -p oracle-forms-cli -- query samples/orders_fmb.xml --backend mock

# Inspect an XML module export
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml

# Inspect a raw binary .FMB directly (ROS 60050 stream decoding)
cargo run -p oracle-forms-cli -- inspect samples/simple_module.fmb

# Dump all PL/SQL triggers and program units as .sql files
cargo run -p oracle-forms-cli -- dump-sql samples/orders_fmb.xml -o ./extracted-sql
```

### 3. Run the Automated Test Suite

```bash
cargo test
```

### 4. Run Forms Live via Desktop FSAL (Oracle 14c + 23ai DB)

```bash
# Health-check the testbed:
python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/health_check.py

# Convert, compile, and launch any form immediately in a desktop window:
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/test_db.fmb
```

---

## Key Documentation Links

- **[How to Run Native Viewer & CLI](HOW-TO-RUN.md)**: Server configuration, custom ports, CLI commands, and desktop FSAL runner.
- **[Oracle Forms APM Plugin](plugins/oracle-forms/README.md)**: APM skills for setup, conversion, compilation, and live desktop execution.
- **[Native Rust Workbench Architecture](docs/NATIVE-RUST-WORKBENCH-ARCHITECTURE.md)**: Active system architecture, delivered capabilities, and technical roadmap.
- **[System Dependencies & SBOM](docs/DEPENDENCIES.md)**: Complete dependency audit for Rust workspace, Podman container, and host systems.
- **[Running Official Forms Builder GUI](ORACLE-FORMS-BUILDER-GUI.md)**: Launching native Oracle Forms Builder (`frmbld`) on macOS/Linux via X11 for visual ground-truth comparison.
- **[Forms & Reports 14c Testing Guide](docs/FORMS-AND-REPORTS-14C-TESTING-GUIDE.md)**: Full verification blueprint for Forms 14c, Reports Server, FSAL live execution, and 23ai Database integration.
- **[System Architecture](architecture.md)**: Data pipelines, crate contracts, coordinate math, and sequence diagrams.
- **[Agent Guidelines](AGENTS.md)**: Rules and invariants for automated coding agents.
- **[Official Oracle 12c Toolchain](docs/PODMAN-CONVERTER-SETUP-AND-HISTORY.md)**: Podman Rosetta 2 setup for certified baseline XML generation.

---

## License

This project is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

## Recent Workbench Updates

- **Menu modules (.mmb)**: `samples/ofw_emp_menu.mmb` (+ `_mmb.xml`) demonstrates a custom menu for `OFW_DEMO_EMPLOYEES`. Attach one to an open form via the canvas toolbar menu selector (`POST /api/forms/module/{name}/attach-menu`).
- **Browse / Open**: `GET /api/files/browse` lists `.fmb`, `.xml`, `.olb`, `.mmb`, report files from `samples/`; uploads are also supported.
- **Reports mode**: PL/SQL split view, data model diagram, property sheet, template themes and XML export (sample: `ofw_employee_directory_report.xml`).
- **XML entity decoding**: attributes (labels, PL/SQL) are fully unescaped by the XML reader, so menus no longer show `&amp;` / `amp;` / `lt;New_Item>`.
- **Known gap**: opening a standalone `.mmb` shows the previous form's canvas; a dedicated menu inspector view is planned.
