#!/usr/bin/env python3
"""
run_forms_builder.py (CLI)
=====================================

Purpose:
    Cross-platform launcher for the native Oracle Forms Builder (frmbld) 12c GUI
    inside a containerized Podman environment on macOS (Apple Silicon / Intel)
    and Linux with automatic X11 / XQuartz display routing and classpath assembly.

Layer: Tooling / Container GUI Launcher

Usage Examples:
    python3 tools/podman-converter/run_forms_builder.py
    python3 tools/podman-converter/run_forms_builder.py /work/samples/simple_module.fmb
    python3 tools/podman-converter/run_forms_builder.py --oracle-home /custom/oracle_home

Supported Object Types:
    - .fmb (Oracle Forms Binary Module)
    - .mmb (Oracle Forms Menu Module)
    - .olb (Oracle Forms Object Library)
    - .pll (Oracle Forms PL/SQL Library)

CLI Arguments:
    modules               : Optional paths to module files inside /work to open on startup.
    --oracle-home <path>  : Path to host directory containing Oracle Home runtime.

Input Files:
    - tools/podman-converter/oracle_home/bin/frmbld (Forms Builder binary)
    - tools/podman-converter/oracle_home/bin/frmbld.sh (Forms Builder environment script)
    - tools/podman-converter/oracle_home/jlib/ (Oracle GUI JARs for Forms Builder)
    - Target form, menu, library or object files (.fmb, .mmb, .olb, .pll)

Output:
    - Interactive X11 GUI window rendered on host desktop via XQuartz (macOS) or native X11 (Linux).

Key Functions:
    - check_prerequisites(oracle_home: Path) -> int
    - configure_x11_forwarding() -> Optional[str]
    - build_forms_builder_classpath() -> str
    - launch_forms_builder(modules: List[str], oracle_home_override: Optional[str]) -> int
    - parse_cli_args(argv: List[str]) -> argparse.Namespace
    - main() -> int

Script Dependencies:
    - Host Runtime:
        * Python 3.8+ (sys, os, shutil, platform, subprocess, pathlib, argparse, typing, time)
        * Podman CLI (or Docker) installed and active on host system
        * macOS: XQuartz 2.8+ installed (`brew install --cask xquartz`) with network connections enabled
    - Container Image ('forms-converter:latest'):
        * Oracle Linux 8 Slim (linux/amd64) with X11, Motif, and OpenJDK 8 headless/GUI packages

Consumed by:
    - Developers inspecting Oracle Forms modules, layouts, and triggers in the official IDE
    - Differential testing between native Rust viewer and Oracle visual ground truth
"""

import os
import sys
import time
import shutil
import argparse
import platform
import subprocess
from pathlib import Path
from typing import Optional, List, Tuple


# List of required Oracle Java archive dependencies for Forms Builder IDE
REQUIRED_CLASSPATH_JARS: Tuple[str, ...] = (
    "/oracle_home/jlib/frmbld.jar",
    "/oracle_home/jlib/importer.jar",
    "/oracle_home/jlib/debugger.jar",
    "/oracle_home/jlib/utj.jar",
    "/oracle_home/oracle_common/jlib/ewt3.jar",
    "/oracle_home/oracle_common/modules/oracle.bali.share/share.jar",
    "/oracle_home/jlib/dfc.jar",
    "/oracle_home/oracle_common/modules/oracle.help/ohj.jar",
    "/oracle_home/oracle_common/modules/oracle.help/help-share.jar",
    "/oracle_home/oracle_common/modules/oracle.help/oracle_ice.jar",
    "/oracle_home/oracle_common/modules/oracle.bali.jewt/jewt4.jar",
    "/oracle_home/forms/java/frmwebutil.jar",
    "/oracle_home/forms/java/frmall.jar",
)


# Verifies host system requirements and Oracle Home directory readiness
def check_prerequisites(oracle_home: Path) -> int:
    """Validates that Podman CLI and the required Oracle Forms binaries are present.

    Args:
        oracle_home: Path to the host directory containing Oracle Home runtime.

    Returns:
        int: 0 if all prerequisites are met, non-zero otherwise.
    """
    if not shutil.which("podman"):
        print("Error: 'podman' command not found in system PATH.")
        print("Please install Podman and start the machine: podman machine start")
        return 1

    frmbld_bin = oracle_home / "bin" / "frmbld"
    frmbld_sh = oracle_home / "bin" / "frmbld.sh"
    if not frmbld_bin.is_file():
        print(f"Error: Oracle Forms Builder binary not found at: {frmbld_bin}")
        print("Please ensure tools/podman-converter/oracle_home contains Forms 12c binaries.")
        return 1

    if not frmbld_sh.is_file():
        print(f"Error: Oracle Forms Builder launcher script not found at: {frmbld_sh}")
        return 1

    return 0


