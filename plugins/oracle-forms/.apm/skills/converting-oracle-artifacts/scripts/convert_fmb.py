#!/usr/bin/env python3
"""
convert_fmb.py (CLI)
=====================================

Purpose:
    Cross-platform command-line tool to convert Oracle Forms binary files (.fmb, .mmb, .olb, .pll)
    into structured XML representations (*_fmb.xml, *_mmb.xml, *_olb.xml) or plain-text PL/SQL scripts
    (*.pld) via an on-demand Podman container on macOS, Windows, and Linux without requiring native
    local Oracle client installations.

Layer: Tooling / Container Bridge

Usage Examples:
    python3 oracle-reference-stack/convert_fmb.py --check
    python3 oracle-reference-stack/convert_fmb.py example-fmb/form0000.fmb
    python3 oracle-reference-stack/convert_fmb.py example-fmb/form0000.fmb -o example-fmb/custom_output.xml
    python3 oracle-reference-stack/convert_fmb.py example-fmb/formslib.pll
    python3 oracle-reference-stack/convert_fmb.py example-fmb/form0000.fmb --oracle-home /custom/oracle_home

Supported Object Types:
    - .fmb (Oracle Forms Binary Module) -> *_fmb.xml via frmf2xml
    - .mmb (Oracle Forms Menu Module)   -> *_mmb.xml via frmf2xml
    - .olb (Oracle Forms Object Library) -> *_olb.xml via frmf2xml
    - .pll (Oracle Forms PL/SQL Library) -> *.pld via frmcmp_batch

CLI Arguments:
    fmb_path              : Path to the target Oracle Forms binary file (.fmb, .mmb, .olb, .pll).
    -o, --output <path>   : Destination path for the converted output file (optional).
    --check               : Probes host container engine readiness and exits.
    --oracle-home <path>  : Path to local Oracle Home directory (optional).

Input Files:
    - oracle-reference-stack/Containerfile (Container image definition)
    - oracle-reference-stack/oracle_home/ (Optional drop-in directory for Oracle Forms runtime JARs or binaries)
        * frmxmltools.jar & frmjdapi.jar (Java Forms2XML tools)
        * OR bin/frmf2xml.sh (Full Oracle Forms Linux installation)
    - Target binary files (.fmb, .mmb, .olb)

Output:
    - <target_dir>/<module_name>_fmb.xml (Standard Oracle Forms XML representation)
    - OR custom destination path if specified via -o / --output

Key Functions:
    - check_podman_readiness() -> int
    - build_container_script(file_name: str, stem: str) -> str
    - resolve_oracle_home(custom_oracle_home: Optional[str]) -> Optional[Path]
    - copy_to_custom_output(source_xml: Path, output_path_str: Optional[str]) -> None
    - convert_file(input_path_str: str, output_path_str: Optional[str], custom_oracle_home: Optional[str]) -> int
    - parse_cli_args(argv: List[str]) -> argparse.Namespace
    - main() -> int

Script Dependencies:
    - Host Runtime:
        * Python 3.8+ (standard library: sys, shutil, platform, subprocess, pathlib, argparse, typing)
        * Podman CLI (or Docker) installed and active on host system
        * Podman Apple Silicon VM (4 vCPUs, 8GB RAM, 50GB disk; 'podman machine start')
        * Rosetta 2 binary translation active for linux/amd64 instruction emulation
    - Container Image ('forms-converter:latest' built via Containerfile):
        * Base OS: container-registry.oracle.com/os/oraclelinux:8-slim (linux/amd64)
        * Java Runtime: java-1.8.0-openjdk-headless (Java 8 JVM required by Oracle Forms2XML)
        * X11 / AWT Libraries: libX11, libXext, libXrender, libXtst (required for headless Forms toolkit)
        * C Runtime Linkers: glibc-devel (required by native JNI libfrmjnih.so)
        * Utilities: tar, gzip
        * Working Directory: /work (mounted from host parent directory via /work:Z)

Consumed by:
    - forms-viewer-poc/src/main/kotlin/com/viewer/forms/converter/ModuleConverter.kt
      (Automated ingestion pipeline: queries --check and dispatches conversion before direct binary fallback)
    - Developers performing manual, local Oracle Forms module inspections via CLI
"""

