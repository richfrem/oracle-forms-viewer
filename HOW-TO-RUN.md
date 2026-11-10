# How to Run Oracle Forms Workbench

This guide explains how to compile, test, and run the **Oracle Forms Workbench** (`oracle-forms-workbench`) native Rust server, CLI, and database query engine.

Zero JVM, zero Node.js, and zero Oracle runtime software are required.

---

## 1. Prerequisites

You only need standard Rust and Cargo installed:

```bash
# Verify Rust is installed (Rust 1.80+ recommended)
rustc --version
cargo --version
```

If not installed: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

---

## 2. Starting the Web Viewer Server

### Default Port (7070)

From the repository root, run:

```bash
cargo run -p oracle-forms-server
```

You will see:
```text
============================================================
  Oracle Forms Native Viewer (Rust / Axum Server)
  Listening on http://127.0.0.1:7070
============================================================
```

Open your browser to:
👉 **[http://127.0.0.1:7070](http://127.0.0.1:7070)**

### Custom Port (e.g., 8080 or 7071)

Pass the `PORT` environment variable:

```bash
PORT=8080 cargo run -p oracle-forms-server
```

### Network Access & Database Credentials

The server is a local tool: it can read files and connect to databases, so by default it
listens on `127.0.0.1` only, answers only to `localhost` / `127.0.0.1` / `[::1]` Host names,
and refuses API calls from other websites (cross-origin requests).

| Variable | Default | Effect |
| :--- | :--- | :--- |
| `PORT` | `7070` | Listening port |
| `OFW_HOST` | `127.0.0.1` | Listening address. A non-loopback value (e.g. `0.0.0.0`) exposes the server to your network and prints a warning |
| `OFW_ALLOWED_HOSTS` | *(none)* | Extra comma-separated Host names to accept (needed with a non-loopback `OFW_HOST`) |
| `OFW_SERVER_ENV_CREDENTIALS` | off | When `1`, database requests from the UI that carry no credentials use `OFW_DB_USER` / `OFW_DB_PASSWORD` |

There are no built-in database credentials. To query Oracle from the web UI, start the server with:

```bash
OFW_DB_USER=forms_user OFW_DB_PASSWORD='<password>' OFW_SERVER_ENV_CREDENTIALS=1 \
  cargo run -p oracle-forms-server
```

If the database is unreachable or the credentials are missing, you get an error. The server and
CLI never fall back to mock data.

---

## 3. Using the Web Workbench

### Running a Form Live (triggers + PL/SQL)

**▶ Run Form** starts a live session: the form's triggers, program units and attached
`.pld` libraries run on the server, against the database chosen in the **DB** selector.

1. Open a form (e.g. `test_db_fmb.xml`).
2. Choose a backend. For Oracle, enter the DB user and password in the toolbar (sent only
   to this local server, never stored). `SQLite` / `Mock` need no credentials.
3. Click **▶ Run Form**. Startup triggers run; push buttons fire `WHEN-BUTTON-PRESSED`,
   leaving an edited field fires `WHEN-VALIDATE-ITEM`, `SHOW_ALERT` opens a real dialog.
4. Sessions are **read-only** by default (queries run; DML, locks and commits are refused).
   Tick **Allow writes** before starting to enable `💾 Commit`.
5. Open **Session trace** under the canvas to see every trigger, SQL statement and built-in.

Anything the runtime cannot do (e.g. `HOST`, `OLE2`, `CALL_FORM`, LOVs, timers) stops the
trigger with a clear "Not supported" message instead of silently skipping it.

**Export XML** returns the original Forms2XML file with only your layout edits applied
(lossless). Forms opened from a binary `.fmb` can only be exported as a labelled summary.



Once the server is running at `http://127.0.0.1:7070`:

1. **Open Sample Forms**:
   - Use the **"Select Sample"** dropdown in the top header.
   - Choose any sample (e.g. `test_db_fmb.xml`, `orders_fmb.xml`, `claims_fmb.xml`, `simple_module.fmb`).
2. **Open Your Own Files**:
   - Drag & drop any `.fmb`, `.xml`, `.pld`, `.mmb`, or `.olb` file into the browser window.
   - Or type a local file path into the **"File Path"** input box and click **"Open"**.
3. **Execute Live Database Queries (Phase A)**:
   - Select your backend in the **DB:** dropdown:
     - `Oracle 23ai (:1521)`: Connects directly to Oracle Database 23ai Free (`localhost:1521/FREEPDB1`).
     - `SQLite (Embedded)`: Uses in-memory SQLite with pre-seeded datasets.
     - `Mock Offline`: Offline driver returning deterministic test rows.
   - Click **⚡ Query DB** (or press keyboard shortcut `Ctrl+F11` / `F8`).
   - The canvas automatically populates the form fields with live data!
   - Use the record navigator (`[◀] Rec 1 of 4 [▶]`) to browse records.
4. **Interactive Features**:
   - **Canvas View**: Pan, zoom, and visually inspect 2D coordinates, buttons, frames, prompts, and layout elements.
   - **Object Navigator**: Tree navigation across Blocks, Items, Triggers, Program Units, LOVs, and Alerts.
   - **PL/SQL Tab**: Syntax-highlighted view of procedures, functions, package bodies, and triggers with clean line numbers.
   - **XML Tab**: Direct view of raw XML elements and line boundaries.

### Interactive Preview & Runtime Frame

Enable **Interactive Preview** to edit local field values, toggle checkboxes, and test form behaviors
inside the existing canvas layout. The runtime frame provides classic Oracle Forms menus, toolbars, and status bar.

---

## 4. Running the CLI Toolkit

You can inspect modules, extract PL/SQL, or query database records directly from your terminal:

```bash
# 1. Execute live query against Oracle Database 23ai Free (:1521)
#    (credentials come from OFW_DB_USER / OFW_DB_PASSWORD; there are no defaults)
OFW_DB_USER=forms_user OFW_DB_PASSWORD='<password>' \
  cargo run -p oracle-forms-cli -- query samples/test_db_fmb.xml --backend oracle-thin

# 2. Execute query offline using mock driver
cargo run -p oracle-forms-cli -- query samples/orders_fmb.xml --backend mock

# 3. Inspect an XML Form (Module summary)
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml

# 4. Inspect a Binary .FMB Module directly (ROS 60050 stream decoding)
cargo run -p oracle-forms-cli -- inspect samples/simple_module.fmb

# 5. Output the complete module index as JSON
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml --json

# 6. Dump all extracted PL/SQL (triggers and procedures) into a directory as .sql files
cargo run -p oracle-forms-cli -- dump-sql samples/orders_fmb.xml -o ./extracted-sql
```

---

## 5. Running the Automated Test Suite

To run all unit, differential, and conformance tests:

```bash
cargo test
```

All 5 integration test suites will execute:
- `test_orders_geometry_and_triggers`
- `test_claims_geometry_and_triggers`
- `test_pld_parsing`
- `test_simple_module_binary_parsing`
- `test_direct_binary_fmb_parsing`

---

## 6. Managing & Restarting the Server Process

### Stop the Server
Press `Ctrl + C` in the terminal where `cargo run` is running.

### Check if Port 7070 is in Use
```bash
lsof -i :7070
```

### Kill any Process on Port 7070
```bash
kill -9 $(lsof -t -i:7070) 2>/dev/null || true
```

### One-Line Server Restart

If you made code/UI changes or encounter `Address already in use (os error 48)`:

```bash
kill -9 $(lsof -t -i:7070) 2>/dev/null || true; cargo run -p oracle-forms-server
```

---

## 7. Running Forms Live via Desktop FSAL (Track 1 Container Testbed)

In addition to the database-free native Rust viewer, this repository includes an official Oracle Forms 14c + WebLogic + Oracle Database 23ai Free testbed running via Podman containers. Forms run as native desktop applications via the **Forms Standalone Launcher (FSAL)** without needing XQuartz, X11 forwarding, or browser plugins.

### Prerequisites

1. **Podman Containers Running**:
   - `oracle23ai-lite` on port 1521
   - `forms-server-14c` on port 7001
2. **Java Runtime on Host**: Java 8, 11, 17, or 21+ (`java -version`).
3. **Python 3**: For running the APM automation scripts.

### 1. Verify Testbed Health

Run the automated health check:
```bash
python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/health_check.py
```
Expected output:
```text
=== Oracle Testbed Health Check ===
[1/3] Checking Podman containers...
  - Oracle Database 23ai: RUNNING (ok)
  - Oracle Forms 14c Server: RUNNING (ok)
[2/3] Checking Forms Listener Servlet (Port 7001)...
  - Forms Listener Servlet: UP AND RUNNING (ok)
[3/3] Checking Database Connection & Query (FREEPDB1)...
  - Database Query test_items: SUCCESS (ok)

All systems operational! Ready to compile and run Forms.
```

### 2. One-Command Form Runner (`run_form.py`)

Convert (if XML), compile (`.fmb` -> `.fmx`), and launch any form immediately in a native desktop window:

```bash
# Run the database-connected test form:
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/test_db.fmb

# Or run with automatic database logon (bypasses logon dialog):
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/test_db.fmb --auto-login

# Run a custom XML form:
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/simple_module_fmb.xml
```

### 3. Step-by-Step Manual Workflow

If you prefer to compile and launch separately:

#### Step A: Headless Batch Compile
```bash
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/compile_form.py samples/test_db.fmb
```

#### Step B: Launch Desktop FSAL Client
```bash
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/launch_fsal.py test_db.fmx
```

### 4. Database Logon Credentials

When prompted by the native Forms **Logon** dialog:
- **Username**: `forms_user`
- **Password**: `Welcome123#`
- **Database**: `FREEPDB1`

Click **Connect** to load the form and execute live PL/SQL triggers against Oracle Database 23ai.