# Configures X11 display permissions and returns container DISPLAY target
def configure_x11_forwarding() -> Optional[str]:
    """Sets up X11 authorization on the host and resolves container DISPLAY value.

    Returns:
        Optional[str]: Container DISPLAY target string, or None if X11 setup failed.
    """
    # Ensure X11 paths are present in environment
    x11_paths = ["/opt/X11/bin", "/usr/X11/bin"]
    current_path = os.environ.get("PATH", "")
    for p in x11_paths:
        if p not in current_path and Path(p).is_dir():
            os.environ["PATH"] = f"{p}:{os.environ.get('PATH', '')}"

    os_type = platform.system()
    if os_type == "Darwin":
        print("Host OS: macOS (Apple Silicon / Intel)")
        if not shutil.which("xhost"):
            print("Error: 'xhost' not found in PATH or /opt/X11/bin.")
            print("Please install XQuartz: brew install --cask xquartz")
            return None

        # Check if XQuartz is running
        check_proc = subprocess.run(["pgrep", "-i", "Xquartz"], capture_output=True)
        if check_proc.returncode != 0:
            print("Starting XQuartz application...")
            subprocess.run(["open", "-a", "XQuartz"])
            time.sleep(2)

        print("Authorizing X11 connections for container...")
        for host in ["", "127.0.0.1", "host.containers.internal"]:
            cmd = ["xhost", f"+{host}".strip()]
            subprocess.run(cmd, env={"DISPLAY": ":0", "PATH": os.environ["PATH"]}, capture_output=True)

        return "host.containers.internal:0"

    print("Host OS: Linux / Unix")
    subprocess.run(["xhost", "+local:"], capture_output=True)
    return os.environ.get("DISPLAY", ":0")


# Assembles the colon-delimited Java classpath required by Forms Builder
def build_forms_builder_classpath() -> str:
    """Builds the complete FORMS_BUILDER_CLASSPATH required by frmbld.jar.

    Returns:
        str: Colon-separated list of JAR file paths inside the container.
    """
    return ":".join(REQUIRED_CLASSPATH_JARS)


# Dispatches the container run command to start Oracle Forms Builder
def launch_forms_builder(modules: List[str], oracle_home_override: Optional[str] = None) -> int:
    """Executes the Podman container running Oracle Forms Builder in interactive GUI mode.

    Args:
        modules: Optional list of module paths to pass to frmbld.
        oracle_home_override: Optional user-specified path to Oracle Home.

    Returns:
        int: Process exit code from the container execution.
    """
    script_dir = Path(__file__).resolve().parent
    workspace_root = script_dir.parent.parent

    if oracle_home_override:
        oracle_home = Path(oracle_home_override).resolve()
    else:
        oracle_home = script_dir / "oracle_home"

    prereq_status = check_prerequisites(oracle_home)
    if prereq_status != 0:
        return prereq_status

    container_display = configure_x11_forwarding()
    if not container_display:
        return 1

    classpath = build_forms_builder_classpath()

    print("=================================================================")
    print(" Oracle Forms Builder 12c GUI Launcher (Python CLI)")
    print("=================================================================")
    print(f" Connecting to Display: {container_display}")
    print(f" Mounting Oracle Home:   {oracle_home}")
    print(f" Mounting Workspace:     {workspace_root} -> /work")
    print("-----------------------------------------------------------------")
    print(" Starting Oracle Forms Builder (frmbld)...")
    print(" Tip: Inside Forms Builder, use File > Open and navigate to /work")
    print("=================================================================")

    # Prepare Podman run arguments
    cmd = [
        "podman",
        "run",
        "--rm",
        "-it",
        "--platform",
        "linux/amd64",
        "-e",
        f"DISPLAY={container_display}",
        "-e",
        "ORACLE_HOME=/oracle_home",
        "-e",
        "JAVA_HOME=/usr/lib/jvm/jre-1.8.0-openjdk",
        "-e",
        "O_JDK_HOME=/usr/lib/jvm/jre-1.8.0-openjdk",
        "-e",
        "LD_LIBRARY_PATH=/oracle_home/lib:/usr/lib/jvm/jre-1.8.0-openjdk/lib/amd64/server:/usr/lib/jvm/jre-1.8.0-openjdk/lib/amd64",
        "-e",
        f"FORMS_BUILDER_CLASSPATH={classpath}",
        "-e",
        "FORMS_PATH=/work:/work/samples:/work/temp/example-fmb:/work/temp/example-fmb/internal-testing",
        "-v",
        f"{oracle_home.as_posix()}:/oracle_home:Z",
        "-v",
        f"{workspace_root.as_posix()}:/work:Z",
        "forms-converter:latest",
        "/oracle_home/bin/frmbld.sh",
    ] + modules

    try:
        proc = subprocess.run(cmd)
        return proc.returncode
    except KeyboardInterrupt:
        print("\nForms Builder session terminated by user.")
        return 0
    except Exception as err:
        print(f"Failed to launch Forms Builder: {err}")
        return 1


# Parses command line arguments for the GUI launcher
def parse_cli_args(argv: List[str]) -> argparse.Namespace:
    """Configures and parses command-line options for running Forms Builder.

    Args:
        argv: Command line argument tokens.

    Returns:
        argparse.Namespace: Parsed options and argument namespace.
    """
    parser = argparse.ArgumentParser(
        description="Launch native Oracle Forms Builder 12c GUI inside Podman container.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "modules",
        nargs="*",
        help="Optional module files to load on startup (e.g., /work/samples/simple_module.fmb).",
    )
    parser.add_argument(
        "--oracle-home",
        dest="oracle_home",
        help="Path to host directory containing Oracle Home runtime.",
    )
    return parser.parse_args(argv)


# Main script entry point
def main() -> int:
    """Dispatches command-line parsing and launches the Forms Builder container.

    Returns:
        int: Standard program exit code.
    """
    args = parse_cli_args(sys.argv[1:])
    return launch_forms_builder(args.modules, args.oracle_home)


if __name__ == "__main__":
    sys.exit(main())