import sys
import shutil
import argparse
import platform
import subprocess
from pathlib import Path
from typing import Optional, List

import converter_runtime as rt  # shared 14c image, Oracle Home path, and Podman helper


# -----------------------------------------------------------------------------
# System Readiness Probe
# -----------------------------------------------------------------------------
def check_podman_readiness() -> int:
    """Verifies that the Podman CLI is installed, the VM is running, and the image exists.

    Returns:
        int: 0 if the container environment is ready to execute conversions, 1 otherwise.
    """
    return rt.check_readiness()


# -----------------------------------------------------------------------------
# Oracle Home Directory Resolver
# -----------------------------------------------------------------------------
def resolve_oracle_home(custom_oracle_home: Optional[str]) -> Optional[Path]:
    """Resolves the Oracle Home directory path from CLI options or default workspace location.

    Args:
        custom_oracle_home: Optional user-supplied path to an Oracle Home directory.

    Returns:
        Optional[Path]: Validated Path object if the directory exists on disk, None otherwise.
    """
    if custom_oracle_home:
        candidate = Path(custom_oracle_home).resolve()
        if candidate.is_dir():
            return candidate

    return rt.default_oracle_home()


# -----------------------------------------------------------------------------
# Container Bash Command Builder
# -----------------------------------------------------------------------------
def build_container_script(file_name: str, stem: str) -> str:
    """Generates the headless bash script string to be executed inside the container.

    Args:
        file_name: Base file name including extension (e.g., 'rccm0000.fmb', 'aglib.pll').
        stem: File name without extension (e.g., 'rccm0000', 'aglib').

    Returns:
        str: Multi-line bash command for testing tools and running conversion.
    """
    home = rt.HOME
    env = (
        f"export ORACLE_HOME={home}; "
        f"export ORACLE_TERM=vt220; "
        f"export TERM=vt220; "
        f"export LD_LIBRARY_PATH={home}/lib:{home}/oracle_common/lib:$LD_LIBRARY_PATH; "
        f"export FORMS_API_TK_BYPASS=TRUE; "
        f"export FORMS_PATH=/work:/work/shared:$FORMS_PATH; "
    )
    if file_name.lower().endswith(".pll"):
        # Forms 14c needs batch=yes for the PL/SQL library export (without it: PDE-PER001).
        batch = " batch=yes" if rt.IS_14C else ""
        return env + (
            f"cd /work; "
            f"echo 'Executing Oracle native frmcmp_batch utility to export PL/SQL library to .pld...'; "
            f"{home}/bin/frmcmp_batch module={file_name} module_type=library script=yes logon=no{batch}; "
        )

    classpath = f"{home}/jlib/frmjdapi.jar:{home}/jlib/frmxmltools.jar:{home}/oracle_common/modules/oracle.xdk/xmlparserv2.jar"
    return env + (
        f"if [ -f {home}/bin/frmf2xml.sh ]; then "
        f"  echo 'Executing Oracle native frmf2xml.sh utility...'; "
        f"  chmod +x {home}/bin/frmf2xml.sh 2>/dev/null || true; "
        f"  {home}/bin/frmf2xml.sh OVERWRITE=YES /work/{file_name}; "
        f"  echo 'Conversion complete.'; "
        f"elif [ -f {home}/jlib/frmxmltools.jar ]; then "
        f"  echo 'Executing the Forms2XML Java utility (Forms 14c ships it as a class; frmf2xml.sh is only a template)...'; "
        f"  java -Djava.library.path={home}/lib -classpath {classpath} "
        f"       oracle.forms.util.xmltools.Forms2XML OVERWRITE=YES /work/{file_name}; "
        f"  echo 'Conversion complete.'; "
        f"elif [ -f {home}/forms/java/frmxmltools.jar ]; then "
        f"  echo 'Executing Java Forms2XML directly with bundled JARs...'; "
        f"  java -cp {home}/forms/java/frmxmltools.jar:{home}/forms/java/frmjdapi.jar "
        f"       oracle.forms.util.xmltools.Forms2XML OVERWRITE=YES /work/{file_name}; "
        f"  echo 'Conversion complete.'; "
        f"else "
        f"  echo 'Notice: /work/{file_name} mounted successfully inside container.'; "
        f"  echo 'Oracle runtime binaries not yet found in {home}.'; "
        f"  echo 'To enable native conversion, point --oracle-home at an Oracle Forms Linux install'; "
        f"  echo '(oracle-reference-stack/oracle_home_14c).'; "
        f"  exit 2; "
        f"fi"
    )

