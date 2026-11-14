#!/usr/bin/env python3
"""
test_parity.py (CLI)
=====================================

Purpose:
    End-to-end parity tester comparing Oracle Forms modules running on:
      1. Oracle Fusion Middleware 14c Forms Server (WebLogic 14.1.2 + FSAL)
      2. Native Rust Oracle Forms Workbench (oracle-forms-server + oracle-thin)

    Validates schema seeding in Oracle 23ai, headless compilation (.xml -> .fmb -> .fmx)
    via the 14c container toolchain, staging to WebLogic Forms Server, execution
    via the native Rust engine, and provides live links/commands to test both engines.

Usage Examples:
    python3 plugins/oracle-forms/.apm/skills/testing-forms-parity/scripts/test_parity.py samples/ofw_demo_employees_fmb.xml
    python3 plugins/oracle-forms/.apm/skills/testing-forms-parity/scripts/test_parity.py samples/ofw_sales_fmb.xml
    python3 plugins/oracle-forms/.apm/skills/testing-forms-parity/scripts/test_parity.py samples/test_db_fmb.xml --block BLOCK5

Layer: Tooling / Skill Script (testing-forms-parity)
"""

import sys
import os
import argparse
import subprocess
import urllib.request
import urllib.error
from pathlib import Path
from typing import Optional, Tuple, Dict, Any


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


def check_container(name: str) -> bool:
    try:
        res = subprocess.run(
            ["podman", "ps", "--filter", f"name={name}", "--format", "{{.Names}}"],
            capture_output=True,
            text=True,
            check=True,
        )
        return name in res.stdout
    except Exception:
        return False


def seed_database_if_present(sql_path: Path, repo_root: Path) -> bool:
    if not sql_path.exists():
        return True
    print(f"[*] Seeding database using {sql_path.name}...")
    try:
        cmd = [
            "podman", "exec", "-i", "oracle23ai-lite",
            "sqlplus", "-s", f"{_env('DB_USER')}/{_env('DB_PASSWORD')}@localhost:1521/{_env('DB_SERVICE', 'FREEPDB1')}"
        ]
        with open(sql_path, "rb") as f:
            proc = subprocess.run(cmd, stdin=f, capture_output=True, text=True, check=True)
        print(f"    Schema seeded successfully.")
        return True
    except subprocess.CalledProcessError as e:
        print(f"[!] Warning: database seeding had output:\n{e.stderr or e.stdout}")
        return True


def convert_xml_to_fmb(xml_path: Path, repo_root: Path) -> Optional[Path]:
    fmb_stem = xml_path.stem.replace("_fmb", "")
    fmb_path = xml_path.parent / f"{fmb_stem}.fmb"
    rel_xml = xml_path.relative_to(repo_root)
    
    print(f"[*] Converting XML -> FMB via container XML2Forms: {rel_xml}")
    cmd = [
        "podman", "exec", "forms-server-14c", "/bin/bash", "-c",
        f"""
        export ORACLE_HOME=/oracle_home_14c
        export LD_LIBRARY_PATH=$ORACLE_HOME/lib:$LD_LIBRARY_PATH
        export FORMS_PATH=/work:$FORMS_PATH
        export FORMS_API_TK_BYPASS=TRUE
        CLASSPATH="$ORACLE_HOME/jlib/frmjdapi.jar:$ORACLE_HOME/jlib/frmxmltools.jar:$ORACLE_HOME/oracle_common/modules/oracle.xdk/xmlparserv2.jar"
        java -Djava.library.path="$ORACLE_HOME/lib" -classpath "$CLASSPATH" oracle.forms.util.xmltools.XML2Forms OVERWRITE=YES /work/{rel_xml}
        """
    ]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0 or not fmb_path.exists():
        print(f"[!] XML2Forms failed: {res.stderr or res.stdout}")
        return None
    print(f"    Generated {fmb_path.name}")
    return fmb_path


