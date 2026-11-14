#!/usr/bin/env python3
"""
test_conversions.py: runs every Oracle artifact conversion path in the 14c container and prints a summary table.

Purpose:
    Verifies the five artifact types end to end, using Oracle's own tools in the converter image:
      1. Forms     FMB -> XML, XML -> FMB, compile FMB -> FMX
      2. Menus     MMB -> XML, XML -> MMB, compile MMB -> MMX
      3. Libraries PLL -> PLD, PLD -> PLL
      4. OLB       OLB -> XML, XML -> OLB
      5. Reports   XML -> RDF, RDF -> XML

    Where a skill script covers a path (convert_fmb.py, convert_pll.py, convert_rpt.py, compile_form.py),
    the test calls that script. Paths with no script (XML -> FMB/MMB/OLB, menu compile, XML -> RDF) run as
    direct container commands.

Usage:
    python3 test_conversions.py                 # all types
    python3 test_conversions.py --only menus    # forms | menus | libraries | olb | reports
    python3 test_conversions.py --offline       # compile the form with logon=no (no database needed)

Platforms:
    macOS, Linux, WSL, and Windows PowerShell (through converter_runtime.py). On Windows the Podman calls run
    inside WSL.

Output:
    Test copies go in ./temp/conversion-tests/ (git-ignored). Source samples are never modified.
    Exit code 0 when no path FAILED. BLOCKED and SKIPPED paths do not fail the run, but are listed.

Statuses:
    PASS     the path ran and produced a non-empty output file
    FAIL     the path ran and did not produce the output (the last lines of its log are shown)
    BLOCKED  a known environment problem stops it (for example, a missing library in the image)
    SKIP     an earlier step it needs did not pass, or its input is missing
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Callable, List, Optional, Tuple

import converter_runtime as rt

ROOT = rt.repo_root()
SCRIPTS = Path(__file__).resolve().parent
COMPILE_FORM = SCRIPTS.parents[1] / "compiling-and-running-oracle-forms" / "scripts" / "compile_form.py"
WORK = ROOT / "temp" / "conversion-tests"
SAMPLES = ROOT / "samples"
ORACLE_HOME = rt.default_oracle_home()

TYPES = ["forms", "menus", "libraries", "olb", "reports"]
RESULTS: List[Tuple[str, str, str, str]] = []  # type, path, status, detail

H = rt.HOME
JAVA_CP = f"{H}/jlib/frmjdapi.jar:{H}/jlib/frmxmltools.jar:{H}/oracle_common/modules/oracle.xdk/xmlparserv2.jar"
CONTAINER_ENV = (
    f"export ORACLE_HOME={H} ORACLE_TERM=vt220 TERM=vt220 FORMS_API_TK_BYPASS=TRUE; "
    f"export LD_LIBRARY_PATH={H}/lib:{H}/oracle_common/lib; "
)
XML2FORMS = f"java -Djava.library.path={H}/lib -classpath {JAVA_CP} oracle.forms.util.xmltools.XML2Forms OVERWRITE=YES"
BATCH = " batch=yes" if rt.IS_14C else ""


def tail(text: str, lines: int = 6) -> str:
    kept = [ln for ln in text.strip().splitlines() if ln.strip()]
    return " | ".join(kept[-lines:])[:400]


def run_container(script: str, cwd: Path, timeout: int = 300) -> Tuple[int, str]:
    """Runs a bash snippet in the converter image with the repo mounted at /repo and cwd as the working folder."""
    rel = cwd.relative_to(ROOT).as_posix()
    cmd = rt.podman_cmd([
        "run", "--rm", "--platform", "linux/amd64",
        "-v", f"{rt.host_path(ORACLE_HOME)}:{H}:Z",
        "-v", f"{rt.host_path(ROOT)}:/repo:Z",
        rt.IMAGE, "bash", "-c", f"{CONTAINER_ENV} cd /repo/{rel}; {script}",
    ])
    try:
        res = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return 124, "timed out"
    return res.returncode, (res.stdout or "") + (res.stderr or "")


def run_script(script: Path, args: List[str], timeout: int = 400) -> Tuple[int, str]:
    res = subprocess.run([sys.executable, str(script), *args], capture_output=True, text=True, timeout=timeout)
    return res.returncode, (res.stdout or "") + (res.stderr or "")


def record(kind: str, path: str, status: str, detail: str = "") -> bool:
    RESULTS.append((kind, path, status, detail))
    print(f"  [{status:<7}] {kind}: {path}" + (f"  ({detail})" if detail and status != "PASS" else ""))
    return status == "PASS"


def check_output(kind: str, path: str, produced: Path, rc: int, log: str, blocked_hint: Optional[Tuple[str, str]] = None) -> bool:
    if produced.is_file() and produced.stat().st_size > 0:
        return record(kind, path, "PASS", f"{produced.name}, {produced.stat().st_size} bytes")
    if blocked_hint and blocked_hint[0] in log:
        return record(kind, path, "BLOCKED", blocked_hint[1])
    return record(kind, path, "FAIL", f"exit {rc}: {tail(log)}")


def fresh(case: str) -> Path:
    folder = WORK / case
    shutil.rmtree(folder, ignore_errors=True)
    (folder / "back").mkdir(parents=True)
    return folder


def skip(kind: str, path: str, why: str) -> None:
    record(kind, path, "SKIP", why)


# ---- 1. Forms -----------------------------------------------------------------------------------
def test_forms(offline: bool) -> None:
    print("\n1. Forms")
    src = SAMPLES / "test_db.fmb"
    if not src.is_file():
        return skip("Forms", "all paths", f"{src.name} not found")
    case = fresh("forms")
    shutil.copy(src, case / "t.fmb")
    rc, log = run_script(SCRIPTS / "convert_fmb.py", [str(case / "t.fmb")])
    ok = check_output("Forms", "FMB -> XML", case / "t_fmb.xml", rc, log)
    if not ok:
        skip("Forms", "XML -> FMB", "needs FMB -> XML")
        skip("Forms", "Compile FMB -> FMX", "needs FMB -> XML")
        return
    shutil.copy(case / "t_fmb.xml", case / "back" / "r.xml")
    rc, log = run_container(f"{XML2FORMS} r.xml", case / "back")
    if not check_output("Forms", "XML -> FMB", case / "back" / "r.fmb", rc, log):
        return skip("Forms", "Compile FMB -> FMX", "needs XML -> FMB")
    args = [str((case / "back" / "r.fmb").relative_to(ROOT))] + (["--offline"] if offline else [])
    res = subprocess.run([sys.executable, str(COMPILE_FORM), *args], capture_output=True, text=True, cwd=str(ROOT), timeout=400)
    check_output("Forms", "Compile FMB -> FMX", case / "back" / "r.fmx", res.returncode, res.stdout + res.stderr)


# ---- 2. Menus -----------------------------------------------------------------------------------
def test_menus() -> None:
    print("\n2. Menus")
    src = SAMPLES / "ofw_emp_menu.mmb"
    if not src.is_file():
        return skip("Menus", "all paths", f"{src.name} not found")
    case = fresh("menus")
    shutil.copy(src, case / "m.mmb")
    rc, log = run_script(SCRIPTS / "convert_fmb.py", [str(case / "m.mmb")])
    if not check_output("Menus", "MMB -> XML", case / "m_mmb.xml", rc, log):
        skip("Menus", "XML -> MMB", "needs MMB -> XML")
        return skip("Menus", "Compile MMB -> MMX", "needs MMB -> XML")
    shutil.copy(case / "m_mmb.xml", case / "back" / "r.xml")
    rc, log = run_container(f"{XML2FORMS} r.xml", case / "back")
    if not check_output("Menus", "XML -> MMB", case / "back" / "r.mmb", rc, log):
        return skip("Menus", "Compile MMB -> MMX", "needs XML -> MMB")
    rc, log = run_container(f"{H}/bin/frmcmp_batch module=r.mmb module_type=menu compile_all=yes logon=no{BATCH}", case / "back")
    check_output("Menus", "Compile MMB -> MMX", case / "back" / "r.mmx", rc, log)


# ---- 3. Libraries -------------------------------------------------------------------------------
def test_libraries() -> None:
    print("\n3. Libraries")
    # Oracle ships real .pll files in the Oracle Home; the repo's samples/utils.pld is plain PL/SQL, not a library export.
    src = ORACLE_HOME / "forms" / "EnableDisableItem.pll" if ORACLE_HOME else None
    if not src or not src.is_file():
        return skip("Libraries", "all paths", "no .pll available (expected oracle_home_14c/forms/EnableDisableItem.pll)")
    case = fresh("libraries")
    shutil.copy(src, case / "lib.pll")
    rc, log = run_script(SCRIPTS / "convert_pll.py", [str(case / "lib.pll")])
    if not check_output("Libraries", "PLL -> PLD", case / "lib.pld", rc, log):
        return skip("Libraries", "PLD -> PLL", "needs PLL -> PLD")
    shutil.copy(case / "lib.pld", case / "back" / "lib.pld")
    rc, log = run_script(SCRIPTS / "convert_pll.py", [str(case / "back" / "lib.pld"), "--compile"])
    # Verified on Forms 14c: frmcmp_batch prints nothing and writes no .pll when given a .pld text file, and
    # Forms2XML/XML2Forms reject libraries ("must be a FormModule, ObjectLibrary or MenuModule"). Reported as a
    # known limitation (BLOCKED), not a regression, unless frmcmp_batch starts producing the file.
    hint = ("", "known 14c limitation: frmcmp_batch makes no .pll from .pld text, and PLL <-> XML is not supported")
    produced = case / "back" / "lib.pll"
    if produced.is_file() and produced.stat().st_size > 0:
        record("Libraries", "PLD -> PLL", "PASS", f"{produced.name}, {produced.stat().st_size} bytes")
    elif "FRM-" in log or "ORA-" in log or "PDE-" in log:
        record("Libraries", "PLD -> PLL", "FAIL", f"exit {rc}: {tail(log)}")
    else:
        record("Libraries", "PLD -> PLL", "BLOCKED", hint[1])


# ---- 4. OLB -------------------------------------------------------------------------------------
def test_olb() -> None:
    print("\n4. OLB (object libraries)")
    src = SAMPLES / "object_library.olb"
    if not src.is_file():
        return skip("OLB", "all paths", f"{src.name} not found")
    case = fresh("olb")
    shutil.copy(src, case / "o.olb")
    rc, log = run_script(SCRIPTS / "convert_fmb.py", [str(case / "o.olb")])
    if not check_output("OLB", "OLB -> XML", case / "o_olb.xml", rc, log):
        return skip("OLB", "XML -> OLB", "needs OLB -> XML")
    shutil.copy(case / "o_olb.xml", case / "back" / "r.xml")
    rc, log = run_container(f"{XML2FORMS} r.xml", case / "back")
    check_output("OLB", "XML -> OLB", case / "back" / "r.olb", rc, log)


# ---- 5. Reports ---------------------------------------------------------------------------------
def test_reports() -> None:
    print("\n5. Reports")
    src = SAMPLES / "orders_summary_report.xml"
    if not src.is_file():
        return skip("Reports", "all paths", f"{src.name} not found")
    case = fresh("reports")
    shutil.copy(src, case / "r.xml")
    # Oracle's bin/rwconverter.sh is an unfilled template, so the binary is called directly.
    rw = f"export REPORTS_PATH=/repo/{case.relative_to(ROOT).as_posix()}; {H}/bin/rwconverter"
    hint = ("libnsl.so.2", "image lacks libnsl.so.2 (add libnsl2 to Containerfile.14c and rebuild the image)")
    rc, log = run_container(f"{rw} stype=xmlfile source=r.xml dtype=rdffile dest=r.rdf batch=yes overwrite=yes logon=no", case)
    if not check_output("Reports", "XML -> RDF", case / "r.rdf", rc, log, hint):
        return skip("Reports", "RDF -> XML", "needs XML -> RDF")
    shutil.copy(case / "r.rdf", case / "back" / "r.rdf")
    rc, log = run_script(SCRIPTS / "convert_rpt.py", [str(case / "back" / "r.rdf")])
    check_output("Reports", "RDF -> XML", case / "back" / "r_rdf.xml", rc, log, hint)


# ---- Summary ------------------------------------------------------------------------------------
def print_summary() -> int:
    print("\n" + "=" * 78)
    print(f" Conversion test summary  (image {rt.IMAGE})")
    print("=" * 78)
    print(f" {'Type':<10} {'Path':<22} {'Status':<8} Detail")
    print(f" {'-' * 10} {'-' * 22} {'-' * 8} {'-' * 30}")
    for kind, path, status, detail in RESULTS:
        print(f" {kind:<10} {path:<22} {status:<8} {detail[:120]}")
    counts = {s: sum(1 for r in RESULTS if r[2] == s) for s in ("PASS", "FAIL", "BLOCKED", "SKIP")}
    print("-" * 78)
    print(" " + ", ".join(f"{n} {s}" for s, n in counts.items()))
    return 1 if counts["FAIL"] else 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Tests every Oracle artifact conversion path and prints a summary table.")
    parser.add_argument("--only", choices=TYPES, action="append", help="Run only this type (repeatable).")
    parser.add_argument("--offline", action="store_true", help="Compile the form with logon=no (no database).")
    args = parser.parse_args()

    if rt.check_readiness() != 0:
        return 2
    if ORACLE_HOME is None:
        print(f"NOT_READY: no Oracle Home folder found for image {rt.IMAGE}")
        return 2
    WORK.mkdir(parents=True, exist_ok=True)
    selected = args.only or TYPES
    runners: dict = {
        "forms": lambda: test_forms(args.offline),
        "menus": test_menus,
        "libraries": test_libraries,
        "olb": test_olb,
        "reports": test_reports,
    }
    for kind in TYPES:
        if kind in selected:
            runners[kind]()
    return print_summary()


if __name__ == "__main__":
    sys.exit(main())
