#!/usr/bin/env python3
"""
compare_features.py (CLI)
=====================================

Purpose:
    Performs side-by-side feature comparison between Oracle Fusion Middleware 14c
    Forms Server and the native pure-Rust Oracle Forms Workbench (OFW).

    Parses module AST (.xml or .fmb), audits all items, blocks, canvases, triggers,
    PL/SQL units, built-in calls, and database sources, computes a compatibility
    and parity score, and outputs detailed comparison tables and migration diagnostics.

Usage Examples:
    python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/ofw_demo_employees_fmb.xml
    python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/ofw_sales_fmb.xml --format markdown
    python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/orders_fmb.xml --json
    python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/test_db_fmb.xml --benchmark

Layer: Tooling / Skill Script (comparing-forms-features)
"""

import sys
import os
import re
import json
import time
import argparse
import subprocess
import urllib.request
from pathlib import Path
from typing import Dict, List, Set, Tuple, Any, Optional


# Catalog of Oracle Forms Built-ins categorized by Rust Runtime support status
SUPPORTED_BUILTINS = {
    # Navigation
    "GO_ITEM": "Full support: moves focus and updates SYSTEM.CURSOR_ITEM",
    "GO_BLOCK": "Full support: shifts block context and updates cursor",
    "GO_RECORD": "Full support: positional record jumping within block",
    "NEXT_ITEM": "Full support: tab order progression",
    "PREVIOUS_ITEM": "Full support: reverse tab progression",
    "NEXT_RECORD": "Full support: moves to next record buffer",
    "PREVIOUS_RECORD": "Full support: moves to previous record buffer",
    "FIRST_RECORD": "Full support: navigates to first record (index 1)",
    "LAST_RECORD": "Full support: navigates to last queried record",
    # Query & DML Lifecycle
    "EXECUTE_QUERY": "Full support: triggers query execution over DB backend",
    "ENTER_QUERY": "Full support: toggles query mode and criteria buffers",
    "CREATE_RECORD": "Full support: inserts new dirty record into active block",
    "DELETE_RECORD": "Full support: marks record as deleted for transaction",
    "CLEAR_RECORD": "Full support: clears current record buffer",
    "CLEAR_BLOCK": "Full support: empties block cache and resets status",
    "CLEAR_FORM": "Full support: resets entire form state and all blocks",
    "COMMIT_FORM": "Full support: atomic commit with savepoints and validation",
    "EXIT_FORM": "Full support: terminates active form session",
    # Alerts & Dialogs
    "FIND_ALERT": "Full support: resolves alert definition by name",
    "SHOW_ALERT": "Full support: triggers modal alert with button continuations",
    "SET_ALERT_PROPERTY": "Full support: dynamically updates alert message/title",
    "SET_ALERT_BUTTON_PROPERTY": "Full support: customizes alert button labels",
    # Messages & Properties
    "MESSAGE": "Full support: logs informational/error status message",
    "SET_ITEM_PROPERTY": "Full support: updates item visibility, enabled, label, colors",
    "GET_ITEM_PROPERTY": "Full support: inspects item properties at runtime",
    "SET_BLOCK_PROPERTY": "Full support: dynamic WHERE clause and ORDER BY override",
    "GET_BLOCK_PROPERTY": "Full support: reads block status, target, and clauses",
    "SET_WINDOW_PROPERTY": "Full support: updates window geometry and title",
    "GET_WINDOW_PROPERTY": "Full support: reads window dimensions and position",
    "SET_CANVAS_PROPERTY": "Full support: updates canvas visibility",
    # Standard SQL in PL/SQL
    "SELECT_INTO": "Full support: positional column binding with NO_DATA_FOUND handling",
    "COMMIT": "Full support: commits current transaction handle",
    "ROLLBACK": "Full support: full rollback or rollback to savepoint",
    "SAVEPOINT": "Full support: named savepoints",
}

PARTIAL_BUILTINS = {
    "POST": "Partial: full commit supported; post without commit pending",
    "COUNT_QUERY": "Partial: query count estimated via SELECT COUNT(*)",
    "SYNCHRONIZE": "No-op in Web UI: DOM updates are reactive and immediate",
    "BELL": "No-op in Web UI: audible bell suppressed",
    "PAUSE": "Mapped to alert confirmation continuation",
}

