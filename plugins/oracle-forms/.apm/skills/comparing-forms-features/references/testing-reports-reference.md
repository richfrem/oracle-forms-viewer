# Testing Reference: Oracle Reports (14c Server vs Native Rust Reports Engine)

Operational guide for evaluating, executing, and benchmarking Oracle Reports (`.rdf`, `.xml`, `.rpt`) across legacy Oracle Reports Server and the native pure-Rust Reports Engine.

---

## 1. Oracle Reports Architecture Comparison

| Dimension | Legacy Oracle Reports 12c / 14c | Native Pure-Rust Reports Engine (OFW) |
| :--- | :--- | :--- |
| **Execution Engine** | `rwserver` / `rwservlet` (C++ / Java daemon) | Pure-Rust async evaluator (`oracle-forms-runtime/report_evaluator`) |
| **Output Formats** | PostScript, PDF (heavy generator), HTMLCSS | Clean semantic HTML5, SVG charts, native browser PDF printing |
| **Database Access** | OCI / SQL*Net via Reports Client | Pure-Rust async Tokio wire protocol (`oracle-thin`) |
| **Data Model Engine** | Proprietary multi-query matrix & group breaks | Streaming SQL executor with hierarchical group aggregations |
| **Summary Columns** | In-engine accumulators (`SUM`, `AVG`, `COUNT`, `MIN`, `MAX`) | Exact decimal summary accumulators with group reset levels |
| **Deployment** | Multi-gigabyte Fusion Middleware Reports Tools | Integrated zero-dependency submodule in Axum server & CLI |

---

## 2. Testing Reports via Standalone CLI

The `oracle-forms-cli` contains a dedicated `report` subcommand that evaluates an Oracle Report XML definition, executes its queries against the live database, calculates all summary and formula columns, and writes a self-contained HTML5 document.

### Command Syntax:
```bash
cargo run -p oracle-forms-cli -- report <REPORT_XML> [OPTIONS]
```

### Options:
- `--backend <oracle-thin|sqlite|mock>`: Database backend (default: `mock`).
- `-o, --output <FILE>`: Destination HTML file (default: stdout).
- `-p, --param <KEY=VALUE>`: Supply report runtime parameters (e.g. `-p P_DEPT_ID=10`).

### Examples:
```bash
# Evaluate Customer Orders Summary report against live Oracle 23ai:
OFW_DB_USER=forms_user OFW_DB_PASSWORD=Welcome123# cargo run -p oracle-forms-cli -- report samples/orders_summary_report.xml --backend oracle-thin -o orders_summary.html

# Evaluate Department Salaries master-detail report:
OFW_DB_USER=forms_user OFW_DB_PASSWORD=Welcome123# cargo run -p oracle-forms-cli -- report samples/department_salaries_report.xml --backend oracle-thin -o dept_salaries.html
```

Open the generated report in any browser:
```bash
open orders_summary.html
```

---

## 3. Testing Reports via Browser UI

The native Rust web interface (`http://127.0.0.1:7070`) features an integrated **Reports Engine** tab:

1. **Start Server**:
   ```bash
   OFW_SERVER_ENV_CREDENTIALS=1 OFW_DB_USER=forms_user OFW_DB_PASSWORD=Welcome123# cargo run -p oracle-forms-server
   ```
2. **Navigate**: Open [http://127.0.0.1:7070](http://127.0.0.1:7070) in your browser.
3. **Switch Tab**: Click the **Reports Engine** tab in the main tab bar (next to *Visual Canvas Preview*, *PL/SQL Viewer*, *Raw XML Slice*, *Form Diff Tool*).
4. **Select Report**:
   - `ORDERS_SUMMARY - Customer Orders Summary Report`
   - `DEPARTMENT_SALARIES - Department Salaries Master-Detail Report`
5. **Select Backend**: Choose `Oracle Thin (Pure-Rust :1521)` from the dropdown.
6. **Click ▶ Run Report**:
   - Executes the report data model query via async Tokio TCP.
   - Computes group breaks and summary calculations.
   - Renders the interactive report canvas in the center pane.
   - Displays calculated summary metrics (`CS_AVG_ORDER`, `CS_TOTAL_SALES`, `CS_ORDER_COUNT`) and record groups in the right-hand **Data Model & Summaries** sidebar.
7. **Print / PDF**: Click **Print / Save as PDF** to invoke browser print styles configured for page headers, margins, and column alignment.

---

## 4. Converting Binary Reports (.rdf -> .xml)

If you have legacy binary `.rdf` files, convert them to XML using the containerized conversion tool:

```bash
# Convert .rdf to XML via Reports Converter:
python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/convert_rpt.py samples/invoice.rdf
```
This produces `samples/invoice_rdf.xml`, which can be immediately evaluated by the native Rust CLI and Reports Engine.

---

## 5. Reports Verification Checklist

| Verification Item | What to Look For | Expected Outcome |
| :--- | :--- | :--- |
| **SQL Data Model** | Correct query execution | All rows retrieved matching report filters |
| **Summary Accumulators** | `CS_TOTAL_SALES`, `CS_AVG_ORDER` | Exact decimal precision without floating-point drift |
| **Group Breaks** | Parent-child records (e.g. Department -> Employees) | Nested grouping renders distinct section headers |
| **Formatting Masks** | Date & currency formatting | Dates render as `YYYY-MM-DD`, currencies with commas and `$` |
| **Print Quality** | Browser Print Preview | Clean page margins, repeating headers, and zero UI chrome |