def compile_fmb_to_fmx(fmb_path: Path, repo_root: Path) -> Optional[Path]:
    fmx_path = fmb_path.parent / f"{fmb_path.stem}.fmx"
    rel_fmb = fmb_path.relative_to(repo_root)

    print(f"[*] Compiling FMB -> FMX via container frmcmp_batch: {rel_fmb}")
    cmd = [
        "podman", "exec", "forms-server-14c", "/bin/bash", "-c",
        f"""
        export ORACLE_HOME=/oracle_home_14c
        export LD_LIBRARY_PATH=$ORACLE_HOME/lib:$LD_LIBRARY_PATH
        export FORMS_PATH=/work:/work/samples:$ORACLE_HOME/forms:$FORMS_PATH
        export TNS_ADMIN=$ORACLE_HOME/user_projects/domains/base_domain/config/fmwconfig
        /oracle_home_14c/bin/frmcmp_batch module=/work/{rel_fmb} module_type=form compile_all=yes userid='{_default_userid()}'
        """
    ]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if not fmx_path.exists():
        print(f"[!] Compilation failed:\n{res.stdout}\n{res.stderr}")
        return None
    print(f"    Compilation SUCCESSFUL: {fmx_path.name}")

    # Stage into oracle_home_14c/forms
    dest = repo_root / "oracle-reference-stack" / "oracle_home_14c" / "forms" / fmx_path.name
    import shutil
    shutil.copy2(fmx_path, dest)
    shutil.copy2(fmb_path, dest.with_suffix(".fmb"))
    print(f"    Staged to Fusion Middleware Forms directory: {dest.name}")
    return fmx_path


def probe_weblogic_servlet(fmx_name: str, port: int = 7001) -> bool:
    url = f"http://localhost:{port}/forms/frmservlet?config=standaloneapp&form={fmx_name}"
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "OFW-Parity-Probe"})
        with urllib.request.urlopen(req, timeout=5) as resp:
            return resp.status == 200
    except Exception as e:
        return False


def test_rust_cli_query(xml_path: Path, block: Optional[str], repo_root: Path) -> Tuple[bool, str]:
    print(f"[*] Querying form via native Rust CLI (pure-Rust oracle-thin)...")
    cmd = [
        "cargo", "run", "-q", "-p", "oracle-forms-cli", "--",
        "query", str(xml_path.relative_to(repo_root)),
        "--backend", "oracle-thin",
    ]
    if block:
        cmd.extend(["--block", block])
    
    env = os.environ.copy()
    env["OFW_DB_USER"] = _env("DB_USER")
    env["OFW_DB_PASSWORD"] = _env("DB_PASSWORD")
    
    res = subprocess.run(cmd, cwd=repo_root, capture_output=True, text=True, env=env)
    if res.returncode == 0:
        lines = [line for line in res.stdout.splitlines() if "Records Fetched:" in line or "Execution Time:" in line]
        summary = " | ".join(lines) if lines else "Query succeeded"
        return True, summary
    else:
        return False, res.stderr or res.stdout