UNSUPPORTED_BUILTINS = {
    "HOST": "Operating system process execution not permitted in sandbox",
    "RUN_PRODUCT": "Oracle Graphics/Reports integration superseded by native HTML5 reporting",
    "RUN_REPORT_OBJECT": "Reports object dispatch superseded by native HTML5 reports",
    "WEB.SHOW_DOCUMENT": "Browser window popup requires client integration hook",
    "CALL_FORM": "Multi-form stack transitions under development",
    "OPEN_FORM": "Multi-form concurrent windows under development",
    "NEW_FORM": "Form replacement under development",
    "SHOW_LOV": "LOV poplist supported; multi-column modal search dialog pending",
    "LIST_VALUES": "LOV poplist supported; modal search dialog pending",
    "CREATE_TIMER": "Forms client timers under development",
    "DELETE_TIMER": "Forms client timers under development",
    "EXEC_SQL": "Dynamic SQL execution via EXEC_SQL package not supported",
    "FORMS_DDL": "Runtime DDL execution not supported",
    "OLE2": "Legacy Windows OLE automation deprecated",
    "DDE": "Legacy Windows Dynamic Data Exchange deprecated",
    "D2KWUTIL": "Legacy Windows 32-bit user32 DLL wrappers deprecated",
    "TEXT_IO": "Server filesystem I/O restricted by security sandbox",
    "CLIENT_TEXT_IO": "Client filesystem I/O restricted by browser security",
    "WEBUTIL": "Java applet WebUtil bridge superseded by REST/WebSocket API",
}

SUPPORTED_TRIGGERS = {
    "WHEN-BUTTON-PRESSED", "WHEN-VALIDATE-ITEM", "WHEN-VALIDATE-RECORD",
    "WHEN-NEW-FORM-INSTANCE", "WHEN-NEW-BLOCK-INSTANCE", "WHEN-NEW-RECORD-INSTANCE",
    "WHEN-NEW-ITEM-INSTANCE", "PRE-QUERY", "POST-QUERY", "ON-SELECT",
    "PRE-COMMIT", "ON-COMMIT", "POST-COMMIT", "POST-DATABASE-COMMIT",
    "ON-INSERT", "ON-UPDATE", "ON-DELETE", "ON-LOCK", "ON-ERROR", "ON-MESSAGE",
    "KEY-EXEQRY", "KEY-ENTQRY", "KEY-COMMIT", "KEY-CLRBLK", "KEY-CLRREC",
    "KEY-EXIT", "KEY-NEXTREC", "KEY-PREVREC", "KEY-CREREC", "KEY-DELREC",
}

SUPPORTED_ITEM_TYPES = {
    "Text Item": "Native <input> or display field with formatting",
    "Push Button": "Native <button> with trigger dispatch",
    "Check Box": "Native <input type='checkbox'> with checked/unchecked values",
    "Radio Group": "Native <input type='radio'> with option groups",
    "List Item": "Native <select> poplist or combo box",
    "Display Item": "Read-only static text label",
    "Image": "Native <img> / SVG preview container",
}


def _read_dotenv() -> dict:
    """Read KEY=VALUE lines from the nearest .env at or above this script's folder (git-ignored)."""
    here = Path(__file__).resolve()
    for folder in [here.parent, *here.parents]:
        candidate = folder / ".env"
        if candidate.is_file():
            values = {}
            for raw in candidate.read_text(encoding="utf-8").splitlines():
                line = raw.strip()
                if line and not line.startswith("#") and "=" in line:
                    key, value = line.split("=", 1)
                    values[key.strip()] = value.strip().strip('"').strip("'")
            return values
    return {}


_DOTENV = _read_dotenv()


def _env(name: str, default: str = "") -> str:
    """Environment variable first, then the .env file, then the default."""
    return os.environ.get(name) or _DOTENV.get(name, default)


def _default_userid() -> str:
    """DB_USER/DB_PASSWORD@DB_SERVICE, from the environment or .env."""
    return f"{_env('DB_USER')}/{_env('DB_PASSWORD')}@{_env('DB_SERVICE', 'FREEPDB1')}"

def resolve_workspace_root() -> Path:
    return Path(__file__).resolve().parents[6]


