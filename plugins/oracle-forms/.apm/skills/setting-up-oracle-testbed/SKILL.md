---
name: setting-up-oracle-testbed
description: "Provisions and configures the complete Oracle Forms 14c and Database 23ai Free container testbed. Use when setting up the database, building the Forms 14c container, relinking runtime libraries, provisioning WebLogic domains, or troubleshooting container errors."
---

# Setting Up Oracle Testbed

Provisions and configures the complete Oracle Forms 14c and Database 23ai Free container testbed. Use when setting up the database, building the Forms 14c container, relinking runtime libraries, provisioning WebLogic domains, or troubleshooting container errors.

## Choose your platform first

| Host | Runbook | Notes |
| :--- | :--- | :--- |
| macOS (Apple Silicon) | [references/podman-setup-guide.md](references/podman-setup-guide.md) | Podman machine with Rosetta 2 (`linux/amd64`) |
| Windows 11 | [references/windows-podman-setup.md](references/windows-podman-setup.md) | Podman runs inside Ubuntu on WSL2 (x86-64, no emulation). Use `Linux x86-64` Oracle installers, not the Windows `.exe`. Full status log: [docs/WINDOWS-PODMAN-SETUP.md](../../../../../../docs/WINDOWS-PODMAN-SETUP.md) |

Windows-specific rules for this skill:
- Run container commands from the Ubuntu WSL shell (`wsl -d Ubuntu`), not PowerShell.
- Use `curl.exe` in PowerShell, not the `curl` alias.
- Keep the `oracle-reference-stack/installer/` folder LF-only for shell scripts; do not write `.sh` or `.rsp` files with PowerShell heredocs.
- Keep the status log in `docs/WINDOWS-PODMAN-SETUP.md` current as steps run.

Host prerequisites a Windows setup must check (full list: [docs/DEPENDENCIES.md](../../../../../docs/DEPENDENCIES.md) sections 2.1, 2.4-2.6):
- WSL 2 current (`wsl --version`, verified 3.0.5.0), and an Ubuntu 24.04 distro.
- Podman inside Ubuntu (`podman --version`). Create `/run/user/$(id -u)` with sudo once. Without it, every `podman build` and `podman run` fails.
- `%USERPROFILE%\.wslconfig` with `memory=12GB` (applies to all distros; needs `wsl --shutdown`).
- Oracle installers in `oracle-reference-stack/installer/` (Linux x86-64 zips, JDK 17 tarball). Never commit them.
- Host JDK 17 or 21 for `frmsal.jar`. Host Java 26 is unverified with FSAL.
- A writable Oracle Home. `/mnt/c` bind mounts reject `chmod +x` (verified). Do not install into a Windows bind mount until a metadata or volume option is verified.

## Contents

- [Quick start](#quick-start)
- [Workflow](#workflow)
- [Verification](#verification)

## Quick start

Run from this skill's root:

```bash
python3 scripts/setting_up_oracle_testbed.py --help
```

## Workflow

1. Check the script's declared inputs and dependencies.
2. Run the supplied script with the requested inputs. Use its diagnostics to fix errors.
3. Verify the output before reporting success.

## Verification

Create at least three representative task scenarios and compare against a baseline.
Keep routing outcomes in `evals/evals.json` and task-success cases separately.
Report executed checks and any missing model coverage.

<!-- Author: implement the canonical script before publishing; --help alone is not a
task-success check. Put critical constraints early, name required dependencies and
link task-specific references directly. -->
