"""
converter_runtime.py: shared container settings and Podman helpers for the convert_*.py scripts.

Purpose:
    The three converters (convert_fmb.py, convert_pll.py, convert_rpt.py) run Oracle's own tools inside the
    Forms 14c container image. This module holds what they share, so each script stays small:
      - the image name and the Oracle Home path inside the container (14c by default);
      - the default Oracle Home folder on the host (oracle-reference-stack/oracle_home_14c);
      - a Podman command builder that also works from Windows PowerShell and inside WSL;
      - the readiness check behind `--check`.

Settings:
    OFW_CONVERTER_IMAGE   Container image to run (default: forms-converter-14c:latest). Set it to
                          forms-converter:latest to use the older 12c image; the Oracle Home path inside the
                          container then becomes /oracle_home instead of /oracle_home_14c.
    OFW_WSL_DISTRO        WSL distribution that runs Podman when a script is started from Windows PowerShell
                          (default: Ubuntu).

Platforms:
    macOS and Linux: plain `podman`, unchanged.
    WSL (Linux on Windows): Podman is local; the runtime directory is /run/user/<uid>, and `podman run` uses
    --user root, because rootless Podman maps the container's "oracle" user to a UID that cannot write files
    on the Windows drive (/mnt/c).
    Windows PowerShell: the call goes through `wsl -d <distro>` and host paths are converted with `wslpath`.
"""

import os
import platform
import shlex
import shutil
import subprocess
from pathlib import Path
from typing import List, Optional

IMAGE = os.environ.get("OFW_CONVERTER_IMAGE", "forms-converter-14c:latest")
IS_14C = "14c" in IMAGE
# Oracle Home path inside the container.
HOME = "/oracle_home_14c" if IS_14C else "/oracle_home"
IS_WINDOWS = platform.system() == "Windows"
IS_WSL = platform.system() == "Linux" and "microsoft" in platform.uname().release.lower()


def repo_root() -> Path:
    """Repository root (this file lives in plugins/oracle-forms/.apm/skills/<skill>/scripts)."""
    return Path(__file__).resolve().parents[6]


def _dotenv_value(name: str) -> str:
    """One KEY from the git-ignored .env at the repo root, or an empty string."""
    env_file = repo_root() / ".env"
    if env_file.is_file():
        for raw in env_file.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if line and not line.startswith("#") and "=" in line:
                key, value = line.split("=", 1)
                if key.strip() == name:
                    return value.strip().strip('"').strip("'")
    return ""


def default_oracle_home() -> Optional[Path]:
    """Oracle Home folder on the host that matches the image, if it exists.

    For the 14c image, OFW_ORACLE_HOME (environment, then .env) overrides the default
    oracle-reference-stack/oracle_home_14c, so the Oracle Home can live on Ubuntu's own disk
    (see the setup skill's references/oracle-home-location.md).
    """
    if IS_14C:
        override = os.environ.get("OFW_ORACLE_HOME") or _dotenv_value("OFW_ORACLE_HOME")
        if override:
            path = Path(os.path.expanduser(override))
            return path if path.is_dir() else None
    folder = "oracle_home_14c" if IS_14C else "oracle_home"
    candidate = repo_root() / "oracle-reference-stack" / folder
    return candidate if candidate.is_dir() else None


def _wsl_distro() -> str:
    return os.environ.get("OFW_WSL_DISTRO") or "Ubuntu"


def podman_cmd(args: List[str]) -> List[str]:
    """Podman command line for the current platform (see the module docstring)."""
    args = list(args)
    if (IS_WINDOWS or IS_WSL) and args and args[0] == "run":
        # Files on the Windows drive belong to the Windows user, which rootless Podman maps to container
        # root. Run as root so the container can write the converted file next to the input.
        args[1:1] = ["--user", "root"]
    if IS_WINDOWS:
        script = "export XDG_RUNTIME_DIR=/run/user/$(id -u); podman " + " ".join(shlex.quote(a) for a in args)
        return ["wsl", "-d", _wsl_distro(), "--", "bash", "-lc", script]
    if IS_WSL:
        return ["env", f"XDG_RUNTIME_DIR=/run/user/{os.getuid()}", "podman", *args]
    return ["podman", *args]


def host_path(path: Path) -> str:
    """Host path in the form Podman needs for a -v mount."""
    if IS_WINDOWS:
        converted = subprocess.run(["wsl", "-d", _wsl_distro(), "--", "wslpath", "-u", str(path)],
                                   capture_output=True, text=True)
        if converted.returncode == 0 and converted.stdout.strip():
            return converted.stdout.strip()
    return path.as_posix()


def check_readiness() -> int:
    """Implements `--check`: the Podman CLI, the Podman machine (macOS), and the converter image."""
    if not shutil.which("wsl" if IS_WINDOWS else "podman"):
        print("NOT_READY: 'podman' command not found in system PATH")
        return 1
    if platform.system().lower() == "darwin":
        try:
            res = subprocess.run(["podman", "machine", "list"], capture_output=True, text=True, timeout=10)
            if "running" not in res.stdout.lower():
                print("NOT_READY: Podman machine is not running. Run: podman machine start")
                return 1
        except Exception as err:
            print(f"NOT_READY: Error querying podman machine: {err}")
            return 1
    try:
        # The first Podman call after a restart can be slow, so allow more than a few seconds.
        res = subprocess.run(podman_cmd(["image", "exists", IMAGE]), capture_output=True, timeout=20)
    except Exception as err:
        print(f"NOT_READY: Error querying podman image: {err}")
        return 1
    if res.returncode != 0:
        print(f"NOT_READY: Image '{IMAGE}' not found. Build it in oracle-reference-stack "
              f"(or set OFW_CONVERTER_IMAGE to another image).")
        return 1
    print(f"READY: Podman engine and '{IMAGE}' image are operational.")
    return 0