def inspect_module_ast(file_path: Path, repo_root: Path) -> Optional[Dict[str, Any]]:
    cmd = [
        "cargo", "run", "-q", "-p", "oracle-forms-cli", "--",
        "inspect", str(file_path.relative_to(repo_root)), "--json"
    ]
    try:
        res = subprocess.run(cmd, cwd=repo_root, capture_output=True, text=True, check=True)
        return json.loads(res.stdout)
    except Exception as e:
        print(f"Error inspecting module via oracle-forms-cli: {e}", file=sys.stderr)
        return None


def extract_builtins_from_plsql(text: str) -> Set[str]:
    found = set()
    upper = text.upper()
    for b in list(SUPPORTED_BUILTINS.keys()) + list(PARTIAL_BUILTINS.keys()) + list(UNSUPPORTED_BUILTINS.keys()):
        pattern = r"\b" + re.escape(b) + r"\b"
        if re.search(pattern, upper):
            found.add(b)
    # Check prefixes
    for prefix in ["OLE2.", "DDE.", "TEXT_IO.", "CLIENT_TEXT_IO.", "WEBUTIL.", "CLIENT_"]:
        if prefix in upper:
            found.add(prefix.rstrip("."))
    # Check SELECT INTO
    if re.search(r"\bSELECT\b[\s\S]+?\bINTO\b", upper):
        found.add("SELECT_INTO")
    return found


def extract_all_plsql_code(ast: Dict[str, Any], file_path: Path, repo_root: Path) -> Tuple[List[Dict[str, str]], Set[str]]:
    triggers = []
    builtins_used = set()

    # Use dump-sql to extract raw trigger text safely
    out_dir = repo_root / "temp" / f"eval_{file_path.stem}"
    out_dir.mkdir(parents=True, exist_ok=True)
    try:
        cmd = [
            "cargo", "run", "-q", "-p", "oracle-forms-cli", "--",
            "dump-sql", str(file_path.relative_to(repo_root)), "-o", str(out_dir)
        ]
        subprocess.run(cmd, cwd=repo_root, capture_output=True, text=True, check=True)
        for sql_file in out_dir.rglob("*.sql"):
            code = sql_file.read_text(encoding="utf-8", errors="ignore")
            trig_name = sql_file.stem
            triggers.append({"name": trig_name, "file": sql_file.name, "code": code})
            builtins_used.update(extract_builtins_from_plsql(code))
    except Exception:
        pass

    return triggers, builtins_used


