#!/usr/bin/env python3
"""
run_form.py (CLI)
=====================================

Purpose:
    Cross-platform launcher to build, stage, and execute any Oracle Forms module
    (.fmx, .fmb, or .xml) live in Oracle Fusion Middleware 14c WebLogic Server
    via native desktop Forms Standalone Launcher (FSAL) without legacy browser
    plugins or X11/XQuartz display forwarding.

Layer: Tooling / Skill Script (compiling-and-running-oracle-forms)

Usage Examples:
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/test_db.fmx
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/test_db.fmb
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/test_db_fmb.xml
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/test_db.fmx --interactive

Supported Object Types:
    - .fmx (Compiled binary executable form - directly launched via FSAL)
    - .fmb (Binary source form - compiled via frmcmp_batch, then launched)
    - .xml (Forms XML definition - converted via XML2Forms, compiled, then launched)

CLI Arguments:
    form_path             : Path to input .fmx, .fmb, or .xml file.
    -p, --port <int>      : WebLogic AdminServer port (default: 7001).
    -u, --userid <str>    : Database logon connection string (default: forms_user/Welcome123#@FREEPDB1).
    --interactive         : Omit credentials and prompt via native GUI Logon dialog.
    --offline             : Compile offline without database verification (logon=no).

Input Files:
    - tools/podman-converter/oracle_home_14c/forms/java/frmsal.jar (FSAL desktop launcher)
    - Target source form (.fmx, .fmb, or .xml)
    - Docker / Podman running container 'forms-server-14c' (WebLogic Forms 14c runtime)
    - Docker / Podman running container 'oracle23ai-lite' (Oracle Database 23ai Free)

Output:
    - Compiled binary file (.fmx) in target directory and container /oracle_home_14c/forms/
    - Live interactive desktop Forms application window on macOS or Windows

Key Functions:
    - resolve_workspace_root() -> Path
    - convert_xml_to_fmb(xml_path: Path, repo_root: Path) -> Path
    - compile_fmb_to_fmx(fmb_path: Path, repo_root: Path, userid: str, offline: bool) -> Path
    - launch_fsal_desktop(fmx_path: Path, repo_root: Path, port: int, userid: str, interactive: bool) -> int
    - parse_cli_args(argv: List[str]) -> argparse.Namespace
    - main() -> int

Script Dependencies:
    - Python 3.8+ (standard library: sys, os, argparse, subprocess, urllib.parse, pathlib, typing)
    - Java 17+ or Java 25 installed on host machine
    - Podman running containers 'forms-server-14c' and 'oracle23ai-lite'

Consumed by:
    - AI coding agents executing end-to-end form verifications
    - Developers testing custom forms and database round-trips
"""

import sys
import os
import argparse
import subprocess
import urllib.parse
from pathlib import Path
from typing import List, Optional


# Resolves the absolute path to the repository root directory.
def resolve_workspace_root() -> Path:
    """Locate the top-level repository root directory.

    Returns:
        Path pointing to the repository root.
    """
    return Path(__file__).resolve().parents[6]


