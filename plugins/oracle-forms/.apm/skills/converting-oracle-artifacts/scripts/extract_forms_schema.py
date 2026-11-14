#!/usr/bin/env python3
"""
extract_forms_schema.py (CLI)
=====================================

Purpose:
    Extracts the Forms XML Schema (XSD) from the Oracle Forms installation in the container, then compares it with
    the schema committed in this repo. It answers "is our forms-<version>.xsd really Oracle's schema for this
    install?" and refreshes the copies when you ask. The schema is what validate_forms_xml.py (in the
    compiling-and-running-oracle-forms skill) validates hand-written form XML against.

Layer: Tooling / Skill Script (converting-oracle-artifacts)

Usage Examples:
    python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/extract_forms_schema.py
    python3 .../extract_forms_schema.py --version 14c            # name used for the output/compare file (default 14c)
    python3 .../extract_forms_schema.py --usage                  # print the generator's own usage text and exit
    python3 .../extract_forms_schema.py --update                 # replace the committed copies with the extracted schema
    python3 .../extract_forms_schema.py --generator-args "-o forms.xsd"   # extra arguments for the generator

How it works:
    Oracle 14c has no getformsxmlschema.sh. Its schema generator is the Java class
    oracle.forms.util.xmltools.SchemaGenerator in jlib/frmxmltools.jar (the template forms/templates/scripts/
    frmxmlsg.sh just wraps it). This script runs that class in a throwaway forms-converter-14c container, using the
    same classpath as Forms2XML. It accepts the schema either printed to stdout or written as a .xsd file in the
    working folder (temp/forms-schema/<version>/).

Compare / update:
    Compares the SHA-256 of the extracted schema with assets/forms-<version>.xsd in this skill. IDENTICAL or DIFFERENT
    is reported. --update copies the extracted file to: this skill's assets/, the compiling-and-running-oracle-forms
    skill's assets/. There is no copy in docs/. Nothing is changed without --update.

Exit codes: 0 = extracted (and identical, or updated), 1 = extracted but different from the committed copy,
    2 = could not run or no schema was produced.

STATUS: written from the Oracle template script and Java class names; the generator's arguments are NOT VERIFIED
    (the container was not running when this was written). Run it once, and run --usage if no schema is produced.

Script Dependencies:
    - Python 3.8+ standard library only; converter_runtime.py (same folder); Podman running; the 14c Oracle Home.
"""

import argparse
import hashlib
import shutil
import subprocess
import sys
from pathlib import Path
from typing import List, Optional

import converter_runtime as rt

SKILL = Path(__file__).resolve().parents[1]
REPO = rt.repo_root()
COMPILING_ASSETS = SKILL.parent / "compiling-and-running-oracle-forms" / "assets"
WORK_ROOT = REPO / "temp" / "forms-schema"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_generator(work: Path, extra_args: str, usage: bool, oracle_home: Path) -> subprocess.CompletedProcess:
    """Run SchemaGenerator in a throwaway container with the repo at /work; return the finished process."""
    home = rt.HOME
    classpath = f"{home}/jlib/frmjdapi.jar:{home}/jlib/frmxmltools.jar:{home}/oracle_common/modules/oracle.xdk/xmlparserv2.jar"
    rel = work.relative_to(REPO).as_posix()
    args = "-help" if usage else extra_args
    script = (
        f"export ORACLE_HOME={home} FORMS_API_TK_BYPASS=TRUE ORACLE_TERM=vt220 TERM=vt220; "
        f"export LD_LIBRARY_PATH={home}/lib:{home}/oracle_common/lib; "
        f"cd /work/{rel}; "
        f"java -Djava.library.path={home}/lib -classpath {classpath} oracle.forms.util.xmltools.SchemaGenerator {args}"
    )
    cmd = rt.podman_cmd([
        "run", "--rm", "--platform", "linux/amd64",
        "-v", f"{rt.host_path(oracle_home)}:{home}:Z", "-v", f"{rt.host_path(REPO)}:/work:Z",
        rt.IMAGE, "bash", "-c", script,
    ])
    return subprocess.run(cmd, capture_output=True, text=True, timeout=300)


def find_schema(work: Path, stdout: str) -> Optional[Path]:
    """The schema produced: a .xsd file in the work folder, or stdout when it looks like a schema."""
    produced = sorted(work.glob("*.xsd"), key=lambda p: p.stat().st_size, reverse=True)
    if produced:
        return produced[0]
    start = stdout.find("<?xml")
    if start < 0:
        start = stdout.find("<schema")
    if start >= 0 and "<schema" in stdout[start:]:
        target = work / "stdout-schema.xsd"
        target.write_text(stdout[start:], encoding="utf-8")
        return target
    return None


def parse_cli_args(argv: List[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Extract the Forms XML Schema from the Oracle install and compare it with the committed copy")
    parser.add_argument("--version", default="14c", help="schema version name used for the output and the compare file (default: 14c)")
    parser.add_argument("--oracle-home", help="host folder with the Oracle Home (default: oracle-reference-stack/oracle_home_14c)")
    parser.add_argument("--generator-args", default="", help="extra arguments passed to SchemaGenerator")
    parser.add_argument("--usage", action="store_true", help="print SchemaGenerator's own usage text and exit")
    parser.add_argument("--update", action="store_true", help="replace the committed copies with the extracted schema")
    return parser.parse_args(argv)


def main() -> int:
    args = parse_cli_args(sys.argv[1:])
    if rt.check_readiness() != 0:
        return 2
    oracle_home = Path(args.oracle_home) if args.oracle_home else rt.default_oracle_home()
    if oracle_home is None or not oracle_home.is_dir():
        print(f"ERROR: Oracle Home folder not found for image {rt.IMAGE}")
        return 2
    work = WORK_ROOT / args.version
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)

    result = run_generator(work, args.generator_args, args.usage, oracle_home)
    if args.usage:
        print((result.stdout or "") + (result.stderr or ""))
        return 0
    schema = find_schema(work, result.stdout or "")
    if schema is None:
        print("ERROR: SchemaGenerator produced no schema. Its output:")
        print(((result.stdout or "") + (result.stderr or "")).strip()[-1500:])
        print("Run with --usage to see the tool's arguments, then pass them with --generator-args.")
        return 2
    print(f"Extracted: {schema} ({schema.stat().st_size} bytes, sha256 {sha256(schema)[:16]}...)")

    committed = SKILL / "assets" / f"forms-{args.version}.xsd"
    if committed.is_file():
        same = sha256(schema) == sha256(committed)
        print(f"Committed:  {committed} ({committed.stat().st_size} bytes)")
        print("Result: IDENTICAL to the committed schema." if same else "Result: DIFFERENT from the committed schema.")
    else:
        same = False
        print(f"No committed schema for version {args.version} yet ({committed}).")

    if args.update:
        targets = [committed, COMPILING_ASSETS / f"forms-{args.version}.xsd"]
        for target in targets:
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(schema, target)
            print(f"Updated: {target}")
        return 0
    return 0 if same else 1


if __name__ == "__main__":
    sys.exit(main())