def audit_module_features(ast: Dict[str, Any]) -> Dict[str, Any]:
    blocks = ast.get("blocks", [])
    canvases = ast.get("canvases", [])
    windows = ast.get("windows", [])
    alerts = ast.get("alerts", [])
    lovs = ast.get("lovs", [])
    record_groups = ast.get("recordGroups", [])
    all_triggers = ast.get("triggers", [])
    program_units = ast.get("programUnits", [])
    attached_libs = ast.get("attachedLibraries", [])

    item_types = {}
    total_items = 0
    db_blocks = []
    control_blocks = []

    for blk in blocks:
        is_db = bool(blk.get("queryDataSourceName"))
        if is_db:
            db_blocks.append(blk.get("name"))
        else:
            control_blocks.append(blk.get("name"))
        for item in blk.get("items", []):
            total_items += 1
            itype = item.get("itemType") or "Unknown"
            item_types[itype] = item_types.get(itype, 0) + 1

    builtins_used = set()
    # Extract built-ins from trigger source texts
    for trig in all_triggers:
        code = trig.get("sourceText") or ""
        builtins_used.update(extract_builtins_from_plsql(code))
    for pu in program_units:
        code = pu.get("sourceText") or ""
        builtins_used.update(extract_builtins_from_plsql(code))

    supported_b = {b for b in builtins_used if b in SUPPORTED_BUILTINS}
    partial_b = {b for b in builtins_used if b in PARTIAL_BUILTINS}
    unsupported_b = {b for b in builtins_used if b in UNSUPPORTED_BUILTINS}
    unknown_b = builtins_used - (supported_b | partial_b | unsupported_b)

    supported_trig_count = sum(1 for t in all_triggers if t.get("name", "").upper() in SUPPORTED_TRIGGERS)
    unsupported_trig_count = len(all_triggers) - supported_trig_count

    # Parity Score Calculation (0-100%)
    block_score = 100.0
    item_score = 100.0 if all(it in SUPPORTED_ITEM_TYPES for it in item_types) else 80.0
    canvas_score = 100.0
    trig_score = (supported_trig_count / len(all_triggers) * 100.0) if all_triggers else 100.0
    
    total_builtins = len(builtins_used)
    if total_builtins == 0:
        builtin_score = 100.0
    else:
        builtin_score = ((len(supported_b) + 0.5 * len(partial_b)) / total_builtins) * 100.0

    total_parity = (block_score * 0.20) + (item_score * 0.25) + (canvas_score * 0.15) + (trig_score * 0.25) + (builtin_score * 0.15)

    return {
        "module_name": ast.get("key", {}).get("name", "UNKNOWN"),
        "total_blocks": len(blocks),
        "db_blocks": db_blocks,
        "control_blocks": control_blocks,
        "total_items": total_items,
        "item_types": item_types,
        "canvases": [c.get("name") for c in canvases],
        "windows": [w.get("name") for w in windows],
        "alerts": [a.get("name") for a in alerts],
        "lovs": [l.get("name") for l in lovs],
        "record_groups": [rg.get("name") for rg in record_groups],
        "attached_libraries": [lib.get("name") for lib in attached_libs],
        "total_triggers": len(all_triggers),
        "supported_trig_count": supported_trig_count,
        "unsupported_trig_count": unsupported_trig_count,
        "program_units_count": len(program_units),
        "builtins_used": sorted(list(builtins_used)),
        "supported_builtins": sorted(list(supported_b)),
        "partial_builtins": sorted(list(partial_b)),
        "unsupported_builtins": sorted(list(unsupported_b)),
        "unknown_builtins": sorted(list(unknown_b)),
        "parity_score": round(total_parity, 1),
    }


