#!/usr/bin/env python3
"""
compile_form.py (CLI)
=====================================

Purpose:
    Headless batch compiler wrapper for Oracle Forms (.fmb), menus (.mmb),
    and libraries (.pll) using frmcmp_batch inside the Forms 14c container
    with automatic environment variable injection, TNS Net services binding,
    and structured error parsing.

Layer: Tooling / Skill Script (compiling-and-running-oracle-forms)

Usage Examples:
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/compile_form.py samples/test_db.fmb
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/compile_form.py samples/simple_module.fmb --offline
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/compile_form.py samples/mainmenu.mmb --type menu

Supported Object Types:
    - .fmb (Oracle Forms binary form module) -> .fmx
    - .mmb (Oracle Forms binary menu module) -> .mmx
    - .pll (Oracle Forms binary PL/SQL library) -> .plx

CLI Arguments:
    module                : Path to input .fmb, .mmb, or .pll file.
    -t, --type <str>      : Module type (form, menu, library; default: form).
    --offline             : Compile offline without database logon (logon=no).
    -u, --userid <str>    : Database logon connection string (default: DB_USER/DB_PASSWORD/DB_SERVICE from .env).

Input Files:
    - Target source module (.fmb, .mmb, or .pll)
    - Container 'forms-server-14c' running with /oracle_home_14c/bin/frmcmp_batch
    - TNS configuration: /oracle_home_14c/user_projects/domains/base_domain/config/fmwconfig/tnsnames.ora

Output:
    - Compiled executable file (.fmx, .mmx, or .plx) placed in source directory

Key Functions:
    - resolve_workspace_root() -> Path
    - build_compiler_command(container_path: str, mod_type: str, logon_param: str) -> str
    - execute_compilation(cmd: str) -> int
    - parse_cli_args(argv: List[str]) -> argparse.Namespace
    - main() -> int

Script Dependencies:
    - Python 3.8+ (standard library: sys, os, argparse, subprocess, pathlib, typing)
    - Podman CLI available on host system
    - Running container: forms-server-14c

Consumed by:
    - run_form.py (compilation stage before launch)
    - Developers running batch compiles of legacy forms
"""

import sys
import os
import argparse
import platform
import shlex
import subprocess
from pathlib import Path
from typing import List


# Resolves the absolute path to the repository root directory.
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
    """Locate the top-level repository root directory.

    Returns:
        Path pointing to the repository root.
    """
    return Path(__file__).resolve().parents[6]


# Constructs the shell command executed inside the Forms 14c container for frmcmp_batch.
def build_compiler_command(container_path: str, mod_type: str, logon_param: str) -> str:
    """Build the environment exports and frmcmp_batch invocation string.

    Args:
        container_path: Path to module inside /work mount.
        mod_type: Module type (form, menu, library).
        logon_param: Database logon argument string.

    Returns:
        Complete bash shell command string.
    """
    return (
        f"export ORACLE_HOME=/oracle_home_14c; "
        f"export LD_LIBRARY_PATH=/oracle_home_14c/lib; "
        f"export ORACLE_TERM=vt220; "
        f"export TERM=vt220; "
        f"export FORMS_PATH=/oracle_home_14c/forms:/work/samples:{container_path.rsplit('/', 1)[0]}; "
        f"export TNS_ADMIN=/oracle_home_14c/user_projects/domains/base_domain/config/fmwconfig; "
        f"/oracle_home_14c/bin/frmcmp_batch "
        f"module={container_path} "
        f"module_type={mod_type} "
        f"{logon_param} "
        f"compile_all=yes"
    )


# Running inside WSL (Linux on a Windows machine): Podman is local, but the repo is on the Windows drive.
IS_WSL = platform.system() == "Linux" and "microsoft" in platform.uname().release.lower()


def podman_cmd(args: List[str]) -> List[str]:
    """Podman command line.

    Windows: Podman runs inside WSL, so the call goes through `wsl -d <distro>` (OFW_WSL_DISTRO, default Ubuntu).
    WSL: Podman is local; its runtime directory must be /run/user/<uid>.
    macOS and Linux: plain `podman`, unchanged.
    """
    on_windows = platform.system() == "Windows"
    if on_windows or IS_WSL:
        # Files on the Windows drive belong to the Windows user, which rootless Podman maps to container
        # root. The container's "oracle" user cannot write the compiled .fmx next to the module (FRM-10044),
        # so run exec as root.
        if args and args[0] == "exec":
            rest = list(args[1:])
            if "-u" in rest and rest.index("-u") + 1 < len(rest):
                rest[rest.index("-u") + 1] = "root"
            else:
                rest[:0] = ["-u", "root"]
            args = ["exec", *rest]
    if on_windows:
        distro = os.environ.get("OFW_WSL_DISTRO") or "Ubuntu"
        script = "export XDG_RUNTIME_DIR=/run/user/$(id -u); podman " + " ".join(shlex.quote(a) for a in args)
        return ["wsl", "-d", distro, "--", "bash", "-lc", script]
    if IS_WSL:
        return ["env", f"XDG_RUNTIME_DIR=/run/user/{os.getuid()}", "podman", *args]
    return ["podman", *args]


# Dispatches the compilation command inside the container and processes output.
def execute_compilation(cmd: str) -> int:
    """Execute the compiler inside the container via podman exec.

    Args:
        cmd: Shell command string to execute.

    Returns:
        0 on success, non-zero on compilation failure.
    """
    res = subprocess.run(podman_cmd(["exec", "-u", "oracle", "forms-server-14c", "/bin/sh", "-c", cmd]),
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    print(res.stdout)

    if "Created form file" in res.stdout or "Created menu file" in res.stdout or res.returncode == 0:
        print("Compilation SUCCESSFUL.")
        return 0
    else:
        print(f"Compilation FAILED with exit code {res.returncode}.", file=sys.stderr)
        return 1


# Parses CLI arguments for compile_form.
def parse_cli_args(argv: List[str]) -> argparse.Namespace:
    """Parse command-line arguments for the form compiler.

    Args:
        argv: List of raw CLI arguments.

    Returns:
        Parsed arguments namespace.
    """
    parser = argparse.ArgumentParser(description="Headless Oracle Forms Batch Compiler")
    parser.add_argument("module", help="Path to input .fmb, .mmb, or .pll file")
    parser.add_argument("-t", "--type", default="form", choices=["form", "menu", "library"], help="Module type (default: form)")
    parser.add_argument("--offline", action="store_true", help="Compile offline without database logon (logon=no)")
    parser.add_argument("-u", "--userid", default=None, help="Database logon connection string")
    args = parser.parse_args(argv)
    if args.userid is None:
        args.userid = _default_userid()
    return args


# Main execution entry point for compile_form.py.
def main() -> int:
    """Validate inputs, resolve paths, and trigger headless compilation.

    Returns:
        0 on success, 1 on failure.
    """
    args = parse_cli_args(sys.argv[1:])
    input_path = Path(args.module).resolve()

    if not input_path.exists():
        print(f"Error: Input module not found: {input_path}", file=sys.stderr)
        return 1

    repo_root = resolve_workspace_root()
    try:
        rel_path = input_path.relative_to(repo_root)
        container_module_path = f"/work/{rel_path.as_posix()}"
    except ValueError:
        container_module_path = f"/work/{input_path.name}"

    logon_param = "logon=no" if args.offline else f"userid='{args.userid}'"
    print(f"Compiling {input_path.name} ({args.type}) via container frmcmp_batch...")
    cmd = build_compiler_command(container_module_path, args.type, logon_param)
    return execute_compilation(cmd)


if __name__ == "__main__":
    sys.exit(main())
