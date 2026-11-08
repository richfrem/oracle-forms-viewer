# How to Run the Native Rust Oracle Forms Viewer

This guide explains how to compile, test, and run the **`oracle-forms-viewer`** native Rust server and CLI.

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

Navigate to the `oracle-forms-viewer` workspace directory and run:

```bash
cd oracle-forms-viewer
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
cd oracle-forms-viewer
PORT=8080 cargo run -p oracle-forms-server
```

---

## 3. Using the Web Viewer

Once the server is running at `http://127.0.0.1:7070`:

1. **Open Sample Forms**:
   - Use the **"Select Sample"** dropdown in the top header.
   - Choose any sample (e.g. `orders_fmb.xml`, `claims_fmb.xml`, `simple_module.fmb`, or `rccm0000.fmb`).
2. **Open Your Own Files**:
   - Drag & drop any `.fmb`, `.xml`, `.pld`, `.mmb`, or `.olb` file into the browser window.
   - Or type a local file path into the **"File Path"** input box and click **"Open"**.
3. **Interactive Features**:
   - **Canvas View**: Pan, zoom, and visually inspect 2D coordinates, buttons, frames, prompts, and layout elements.
   - **Object Navigator**: Tree navigation across Blocks, Items, Triggers, Program Units, LOVs, and Alerts.
   - **PL/SQL Tab**: Syntax-highlighted view of procedures, functions, package bodies, and triggers with clean line numbers.
   - **XML Tab**: Direct view of raw XML elements and line boundaries.

### Database-Free Interactive Preview

Enable **Interactive Preview** to edit local field values and toggle checkboxes
inside the existing canvas layout. Values are separate for each displayed record
and survive canvas and tab switches. **Reset Preview** restores initial values;
opening another form clears all local edits. Disabling preview returns to inspection.

The runtime frame is enabled automatically. Buttons, toolbar actions, and custom
menu commands show a **not executed** notice; associated source can be inspected.
Use **Preview Alert...** to display exported alert messages. This is a UI simulation:
no database connection, PL/SQL execution, inferred application navigation, or
query-generated data. Disabled/non-enterable fields remain disabled/read-only,
and hidden/off-canvas controls are omitted. Lists and radio groups without parsed
choices are explicitly unavailable rather than populated with invented values.

For custom menus, open the form first, then choose **Load Menu XML** and select
its converted `*_mmb.xml` file. The form stays loaded and menu source remains
available in the PL/SQL viewer. A browser upload cannot automatically access
companion files in its original directory; opening a local form path can resolve
its existing sibling form/menu XML. Without menu definitions, the runtime frame
shows the standard placeholder menus, not the application's custom menu.

---

## 4. Running the CLI Toolkit

You can inspect modules or extract PL/SQL directly from your terminal:

```bash
cd oracle-forms-viewer

# 1. Inspect an XML Form (Module summary)
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml

# 2. Inspect a Binary .FMB Module (ROS 60050 stream decoding)
cargo run -p oracle-forms-cli -- inspect samples/simple_module.fmb

# 3. Output the complete module index as JSON
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml --json

# 4. Dump all extracted PL/SQL (triggers and procedures) into a directory as .sql files
cargo run -p oracle-forms-cli -- dump-sql samples/orders_fmb.xml -o ./extracted-sql
```

---

## 5. Running the Automated Test Suite

To run all unit, differential, and conformance tests:

```bash
cd oracle-forms-viewer
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

**From repository root (`oracle-forms-fmb-research`):**
```bash
kill -9 $(lsof -t -i:7070) 2>/dev/null || true; cd oracle-forms-viewer && cargo run -p oracle-forms-server
```

**If already inside `oracle-forms-viewer/`:**
```bash
kill -9 $(lsof -t -i:7070) 2>/dev/null || true; cargo run -p oracle-forms-server
```
