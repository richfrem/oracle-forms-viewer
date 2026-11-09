#!/usr/bin/env python3
"""
convert_rpt.py (CLI)
=====================================

Purpose:
    Cross-platform command-line tool interface for converting Oracle Reports binary files (.rdf, .rpt)
    into structured XML representations (*_rdf.xml) or ASCII text definition scripts (*.rex) via
    an on-demand Podman container running Oracle Reports rwconverter.sh.

    NOTE: The default container installation uses 'Standalone Forms Builder', which intentionally
    excludes Oracle Reports binaries (rwconverter.sh) because Oracle 12c requires a separate full
    WebLogic / FMW Infrastructure installation to deploy Reports.
    RECOMMENDED: Use the pure-native Rust toolchain ('oracle-forms-cli inspect <file.rdf>' and
    'oracle-forms-cli dump-sql') which parses ROS.60050 report files natively with 0 JVM and 0 Oracle runtime!

Layer: Tooling / Container Bridge

Usage Examples:
    python3 tools/podman-converter/convert_rpt.py --check
    python3 tools/podman-converter/convert_rpt.py samples/invoice.rdf
    python3 tools/podman-converter/convert_rpt.py samples/invoice.rdf -o ./custom/invoice.xml
    python3 tools/podman-converter/convert_rpt.py samples/invoice.rdf --format rex
    python3 tools/podman-converter/convert_rpt.py samples/invoice.rdf --oracle-home /custom/oracle_home

Supported Object Types:
    - .rdf (Oracle Reports Binary Definition) -> XML (*_rdf.xml) or ASCII Text (*.rex)
    - .rpt (Legacy Oracle Reports Definition) -> XML (*_rpt.xml) or ASCII Text (*.rex)
    - .rex (Oracle Reports Text Export)       -> Compiled Binary (.rdf)

CLI Arguments:
    rpt_path              : Path to the target Oracle Reports binary file (.rdf, .rpt, .rex).
    -o, --output <path>   : Destination path for converted output file (optional).
    -f, --format <format> : Target output format: 'xml' (default) or 'rex' (ASCII text).
    --check               : Probes host container engine and Reports runtime readiness.
    --oracle-home <path>  : Path to local Oracle Home directory (optional).

Input Files:
    - tools/podman-converter/Containerfile (Container image definition)
    - tools/podman-converter/oracle_home/bin/rwconverter.sh (Oracle Reports converter)
    - Target report binary files (.rdf, .rpt, .rex)

Output:
    - <target_dir>/<report_name>_rdf.xml (Reports XML schema representation)
    - OR <target_dir>/<report_name>.rex (Reports ASCII text representation)
    - OR custom destination path if specified via -o / --output

Key Functions:
    - check_podman_readiness() -> int
    - resolve_oracle_home(custom_oracle_home: Optional[str]) -> Optional[Path]
    - check_reports_runtime(oracle_home_path: Optional[Path]) -> bool
    - build_container_script(file_name: str, stem: str, out_ext: str, dtype: str) -> str
    - copy_to_custom_output(source_file: Path, output_path_str: Optional[str]) -> None
    - convert_report(input_path_str: str, output_path_str: Optional[str], target_format: str, custom_oracle_home: Optional[str]) -> int
    - parse_cli_args(argv: List[str]) -> argparse.Namespace
    - main() -> int

Script Dependencies:
    - Host Runtime:
        * Python 3.8+ (sys, shutil, platform, subprocess, pathlib, argparse, typing)
        * Podman CLI (or Docker) installed and active on host system
        * Rosetta 2 binary translation active on macOS Apple Silicon
    - Container Image ('forms-converter:latest'):
        * Base OS: Oracle Linux 8 Slim (linux/amd64)
        * Oracle Fusion Middleware Reports component (rwconverter.sh)

Consumed by:
    - Developers extracting queries, layout definitions, and data models from legacy Oracle Reports
    - Migration tooling converting Oracle Reports to modern BI / Jasper / SSRS equivalents
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
# Resolves host directory containing Oracle Forms & Reports installation files
def resolve_oracle_home(custom_oracle_home: Optional[str]) -> Optional[Path]:
    """Locates the Oracle Home directory containing Oracle runtime binaries.

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
# Reports Runtime Availability Probe
# -----------------------------------------------------------------------------
# Checks whether rwconverter.sh exists in the resolved Oracle Home
def check_reports_runtime(oracle_home_path: Optional[Path]) -> bool:
    """Verifies if the Oracle Reports batch converter binary (rwconverter.sh) is installed.

    Args:
        oracle_home_path: Resolved path to the Oracle Home directory.

    Returns:
        bool: True if rwconverter.sh is present, False otherwise.
    """
    if not oracle_home_path:
        return False

    rwconverter = oracle_home_path / "bin" / "rwconverter.sh"
    return rwconverter.is_file()