def main() -> int:
    parser = argparse.ArgumentParser(description="Test parity between native Rust Oracle Forms Workbench and Fusion Middleware 14c")
    parser.add_argument("form_path", help="Path to input .xml or .fmb module file")
    parser.add_argument("--block", help="Specific block to query in Rust CLI")
    parser.add_argument("--port", type=int, default=7001, help="WebLogic AdminServer port")
    parser.add_argument("--skip-fsal", action="store_true", help="Do not launch desktop FSAL GUI")
    args = parser.parse_args()

    repo_root = resolve_workspace_root()
    input_file = (repo_root / args.form_path).resolve()
    if not input_file.exists():
        input_file = Path(args.form_path).resolve()
    if not input_file.exists():
        print(f"Error: Module file not found: {args.form_path}")
        return 1

    print("======================================================================")
    print("  Oracle Forms Workbench <-> Fusion Middleware 14c Parity Verifier")
    print("======================================================================")
    print(f"  Target Module: {input_file.name}")

    # 1. Check Containers
    db_ok = check_container("oracle23ai-lite")
    fmw_ok = check_container("forms-server-14c")
    print(f"  Oracle Database 23ai container : {'[UP]' if db_ok else '[DOWN]'}")
    print(f"  WebLogic Forms 14c container   : {'[UP]' if fmw_ok else '[DOWN]'}")
    if not db_ok or not fmw_ok:
        print("\n[!] Please ensure both containers are running:")
        print("    podman start oracle23ai-lite forms-server-14c")
        return 1

    # 2. Seed matching SQL if available
    sql_candidate = input_file.parent / f"{input_file.stem.replace('_fmb', '')}.sql"
    if not sql_candidate.exists():
        # Try ofw_demo_sales.sql or ofw_demo_employees.sql
        alt_name = input_file.stem.replace("_fmb", "").replace("ofw_", "ofw_demo_") + ".sql"
        alt_path = input_file.parent / alt_name
        if alt_path.exists():
            sql_candidate = alt_path

    if sql_candidate.exists():
        seed_database_if_present(sql_candidate, repo_root)

    # 3. XML -> FMB -> FMX compilation on Fusion Middleware 14c
    fmb_file = input_file
    if input_file.suffix.lower() == ".xml":
        converted = convert_xml_to_fmb(input_file, repo_root)
        if not converted:
            return 1
        fmb_file = converted

    fmx_file = compile_fmb_to_fmx(fmb_file, repo_root)
    if not fmx_file:
        return 1

    # 4. Probe Fusion Middleware Forms Listener Servlet
    servlet_ok = probe_weblogic_servlet(fmx_file.name, args.port)
    print(f"[*] Probing Fusion Middleware HTTP Servlet: {'[200 OK]' if servlet_ok else '[FAILED]'}")

    # 5. Test Native Rust Query Engine
    target_xml = input_file if input_file.suffix.lower() == ".xml" else input_file.with_name(f"{input_file.stem}_fmb.xml")
    rust_ok, rust_summary = False, "No matching XML for Rust CLI"
    if target_xml.exists():
        rust_ok, rust_summary = test_rust_cli_query(target_xml, args.block, repo_root)
        print(f"    Rust Engine Status: {'[SUCCESS]' if rust_ok else '[FAILED]'} ({rust_summary})")

    # 6. Parity Verification Summary & Launch Links
    fsal_url = f"http://localhost:{args.port}/forms/frmservlet?config=standaloneapp&form={fmx_file.name}"
    fsal_cmd = f"java -jar oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar -url \"{fsal_url}\""

    print("\n======================================================================")
    print("  PARITY VERIFICATION RESULTS")
    print("======================================================================")
    print(f"  [1] Fusion Middleware 14c Compilation : PASS ({fmx_file.name})")
    print(f"  [2] Fusion Middleware WebLogic Servlet : {'PASS (HTTP 200)' if servlet_ok else 'FAIL'}")
    print(f"  [3] Native Rust Engine Execution      : {'PASS' if rust_ok else 'FAIL'}")
    print("----------------------------------------------------------------------")
    print("  HOW TO TEST & COMPARE BOTH ENGINES:")
    print("----------------------------------------------------------------------")
    print("  A. Fusion Middleware 14c (Desktop FSAL):")
    print(f"     {fsal_cmd}")
    print(f"     Direct Servlet URL: {fsal_url}")
    print()
    print("  B. Native Rust Workbench (Browser):")
    print("     cargo run -p oracle-forms-server")
    print(f"     Open http://127.0.0.1:7070 -> Select '{target_xml.name if target_xml.exists() else input_file.name}' -> Click [▶ Run Form]")
    print("======================================================================\n")

    return 0 if (servlet_ok and rust_ok) else 1


if __name__ == "__main__":
    sys.exit(main())
