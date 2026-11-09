#!/usr/bin/env python3
"""
launch_fsal.py (CLI)
=====================================

Purpose:
    Cross-platform desktop runner for Oracle Forms applications using
    Forms Standalone Launcher (FSAL). Automatically handles URL generation,
    default port mapping, password encoding, and runtime launching without
    requiring browser applets or X11 forwarding.

Layer: Tooling / Skill Script (compiling-and-running-oracle-forms)

Usage Examples:
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/launch_fsal.py test_db.fmx
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/launch_fsal.py simple_module.fmx --interactive
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/launch_fsal.py test_db.fmx --port 7001

Supported Object Types:
    - .fmx (Compiled binary executable form module)

CLI Arguments:
    form                  : Name or path of the form to run (e.g., test_db.fmx).
    -p, --port <int>      : WebLogic AdminServer port (default: 7001).
    -u, --userid <str>    : Database logon string (default: forms_user/Welcome123#@FREEPDB1).
    --interactive         : Omit credentials and show the native GUI Logon dialog.
    --frmsal-jar <path>   : Custom path to frmsal.jar (optional).

Input Files:
    - tools/podman-converter/oracle_home_14c/forms/java/frmsal.jar (Official FSAL client)
    - WebLogic Forms Server active on http://localhost:<port>/forms/frmservlet

Output:
    - Native desktop application window running Oracle Forms on macOS or Windows

Key Functions:
    - resolve_workspace_root() -> Path
    - build_fsal_url(form_name: str, port: int, userid: str, interactive: bool) -> str
    - launch_fsal_process(jar_path: Path, url: str) -> int
    - parse_cli_args(argv: List[str]) -> argparse.Namespace
    - main() -> int

Script Dependencies:
    - Python 3.8+ (standard library: sys, os, argparse, subprocess, urllib.parse, pathlib, typing)
    - Java 17+ or Java 25 JRE installed on host OS

Consumed by:
    - run_form.py (final desktop launch step)
    - Developers running standalone Forms sessions on desktop
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


# Constructs the complete Forms Listener Servlet URL with safe parameter encoding.
def build_fsal_url(form_name: str, port: int, userid: str, interactive: bool) -> str:
    """Construct the HTTP URL for the standaloneapp configuration.

    Args:
        form_name: File name of the .fmx module.
        port: Listening port for WebLogic Forms server.
        userid: Database logon credential string.
        interactive: If True, omit credentials so Forms displays the GUI logon box.

    Returns:
        Formatted and URL-encoded query string.
    """
    base_url = f"http://localhost:{port}/forms/frmservlet?config=standaloneapp&form={form_name}"
    if not interactive and userid and userid.lower() != "none":
        # Password '#' must be URL encoded as '%23'
        encoded_userid = urllib.parse.quote(userid, safe="@/:")
        return f"{base_url}&userid={encoded_userid}"
    return base_url


# Dispatches the Java process running frmsal.jar against the specified URL.
def launch_fsal_process(jar_path: Path, url: str) -> int:
    """Spawn the native FSAL desktop Java process.

    Args:
        jar_path: Path to frmsal.jar.
        url: Complete Forms Servlet connection URL.

    Returns:
        Exit code from the Java process.
    """
    cmd: List[str] = ["java", "-jar", str(jar_path), "-url", url]
    print(f"Launching Forms Standalone Launcher...")
    print(f"  JAR: {jar_path.name}")
    print(f"  URL: {url.split('&userid=')[0]}")
    try:
        return subprocess.call(cmd)
    except KeyboardInterrupt:
        print("\nFSAL window closed.")
        return 0


# Parses CLI arguments for launch_fsal.
def parse_cli_args(argv: List[str]) -> argparse.Namespace:
    """Parse command-line flags and positional parameters.

    Args:
        argv: Command-line arguments.

    Returns:
        Parsed arguments namespace.
    """
    parser = argparse.ArgumentParser(description="Oracle Forms Standalone Launcher (FSAL) Runner")
    parser.add_argument("form", help="Name or path of the form to run (e.g., test_db.fmx or simple_module.fmx)")
    parser.add_argument("-p", "--port", default=7001, type=int, help="WebLogic AdminServer port (default: 7001)")
    parser.add_argument("-u", "--userid", default="forms_user/Welcome123#@FREEPDB1", help="Database logon string")
    parser.add_argument("--interactive", action="store_true", help="Omit credentials and show native GUI Logon dialog")
    parser.add_argument("--frmsal-jar", default=None, help="Custom path to frmsal.jar (optional)")
    return parser.parse_args(argv)


# Main execution entry point for launch_fsal.py.
def main() -> int:
    """Validate environment, resolve launcher JAR, and run FSAL desktop client.

    Returns:
        0 on success, 1 on error.
    """
    args = parse_cli_args(sys.argv[1:])
    form_name = Path(args.form).name
    if not form_name.endswith(".fmx"):
        form_name += ".fmx"

    repo_root = resolve_workspace_root()
    if args.frmsal_jar:
        frmsal_path = Path(args.frmsal_jar).resolve()
    else:
        frmsal_path = repo_root / "tools/podman-converter/oracle_home_14c/forms/java/frmsal.jar"

    if not frmsal_path.exists():
        print(f"Error: frmsal.jar not found at {frmsal_path}", file=sys.stderr)
        return 1

    url = build_fsal_url(form_name, args.port, args.userid, args.interactive)
    return launch_fsal_process(frmsal_path, url)


if __name__ == "__main__":
    sys.exit(main())