# -----------------------------------------------------------------------------
# Container Bash Command Builder
# -----------------------------------------------------------------------------
# Generates the shell execution script to run rwconverter inside container
def build_container_script(file_name: str, stem: str, out_ext: str, dtype: str) -> str:
    """Generates the headless bash script string to be executed inside the container.

    Args:
        file_name: Base report file name including extension (e.g., 'invoice.rdf').
        stem: File name without extension (e.g., 'invoice').
        out_ext: Target file extension ('_rdf.xml' or '.rex').
        dtype: Oracle Reports destination type ('xmlfile' or 'rexfile').

    Returns:
        str: Bash command sequence setting up environment and running rwconverter.
    """
    dest_name = f"{stem}{out_ext}"
    return (
        f"export ORACLE_HOME=/oracle_home; "
        f"export ORACLE_TERM=vt220; "
        f"export LD_LIBRARY_PATH=/oracle_home/lib:$LD_LIBRARY_PATH; "
        f"export REPORTS_PATH=/work:/work/shared:$REPORTS_PATH; "
        f"cd /work; "
        f"if [ -f /oracle_home/bin/rwconverter.sh ]; then "
        f"  echo 'Executing Oracle native rwconverter.sh utility...'; "
        f"  chmod +x /oracle_home/bin/rwconverter.sh 2>/dev/null || true; "
        f"  /oracle_home/bin/rwconverter.sh stype=rdffile source={file_name} dtype={dtype} dest={dest_name} batch=yes overwrite=yes; "
        f"  echo 'Conversion complete: /work/{dest_name} generated.'; "
        f"else "
        f"  echo 'Notice: /work/{file_name} mounted successfully inside container.'; "
        f"  echo 'Error: Oracle Reports converter (rwconverter.sh) is not installed in /oracle_home/bin.'; "
        f"  echo ''; "
        f"  echo 'Root Cause: The container oracle_home was installed using Standalone Forms Builder 12c,'; "
        f"  echo 'which deliberately excludes Oracle Reports because Oracle 12c requires a separate full'; "
        f"  echo 'WebLogic / Fusion Middleware Infrastructure installation before Reports can be installed.'; "
        f"  echo ''; "
        f"  echo 'RECOMMENDED SOLUTION (Track 2 Pure-Native Rust Toolchain):'; "
        f"  echo '  Oracle Reports (.rdf) files share the exact same ROS.60050 storage format as Forms (.fmb).'; "
        f"  echo '  You can inspect and dump PL/SQL directly using the pure-native Rust tool:'; "
        f"  echo '    cd oracle-forms-viewer'; "
        f"  echo '    cargo run -p oracle-forms-cli -- inspect /path/to/{file_name}'; "
        f"  echo '    cargo run -p oracle-forms-cli -- dump-sql /path/to/{file_name} -o ./extracted-sql'; "
        f"  echo '  (Requires zero JVM, zero WebLogic, zero Oracle runtime, and runs instantaneously!)'; "
        f"  exit 2; "
        f"fi"
    )


# -----------------------------------------------------------------------------
# Custom Output File Copier
# -----------------------------------------------------------------------------
# Copies generated report output to user-specified destination path
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
# Report Conversion Execution
# -----------------------------------------------------------------------------
# Mounts host directory and dispatches rwconverter inside container
def convert_report(
    input_path_str: str,
    output_path_str: Optional[str] = None,
    target_format: str = "xml",
    custom_oracle_home: Optional[str] = None,
) -> int:
    """Mounts host directory into container and executes Oracle Reports conversion.

    Args:
        input_path_str: String path to the target Oracle Reports file (.rdf, .rpt, .rex).
        output_path_str: Optional destination path for converted file.
        target_format: Output format ('xml' for XML representation, 'rex' for text export).
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
    oracle_home_path = resolve_oracle_home(custom_oracle_home)

    if target_format.lower() == "rex":
        out_ext = ".rex"
        dtype = "rexfile"
    else:
        out_ext = "_rdf.xml"
        dtype = "xmlfile"

    expected_output = target_dir / f"{input_path.stem}{out_ext}"

    extra_mount: List[str] = []
    if oracle_home_path:
        extra_mount = ["-v", f"{oracle_home_path.as_posix()}:/oracle_home:Z"]

    print("=================================================================")
    print(" Oracle Reports Containerized Converter (rwconverter)")
    print("=================================================================")
    print(f" OS:             {platform.system()} ({platform.machine()})")
    print(f" File:           {file_name}")
    print(f" Host Directory: {target_dir}")
    print(f" Target Format:  {dtype} -> {out_ext}")
    if output_path_str:
        print(f" Target Output:  {output_path_str}")
    if extra_mount:
        print(f" Oracle Home:    {oracle_home_path} (mounted to /oracle_home)")
    else:
        print(" Oracle Home:    Default internal container path (/oracle_home)")
    print("=================================================================")

    container_cmd = build_container_script(file_name, input_path.stem, out_ext, dtype)
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
        description="Cross-platform Oracle Reports (.rdf/.rpt/.rex) containerized converter.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "rpt_path",
        nargs="?",
        help="Path to the target Oracle Reports file (.rdf, .rpt, .rex).",
    )
    parser.add_argument(
        "-o",
        "--output",
        dest="output_path",
        help="Destination path or directory for generated output file.",
    )
    parser.add_argument(
        "-f",
        "--format",
        dest="format",
        default="xml",
        choices=["xml", "rex"],
        help="Destination format: 'xml' (Reports XML, default) or 'rex' (ASCII text script).",
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
    """Parses command-line arguments and dispatches probe or report conversion actions.

    Returns:
        int: Standard system exit code.
    """
    args = parse_cli_args(sys.argv[1:])

    if args.check:
        return check_podman_readiness()

    if not args.rpt_path:
        print("Usage: convert_rpt.py <file.rdf|.rpt|.rex> [-o <output>] [-f xml|rex] [--oracle-home <path>]")
        print("       convert_rpt.py --check")
        return 1

    return convert_report(
        input_path_str=args.rpt_path,
        output_path_str=args.output_path,
        target_format=args.format,
        custom_oracle_home=args.oracle_home,
    )


if __name__ == "__main__":
    sys.exit(main())
