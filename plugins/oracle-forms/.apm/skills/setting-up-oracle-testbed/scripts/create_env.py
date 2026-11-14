"""
create_env.py: create the git-ignored .env file for the Oracle testbed.

Purpose:
    Copies example.env (repo root) to .env, then prompts for each value that is
    still empty. Passwords are read with hidden input and are never printed or
    written anywhere except .env.

Usage:
    python3 scripts/create_env.py            # create .env, prompt for empty values
    python3 scripts/create_env.py --force    # overwrite an existing .env (asks first)

Notes:
    - Never commit .env. The repo's .gitignore lists it; this script checks that.
    - Local development only. Use throwaway-container passwords.
    - Load the file in bash before running the skill commands:
        set -a; source .env; set +a
    - Podman can read the same file for containers: --env-file .env
"""

import argparse
import getpass
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple

PASSWORD_KEYS = {"ORACLE_PWD", "DB_PASSWORD", "OFW_DB_PASSWORD", "WLS_ADMIN_PASSWORD"}


def find_repo_root(start: Path) -> Path:
    """Walk up from start until a folder containing example.env is found."""
    for folder in [start, *start.parents]:
        if (folder / "example.env").is_file():
            return folder
    raise SystemExit("example.env not found above this script. Run from the repository.")


def parse_env(path: Path) -> List[Tuple[Optional[str], str]]:
    """Return (key, raw_line) pairs, keeping comments and blank lines so the file keeps its layout."""
    rows: List[Tuple[Optional[str], str]] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        stripped = raw.strip()
        if stripped and not stripped.startswith("#") and "=" in stripped:
            key, value = stripped.split("=", 1)
            rows.append((key.strip(), value.strip()))
        else:
            rows.append((None, raw))
    return rows


def check_gitignored(repo_root: Path) -> bool:
    """True when git reports .env as ignored (or git is unavailable and .gitignore lists it)."""
    try:
        result = subprocess.run(["git", "check-ignore", "-q", ".env"], cwd=repo_root, capture_output=True)
        if result.returncode in (0, 1):
            return result.returncode == 0
    except FileNotFoundError:
        pass
    gitignore = repo_root / ".gitignore"
    return gitignore.is_file() and any(line.strip() == ".env" for line in gitignore.read_text(encoding="utf-8").splitlines())


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Create the git-ignored .env from example.env")
    parser.add_argument("--force", action="store_true", help="Overwrite an existing .env (asks first)")
    args = parser.parse_args(argv)

    repo_root = find_repo_root(Path(__file__).resolve().parent)
    example = repo_root / "example.env"
    target = repo_root / ".env"

    if target.exists() and not args.force:
        print(f".env already exists at {target}. Edit it, or rerun with --force to recreate it.")
    else:
        if target.exists():
            answer = input(".env exists. Overwrite it with a fresh copy of example.env? [y/N] ").strip().lower()
            if answer != "y":
                print("Left .env unchanged.")
                return 0
        target.write_text(example.read_text(encoding="utf-8"), encoding="utf-8", newline="\n")
        print(f"Created {target} from example.env")

    rows = parse_env(target)
    filled = 0
    for index, (key, value) in enumerate(rows):
        if key is None or value:
            continue
        prompt = f"{key}: "
        entered = getpass.getpass(prompt) if key in PASSWORD_KEYS else input(prompt)
        if entered:
            rows[index] = (key, entered)
            filled += 1
        else:
            print(f"  {key} left empty. Set it in .env before using the commands that need it.")

    # Comment and blank rows keep their original text (stored in the value slot with key None).
    text = "\n".join(value if key is None else f"{key}={value}" for key, value in rows)
    target.write_text(text + "\n", encoding="utf-8", newline="\n")
    print(f"Saved {filled} value(s) to {target}")

    if check_gitignored(repo_root):
        print(".env is git-ignored. Do not commit it.")
    else:
        print("WARNING: .env is NOT git-ignored. Add '.env' to .gitignore before committing.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