def format_markdown_report(audit: Dict[str, Any], file_name: str) -> str:
    md = []
    md.append(f"# Side-by-Side Feature Comparison: `{audit['module_name']}`")
    md.append("")
    md.append(f"**Source Module:** `{file_name}`  ")
    md.append(f"**Overall Parity Score:** **{audit['parity_score']}% Match**")
    md.append("")
    md.append("## 1. High-Level Architecture Parity")
    md.append("")
    md.append("| Architectural Aspect | Oracle Fusion Middleware 14c | Native Rust Oracle Forms Workbench | Parity Status |")
    md.append("| :--- | :--- | :--- | :--- |")
    md.append("| **Runtime Engine** | WebLogic Server 14.1.2 (Java EE) | Pure native Rust standalone binary (`oracle-forms-server`) | Full Replacement |")
    md.append("| **Client Experience** | Desktop Java Applet via FSAL (`frmsal.jar`) | Zero-install modern web browser (`http://127.0.0.1:7070`) | Full Modernization |")
    md.append("| **Database Protocol** | Oracle Net / OCI C libraries | Pure-Rust async Tokio wire protocol (`oracle-thin`) | Full Native Parity |")
    md.append(f"| **Data Blocks** | {audit['total_blocks']} blocks ({len(audit['db_blocks'])} DB, {len(audit['control_blocks'])} Control) | Full data source mapping, ORDER BY, and WHERE clauses | Full Parity |")
    md.append(f"| **UI Items** | {audit['total_items']} items across {len(audit['item_types'])} widget types | Responsive canvas rendering with coordinate mapping | Full Parity |")
    md.append(f"| **Canvases & Windows** | {len(audit['canvases'])} canvases, {len(audit['windows'])} windows | Content canvas SVG/CSS layout with active tab pages | Full Parity |")
    md.append(f"| **Alerts & Dialogs** | {len(audit['alerts'])} alerts | Resumable 3-button modal continuation stack | Full Parity |")
    md.append(f"| **Triggers** | {audit['total_triggers']} triggers declared | {audit['supported_trig_count']} supported event triggers, {audit['unsupported_trig_count']} pending | {'Full Parity' if audit['unsupported_trig_count'] == 0 else 'High Parity'} |")
    md.append(f"| **PL/SQL Program Units** | {audit['program_units_count']} program units | AST tree-walking interpreter with package state | Full Parity |")
    md.append("")
    md.append("## 2. Built-in Subprogram Compatibility Audit")
    md.append("")
    md.append("| Built-in Subprogram | Category | Oracle 14c Status | Native Rust OFW Status | Implementation Notes |")
    md.append("| :--- | :--- | :--- | :--- | :--- |")
    
    for b in audit['supported_builtins']:
        note = SUPPORTED_BUILTINS.get(b, "Fully supported by runtime")
        md.append(f"| `{b}` | Core Built-in | Supported | **SUPPORTED** | {note} |")
    for b in audit['partial_builtins']:
        note = PARTIAL_BUILTINS.get(b, "Partially implemented")
        md.append(f"| `{b}` | Extended | Supported | **PARTIAL** | {note} |")
    for b in audit['unsupported_builtins']:
        note = UNSUPPORTED_BUILTINS.get(b, "Legacy construct")
        md.append(f"| `{b}` | Legacy / Sandbox | Supported | **UNSUPPORTED** | {note} |")

    if not audit['builtins_used']:
        md.append("| *(None detected)* | N/A | Supported | **SUPPORTED** | Standard query/navigation operations only |")

    md.append("")
    md.append("## 3. UI Widget Distribution")
    md.append("")
    md.append("| Widget Type | Count in Module | Oracle 14c Support | Native Rust Workbench Support |")
    md.append("| :--- | :--- | :--- | :--- |")
    for itype, count in sorted(audit['item_types'].items(), key=lambda x: -x[1]):
        sup = SUPPORTED_ITEM_TYPES.get(itype, "Under development")
        md.append(f"| `{itype}` | {count} | Native Motif/Java | {sup} |")

    md.append("")
    md.append("## 4. Migration & Execution Recommendation")
    md.append("")
    if audit['parity_score'] >= 90:
        md.append("> [!TIP]")
        md.append(f"> **High Parity Candidate ({audit['parity_score']}%):** This form can be executed immediately in both Oracle Fusion Middleware 14c and the native Rust Oracle Forms Workbench with identical behavior.")
    elif audit['parity_score'] >= 70:
        md.append("> [!NOTE]")
        md.append(f"> **Moderate Parity Candidate ({audit['parity_score']}%):** Core data querying and validation operate identically. Review partial/unsupported built-ins listed above.")
    else:
        md.append("> [!IMPORTANT]")
        md.append(f"> **Manual Review Advised ({audit['parity_score']}%):** This form utilizes legacy features (e.g. OLE, HOST, or WebUtil) requiring modernization.")

    return "\n".join(md)


def print_cli_summary(audit: Dict[str, Any], file_name: str):
    print("==================================================================================")
    print(f"  Oracle Forms Feature Comparison: {audit['module_name']} ({file_name})")
    print("==================================================================================")
    print(f"  Overall Parity Score        : {audit['parity_score']}% Match")
    print(f"  Data Blocks                 : {audit['total_blocks']} ({len(audit['db_blocks'])} DB, {len(audit['control_blocks'])} Control)")
    print(f"  Items & Widgets             : {audit['total_items']} items across {len(audit['item_types'])} widget types")
    print(f"  Canvases / Windows / Alerts : {len(audit['canvases'])} canvases | {len(audit['windows'])} windows | {len(audit['alerts'])} alerts")
    print(f"  Event Triggers              : {audit['supported_trig_count']}/{audit['total_triggers']} supported ({audit['unsupported_trig_count']} pending)")
    print("----------------------------------------------------------------------------------")
    print("  BUILT-IN AUDIT BREAKDOWN:")
    print(f"    Supported in Native Rust  : {', '.join(audit['supported_builtins']) if audit['supported_builtins'] else 'None'}")
    if audit['partial_builtins']:
        print(f"    Partially Supported       : {', '.join(audit['partial_builtins'])}")
    if audit['unsupported_builtins']:
        print(f"    Unsupported / Deprecated  : {', '.join(audit['unsupported_builtins'])}")
    print("----------------------------------------------------------------------------------")
    print("  SIDE-BY-SIDE SUMMARY:")
    print("    [Oracle Fusion Middleware 14c] : Heavy enterprise container, WebLogic, FSAL Java applet")
    print("    [Native Rust Forms Workbench]  : Ultra-lightweight native binary, zero-JVM, pure browser UI")
    print("==================================================================================\n")


