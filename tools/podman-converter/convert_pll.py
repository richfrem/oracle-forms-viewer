#!/usr/bin/env python3
"""
convert_pll.py (CLI)
=====================================

Purpose:
    Cross-platform command-line tool to convert Oracle PL/SQL Library binary files (.pll)
    into human-readable ASCII text scripts (.pld) or compile text scripts into binary libraries (.pll/.plx)
    via an on-demand Podman container running the native Oracle Forms Batch Compiler (frmcmp_batch).

Layer: Tooling / Container Bridge

Usage Examples:
    python3 tools/podman-converter/convert_pll.py --check
    python3 tools/podman-converter/convert_pll.py samples/aglib.pll
    python3 tools/podman-converter/convert_pll.py samples/aglib.pll -o ./custom/aglib.pld
    python3 tools/podman-converter/convert_pll.py samples/aglib.pld --compile
    python3 tools/podman-converter/convert_pll.py samples/aglib.pll --oracle-home /custom/oracle_home

Supported Object Types:
    - .pll (Oracle PL/SQL Binary Library) -> *.pld plain text script export
    - .pld (Oracle PL/SQL Text Script)    -> *.pll compiled binary library import

CLI Arguments:
    pll_path              : Path to target Oracle PL/SQL library file (.pll or .pld).
    -o, --output <path>   : Destination path for converted output file (optional).
    --compile             : Compiles .pld text script back into binary .pll, or compiles .pll to .plx.
    --check               : Probes host container engine readiness and exits.
    --oracle-home <path>  : Path to local Oracle Home directory (optional).

Input Files:
    - tools/podman-converter/Containerfile (Container image definition)
    - tools/podman-converter/oracle_home/bin/frmcmp_batch (Oracle Forms batch compiler)
    - Target library files (.pll or .pld)

Output:
    - <target_dir>/<module_name>.pld (Standard text export of all package specs, bodies, and procedures)
    - OR custom destination path if specified via -o / --output

Key Functions:
    - check_podman_readiness() -> int
    - resolve_oracle_home(custom_oracle_home: Optional[str]) -> Optional[Path]
    - build_container_script(file_name: str, stem: str, is_compile: bool) -> str
    - copy_to_custom_output(source_file: Path, output_path_str: Optional[str]) -> None
    - convert_library(input_path_str: str, output_path_str: Optional[str], is_compile: bool, custom_oracle_home: Optional[str]) -> int
    - parse_cli_args(argv: List[str]) -> argparse.Namespace
    - main() -> int

Script Dependencies:
    - Host Runtime:
        * Python 3.8+ (sys, shutil, platform, subprocess, pathlib, argparse, typing)
        * Podman CLI (or Docker) installed and active on host system
        * Rosetta 2 binary translation active on macOS Apple Silicon
    - Container Image ('forms-converter:latest'):
        * Base OS: Oracle Linux 8 Slim (linux/amd64)
        * Required environment variables: ORACLE_HOME, ORACLE_TERM=vt220, LD_LIBRARY_PATH

Consumed by:
    - Developers extracting PL/SQL packages and functions from legacy .pll libraries for git versioning
    - Rust native viewer ingestion pipeline for .pld AST parsing
"""

import sys
import shutil
import argparse
import platform
import subprocess
from pathlib import Path
from typing import Optional, List