# Converts a Forms XML definition file into a binary .fmb via XML2Forms inside the container.
def convert_xml_to_fmb(xml_path: Path, repo_root: Path) -> Path:
    """Invoke official Oracle XML2Forms tool in the container to generate a binary FMB.

    Args:
        xml_path: Absolute or relative path to the source Forms XML file.
        repo_root: Path to the repository root directory.

    Returns:
        Path to the generated .fmb file.

    Raises:
        RuntimeError: If XML2Forms conversion fails.
    """
    rel_path = xml_path.resolve().relative_to(repo_root)
    container_xml = f"/work/{rel_path}"
    output_fmb = xml_path.resolve().with_suffix(".fmb")
    rel_fmb = output_fmb.relative_to(repo_root)
    container_fmb = f"/work/{rel_fmb}"

    print(f"[1/3] Converting XML definition {xml_path.name} to binary FMB...")
    cmd = (
        f"export ORACLE_HOME=/oracle_home_14c; "
        f"export FORMS_API_TK_BYPASS=TRUE; "
        f"export LD_LIBRARY_PATH=/oracle_home_14c/lib; "
        f"/usr/lib/jvm/jdk-17.0.12/bin/java "
        f"-Djava.library.path=/oracle_home_14c/lib "
        f"-classpath /oracle_home_14c/jlib/frmjdapi.jar:/oracle_home_14c/jlib/frmxmltools.jar:/oracle_home_14c/oracle_common/modules/oracle.xdk/xmlparserv2.jar "
        f"oracle.forms.util.xmltools.XML2Forms "
        f"OVERWRITE=YES "
        f"OUTPUTDIR=/work/{rel_fmb.parent} "
        f"{container_xml}"
    )

    res = subprocess.run(["podman", "exec", "forms-server-14c", "/bin/sh", "-c", cmd],
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    if "Module saved as" not in res.stdout and res.returncode != 0:
        raise RuntimeError(f"XML2Forms failed:\n{res.stdout}")

    print(f"  - Generated: {output_fmb.name}")
    return output_fmb


# Compiles an .fmb module into an executable .fmx via frmcmp_batch inside the container.
def compile_fmb_to_fmx(fmb_path: Path, repo_root: Path, userid: str, offline: bool) -> Path:
    """Compile a binary .fmb module into an executable .fmx using container frmcmp_batch.

    Args:
        fmb_path: Path to the source .fmb file.
        repo_root: Path to the repository root.
        userid: Database connection string.
        offline: If True, compile without database logon (logon=no).

    Returns:
        Path to the compiled .fmx file.

    Raises:
        RuntimeError: If compilation fails.
    """
    rel_path = fmb_path.resolve().relative_to(repo_root)
    container_fmb = f"/work/{rel_path}"
    fmx_path = fmb_path.resolve().with_suffix(".fmx")
    logon_param = "logon=no" if offline else f"userid='{userid}'"

    print(f"[2/3] Compiling {fmb_path.name} to executable FMX (logon={'no' if offline else 'yes'})...")
    cmd = (
        f"export ORACLE_HOME=/oracle_home_14c; "
        f"export LD_LIBRARY_PATH=/oracle_home_14c/lib; "
        f"export ORACLE_TERM=vt220; "
        f"export TERM=vt220; "
        f"export FORMS_PATH=/oracle_home_14c/forms:/work/samples:{container_fmb.rsplit('/', 1)[0]}; "
        f"export TNS_ADMIN=/oracle_home_14c/user_projects/domains/base_domain/config/fmwconfig; "
        f"/oracle_home_14c/bin/frmcmp_batch "
        f"module={container_fmb} "
        f"module_type=form "
        f"{logon_param} "
        f"compile_all=yes"
    )

    res = subprocess.run(["podman", "exec", "-u", "oracle", "forms-server-14c", "/bin/sh", "-c", cmd],
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    if "Created form file" not in res.stdout and res.returncode != 0:
        raise RuntimeError(f"Compilation failed:\n{res.stdout}")

    # Ensure the compiled form is mirrored into container forms deployment directory
    copy_cmd = f"cp {container_fmb.rsplit('.', 1)[0]}.fmx /oracle_home_14c/forms/{fmx_path.name}"
    subprocess.run(["podman", "exec", "forms-server-14c", "/bin/sh", "-c", copy_cmd], check=True)

    print(f"  - Successfully compiled: {fmx_path.name}")
    return fmx_path


# Launches the compiled form in the host desktop Forms Standalone Launcher client.
def launch_fsal_desktop(fmx_path: Path, repo_root: Path, port: int, userid: str, interactive: bool) -> int:
    """Run Forms Standalone Launcher (frmsal.jar) natively on host OS.

    Args:
        fmx_path: Path to the target .fmx file.
        repo_root: Path to the repository root.
        port: WebLogic listening port (default: 7001).
        userid: Database logon string.
        interactive: If True, prompt for logon in GUI.

    Returns:
        Process exit code from frmsal.jar.
    """
    frmsal_jar = repo_root / "tools/podman-converter/oracle_home_14c/forms/java/frmsal.jar"
    if not frmsal_jar.exists():
        print(f"Error: frmsal.jar not found at {frmsal_jar}", file=sys.stderr)
        return 1

    form_name = fmx_path.name
    base_url = f"http://localhost:{port}/forms/frmservlet?config=standaloneapp&form={form_name}"

    if not interactive and userid and userid.lower() != "none":
        encoded_userid = urllib.parse.quote(userid, safe="@/:")
        url = f"{base_url}&userid={encoded_userid}"
    else:
        url = base_url

    print(f"[3/3] Launching live form in Forms Standalone Launcher (FSAL)...")
    print(f"  Target Form: {form_name}")
    print(f"  Server URL:  http://localhost:{port}/forms/frmservlet?config=standaloneapp&form={form_name}")
    print(f"  Credentials: {'Interactive GUI Prompt' if interactive else 'Pre-authenticated via SQL*Net'}")

    cmd = ["java", "-jar", str(frmsal_jar), "-url", url]
    try:
        return subprocess.call(cmd)
    except KeyboardInterrupt:
        print("\nFSAL window closed.")
        return 0


# Parses command line arguments for the run_form utility.
def parse_cli_args(argv: List[str]) -> argparse.Namespace:
    """Parse command line flags and positional arguments.

    Args:
        argv: Command line argument list.

    Returns:
        Parsed arguments namespace.
    """
    parser = argparse.ArgumentParser(
        description="Unified runner for Oracle Forms: converts, compiles, and launches any form in WebLogic FSAL."
    )
    parser.add_argument("form_path", help="Path to input .fmx, .fmb, or .xml module file")
    parser.add_argument("-p", "--port", type=int, default=7001, help="WebLogic AdminServer port (default: 7001)")
    parser.add_argument("-u", "--userid", default="forms_user/Welcome123#@FREEPDB1", help="Database connect string")
    parser.add_argument("--interactive", action="store_true", help="Prompt for credentials via native GUI Logon dialog")
    parser.add_argument("--offline", action="store_true", help="Compile without database connection (logon=no)")
    return parser.parse_args(argv)


# Main execution entry point for run_form.py.
def main() -> int:
    """Execute the end-to-end form execution pipeline.

    Returns:
        0 on success, non-zero on failure.
    """
    args = parse_cli_args(sys.argv[1:])
    repo_root = resolve_workspace_root()
    input_file = Path(args.form_path).resolve()

    if not input_file.exists():
        print(f"Error: Input file does not exist: {input_file}", file=sys.stderr)
        return 1

    suffix = input_file.suffix.lower()
    try:
        if suffix == ".xml":
            fmb_file = convert_xml_to_fmb(input_file, repo_root)
            fmx_file = compile_fmb_to_fmx(fmb_file, repo_root, args.userid, args.offline)
        elif suffix == ".fmb":
            fmx_file = compile_fmb_to_fmx(input_file, repo_root, args.userid, args.offline)
        elif suffix == ".fmx":
            fmx_file = input_file
            # Copy into container forms path to guarantee availability
            rel_fmx = fmx_file.relative_to(repo_root)
            cmd = f"cp /work/{rel_fmx} /oracle_home_14c/forms/{fmx_file.name}"
            subprocess.run(["podman", "exec", "forms-server-14c", "/bin/sh", "-c", cmd], check=True)
        else:
            print(f"Error: Unsupported file format '{suffix}'. Expected .xml, .fmb, or .fmx", file=sys.stderr)
            return 1

        return launch_fsal_desktop(fmx_file, repo_root, args.port, args.userid, args.interactive)
    except Exception as err:
        print(f"\nExecution error: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