# -----------------------------------------------------------------------------
# Custom Output File Copier
# -----------------------------------------------------------------------------
def copy_to_custom_output(source_file: Path, output_path_str: Optional[str]) -> None:
    """Copies generated output file to a user-specified destination path if provided.

    Args:
        source_file: Path to the generated default sidecar file (.xml or .pld).
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
# File Conversion Execution
# -----------------------------------------------------------------------------
def convert_file(
    input_path_str: str,
    output_path_str: Optional[str] = None,
    custom_oracle_home: Optional[str] = None,
) -> int:
    """Mounts the host directory into an on-demand container and executes file conversion.

    Args:
        input_path_str: String path to the target Oracle Forms file (.fmb, .mmb, .olb, .pll).
        output_path_str: Optional destination path for the converted output.
        custom_oracle_home: Optional path to an external Oracle Home runtime folder.

    Returns:
        int: Process exit code (0 for success, 1 for fatal error, 2 for missing Oracle runtime).
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
        extra_mount = ["-v", f"{rt.host_path(oracle_home_path)}:{rt.HOME}:Z"]

    print("=================================================================")
    print(" Oracle Forms Containerized Converter (Cross-Platform Python)")
    print("=================================================================")
    print(f" OS:             {platform.system()} ({platform.machine()})")
    print(f" File:           {file_name}")
    print(f" Host Directory: {target_dir}")
    if output_path_str:
        print(f" Target Output:  {output_path_str}")
    if extra_mount:
        print(f" Oracle Home:    {oracle_home_path} (mounted to {rt.HOME})")
    else:
        print(f" Oracle Home:    Default internal container path ({rt.HOME})")
    print("=================================================================")

    # Determine expected output filename based on module type
    if file_ext == ".pll":
        expected_output = target_dir / f"{input_path.stem}.pld"
    elif file_ext == ".mmb":
        expected_output = target_dir / f"{input_path.stem}_mmb.xml"
    elif file_ext == ".olb":
        expected_output = target_dir / f"{input_path.stem}_olb.xml"
    else:
        expected_output = target_dir / f"{input_path.stem}_fmb.xml"

    container_cmd = build_container_script(file_name, input_path.stem)
    cmd = rt.podman_cmd([
        "run", "--rm", "--platform", "linux/amd64",
        "-v", f"{rt.host_path(target_dir)}:/work:Z",
        *extra_mount,
        rt.IMAGE, "bash", "-c", container_cmd,
    ])

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
def parse_cli_args(argv: List[str]) -> argparse.Namespace:
    """Configures and parses command-line arguments using standard argparse.

    Args:
        argv: Command line argument list.

    Returns:
        argparse.Namespace: Parsed options and flags.
    """
    parser = argparse.ArgumentParser(
        description="Cross-platform Oracle Forms containerized converter.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "fmb_path",
        nargs="?",
        help="Path to the target Oracle Forms binary file (.fmb, .mmb, .olb, .pll).",
    )
    parser.add_argument(
        "-o",
        "--output",
        dest="output_path",
        help="Destination path or directory for the generated output file (.xml or .pld).",
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
def main() -> int:
    """Parses command-line arguments and dispatches probe or conversion actions.

    Returns:
        int: Standard system exit code.
    """
    args = parse_cli_args(sys.argv[1:])

    if args.check:
        return check_podman_readiness()

    if not args.fmb_path:
        print("Usage: convert_fmb.py <file.fmb|.mmb|.olb|.pll> [-o <output>] [--oracle-home <path>]")
        print("       convert_fmb.py --check")
        return 1

    return convert_file(args.fmb_path, args.output_path, args.oracle_home)


if __name__ == "__main__":
    sys.exit(main())