# -----------------------------------------------------------------------------
# System Readiness Probe
# -----------------------------------------------------------------------------
# Validates container engine status and virtualization availability on host
def check_podman_readiness() -> int:
    """Verifies that the Podman CLI is installed, the VM is running, and the image exists.

    Returns:
        int: 0 if the container environment is ready to execute conversions, 1 otherwise.
    """
    if not shutil.which("podman"):
        print("NOT_READY: 'podman' command not found in system PATH")
        return 1

    if platform.system().lower() == "darwin":
        try:
            res = subprocess.run(
                ["podman", "machine", "list"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if "running" not in res.stdout.lower():
                print("NOT_READY: Podman machine is stopped. Run 'podman machine start'")
                return 1
        except Exception as err:
            print(f"NOT_READY: Failed to inspect Podman machine state: {err}")
            return 1

    # Verify the converter container image is available locally
    try:
        res = subprocess.run(
            ["podman", "image", "exists", "forms-converter:latest"],
            capture_output=True,
            timeout=5,
        )
        if res.returncode != 0:
            print("NOT_READY: Image 'forms-converter:latest' not built. Run Containerfile build.")
            return 1
    except Exception as err:
        print(f"NOT_READY: Podman image inspection failed: {err}")
        return 1

    print("READY: Podman engine and 'forms-converter:latest' image are operational.")
    return 0


# -----------------------------------------------------------------------------
# Oracle Home Directory Resolver
# -----------------------------------------------------------------------------
# Resolves host directory containing Oracle Forms 12c installation files
def resolve_oracle_home(custom_oracle_home: Optional[str]) -> Optional[Path]:
    """Locates the Oracle Home directory containing Oracle Forms runtime binaries.

    Args:
        custom_oracle_home: Optional user-specified path to Oracle Home.

    Returns:
        Optional[Path]: Resolved Path if the directory exists, None otherwise.
    """
    if custom_oracle_home:
        custom_path = Path(custom_oracle_home).resolve()
        if custom_path.is_dir():
            return custom_path
        print(f"Warning: Specified --oracle-home '{custom_oracle_home}' is not a valid directory.")

    default_home = Path(__file__).resolve().parent / "oracle_home"
    if default_home.is_dir():
        return default_home

    return None


# -----------------------------------------------------------------------------
# Container Bash Command Builder
# -----------------------------------------------------------------------------
# Generates the shell execution script to run frmcmp_batch inside container
def build_container_script(file_name: str, stem: str, is_compile: bool) -> str:
    """Generates the headless bash script string to be executed inside the container.

    Args:
        file_name: Base file name including extension (e.g., 'aglib.pll', 'aglib.pld').
        stem: File name without extension (e.g., 'aglib').
        is_compile: True if compiling script into library, False if exporting to text.

    Returns:
        str: Bash command sequence setting up environment and running frmcmp_batch.
    """
    if is_compile:
        # Compiles .pld text script into binary .pll or compiles .pll to executable .plx
        return (
            f"export ORACLE_HOME=/oracle_home; "
            f"export ORACLE_TERM=vt220; "
            f"export LD_LIBRARY_PATH=/oracle_home/lib:$LD_LIBRARY_PATH; "
            f"export FORMS_PATH=/work:/work/shared:$FORMS_PATH; "
            f"cd /work; "
            f"echo 'Executing Oracle frmcmp_batch to compile library {file_name}...'; "
            f"/oracle_home/bin/frmcmp_batch Module={file_name} Module_Type=LIBRARY Script=NO compile_all=YES Logon=NO; "
        )

    # Exports binary .pll into plain-text .pld script
    return (
        f"export ORACLE_HOME=/oracle_home; "
        f"export ORACLE_TERM=vt220; "
        f"export LD_LIBRARY_PATH=/oracle_home/lib:$LD_LIBRARY_PATH; "
        f"export FORMS_PATH=/work:/work/shared:$FORMS_PATH; "
        f"cd /work; "
        f"echo 'Executing Oracle native frmcmp_batch utility to export PL/SQL library to .pld...'; "
        f"/oracle_home/bin/frmcmp_batch Module={file_name} Module_Type=LIBRARY Script=YES Logon=NO; "
    )


# -----------------------------------------------------------------------------
# Custom Output File Copier
# -----------------------------------------------------------------------------
# Copies generated .pld or .pll to user-specified destination path
def copy_to_custom_output(source_file: Path, output_path_str: Optional[str]) -> None:
    """Copies generated output file to a user-specified destination path if provided.

    Args:
        source_file: Path to the generated sidecar file.
        output_path_str: Optional user-specified output path or directory.
    """
    if not output_path_str or not source_file.is_file():
        return

    dest = Path(output_path_str).resolve()
    if dest.is_dir() or output_path_str.endswith(("/", "\\")):
        dest.mkdir(parents=True, exist_ok=True)
        dest = dest / source_file.name

    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source_file, dest)
    print(f"SUCCESS: Copied output file to: {dest}")


# -----------------------------------------------------------------------------
# Library Conversion Execution
# -----------------------------------------------------------------------------
# Mounts host directory and dispatches frmcmp_batch inside container
def convert_library(
    input_path_str: str,
    output_path_str: Optional[str] = None,
    is_compile: bool = False,
    custom_oracle_home: Optional[str] = None,
) -> int:
    """Mounts host directory into container and executes library export or compilation.

    Args:
        input_path_str: String path to the target Oracle PL/SQL library file (.pll or .pld).
        output_path_str: Optional destination path for converted file.
        is_compile: True to compile into binary library, False to export to text script.
        custom_oracle_home: Optional path to external Oracle Home runtime folder.

    Returns:
        int: Process exit code (0 for success, 1 for fatal error, 2 for missing runtime).
    """
    input_path = Path(input_path_str).resolve()
    if not input_path.is_file():
        print(f"Error: Input file '{input_path}' does not exist.")
        return 1

    target_dir = input_path.parent
    file_name = input_path.name
    file_ext = input_path.suffix.lower()
    oracle_home_path = resolve_oracle_home(custom_oracle_home)

    extra_mount: List[str] = []
    if oracle_home_path:
        extra_mount = ["-v", f"{oracle_home_path.as_posix()}:/oracle_home:Z"]

    print("=================================================================")
    print(" Oracle PL/SQL Library Containerized Converter (frmcmp_batch)")
    print("=================================================================")
    print(f" OS:             {platform.system()} ({platform.machine()})")
    print(f" File:           {file_name}")
    print(f" Host Directory: {target_dir}")
    print(f" Mode:           {'Compile (.pld -> .pll)' if is_compile else 'Export (.pll -> .pld)'}")
    if output_path_str:
        print(f" Target Output:  {output_path_str}")
    if extra_mount:
        print(f" Oracle Home:    {oracle_home_path} (mounted to /oracle_home)")
    else:
        print(" Oracle Home:    Default internal container path (/oracle_home)")
    print("=================================================================")

    if is_compile:
        expected_output = target_dir / f"{input_path.stem}.pll"
    else:
        expected_output = target_dir / f"{input_path.stem}.pld"

    container_cmd = build_container_script(file_name, input_path.stem, is_compile)
    cmd = [
        "podman",
        "run",
        "--rm",
        "--platform",
        "linux/amd64",
        "-v",
        f"{target_dir.as_posix()}:/work:Z",
    ] + extra_mount + [
        "forms-converter:latest",
        "bash",
        "-c",
        container_cmd,
    ]

    try:
        res = subprocess.run(cmd)
        if res.returncode == 0:
            if expected_output.is_file():
                print(f"SUCCESS: Output file available at {expected_output}")
                copy_to_custom_output(expected_output, output_path_str)
        return res.returncode
    except Exception as err:
        print(f"Execution failed: {err}")
        return 1


# -----------------------------------------------------------------------------
# CLI Argument Parser
# -----------------------------------------------------------------------------
# Sets up and parses command-line flags and parameters
def parse_cli_args(argv: List[str]) -> argparse.Namespace:
    """Configures and parses command-line arguments using standard argparse.

    Args:
        argv: Command line argument list.

    Returns:
        argparse.Namespace: Parsed options and flags.
    """
    parser = argparse.ArgumentParser(
        description="Cross-platform Oracle PL/SQL Library (.pll/.pld) containerized converter.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "pll_path",
        nargs="?",
        help="Path to the target Oracle PL/SQL library file (.pll or .pld).",
    )
    parser.add_argument(
        "-o",
        "--output",
        dest="output_path",
        help="Destination path or directory for generated output file (.pld or .pll).",
    )
    parser.add_argument(
        "--compile",
        action="store_true",
        help="Compiles text script .pld into binary library .pll, or compiles .pll to .plx.",
    )
    parser.add_argument(
        "--oracle-home",
        dest="oracle_home",
        help="Path to host directory containing Oracle Home runtime.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Probes host container engine readiness and exits.",
    )
    return parser.parse_args(argv)


# -----------------------------------------------------------------------------
# Main Entry Point
# -----------------------------------------------------------------------------
# Dispatches execution based on parsed command line options
def main() -> int:
    """Parses command-line arguments and dispatches probe or library conversion actions.

    Returns:
        int: Standard system exit code.
    """
    args = parse_cli_args(sys.argv[1:])

    if args.check:
        return check_podman_readiness()

    if not args.pll_path:
        print("Usage: convert_pll.py <file.pll|.pld> [-o <output>] [--compile] [--oracle-home <path>]")
        print("       convert_pll.py --check")
        return 1

    return convert_library(
        input_path_str=args.pll_path,
        output_path_str=args.output_path,
        is_compile=args.compile,
        custom_oracle_home=args.oracle_home,
    )


if __name__ == "__main__":
    sys.exit(main())