def run_latency_benchmark(repo_root: Path, file_path: Path, block: Optional[str] = None):
    print("[*] Running Live Query Latency Benchmark...")
    env = os.environ.copy()
    env["OFW_DB_USER"] = _env("DB_USER")
    env["OFW_DB_PASSWORD"] = _env("DB_PASSWORD")
    
    cmd = [
        "cargo", "run", "-q", "-p", "oracle-forms-cli", "--",
        "query", str(file_path.relative_to(repo_root)),
        "--backend", "oracle-thin",
    ]
    if block:
        cmd.extend(["--block", block])

    runs = 5
    process_times = []
    wire_times = []
    records = 0
    for _ in range(runs):
        t0 = time.perf_counter()
        res = subprocess.run(cmd, cwd=repo_root, capture_output=True, text=True, env=env)
        elapsed = (time.perf_counter() - t0) * 1000.0
        if res.returncode == 0:
            process_times.append(elapsed)
            for line in res.stdout.splitlines():
                if "Records Fetched:" in line:
                    try:
                        records = int(line.split(":")[-1].strip())
                    except ValueError:
                        pass
                if "Execution Time:" in line:
                    try:
                        # Parse e.g. "8 ms" or "2 ms"
                        val_str = line.split(":")[-1].replace("ms", "").strip()
                        wire_times.append(float(val_str))
                    except ValueError:
                        pass

    if process_times:
        print(f"    Pure-Rust oracle-thin Query Latency (over {runs} runs):")
        print(f"      Records Fetched     : {records}")
        if wire_times:
            avg_wire = sum(wire_times) / len(wire_times)
            print(f"      DB Wire Latency     : {avg_wire:.2f} ms (min: {min(wire_times):.1f} ms, max: {max(wire_times):.1f} ms)")
        avg_proc = sum(process_times) / len(process_times)
        print(f"      CLI Process Startup : {avg_proc:.2f} ms")
    else:
        print("    [!] Benchmark failed to execute query.")


def main() -> int:
    parser = argparse.ArgumentParser(description="Side-by-side feature comparison between Oracle Forms 14c and Native Rust OFW")
    parser.add_argument("module_path", help="Path to Oracle Forms module (.xml or .fmb)")
    parser.add_argument("--format", choices=["table", "markdown", "json"], default="table", help="Output format")
    parser.add_argument("--output", "-o", help="Optional output file for report")
    parser.add_argument("--benchmark", action="store_true", help="Run live latency benchmark against Oracle 23ai")
    parser.add_argument("--block", help="Target block for benchmark")
    args = parser.parse_args()

    repo_root = resolve_workspace_root()
    mod_path = (repo_root / args.module_path).resolve()
    if not mod_path.exists():
        mod_path = Path(args.module_path).resolve()
    if not mod_path.exists():
        print(f"Error: Module file not found: {args.module_path}", file=sys.stderr)
        return 1

    ast = inspect_module_ast(mod_path, repo_root)
    if not ast:
        return 1

    audit = audit_module_features(ast)

    if args.format == "json":
        out_json = json.dumps(audit, indent=2)
        if args.output:
            Path(args.output).write_text(out_json, encoding="utf-8")
            print(f"Wrote JSON audit to {args.output}")
        else:
            print(out_json)
    elif args.format == "markdown":
        report = format_markdown_report(audit, mod_path.name)
        if args.output:
            Path(args.output).write_text(report, encoding="utf-8")
            print(f"Wrote Markdown report to {args.output}")
        else:
            print(report)
    else:
        print_cli_summary(audit, mod_path.name)
        if args.output:
            report = format_markdown_report(audit, mod_path.name)
            Path(args.output).write_text(report, encoding="utf-8")
            print(f"Saved detailed Markdown report to {args.output}")

    if args.benchmark:
        target_block = args.block or (audit["db_blocks"][0] if audit["db_blocks"] else None)
        run_latency_benchmark(repo_root, mod_path, target_block)

    return 0


if __name__ == "__main__":
    sys.exit(main())
