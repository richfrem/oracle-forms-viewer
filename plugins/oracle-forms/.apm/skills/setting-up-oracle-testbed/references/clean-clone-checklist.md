# Clean-clone checklist: from a fresh `git clone` to a running testbed

The ordered list for a new machine. Each step links to the page with the commands and the reasons. Run everything from the repo root; on Windows, in the **Ubuntu WSL shell** (never PowerShell). Credentials live only in the git-ignored `.env`. After each step, record what happened in [windows-wsl-change-log.md](windows-wsl-change-log.md) (the repo rule `.agent/rules/document-every-step.md`).

What you need to bring: an Oracle account for eDelivery, the three installer files ([installing-forms-14c.md](installing-forms-14c.md) step 2), about 25 GB of free disk, and 16 GB of RAM (32 GB recommended).

| # | Step | macOS (Apple Silicon) | Windows 11 |
|---|---|---|---|
| 1 | Host prerequisites | `brew install podman`; Podman machine; Rosetta 2 check: [podman-setup-guide.md](podman-setup-guide.md) | WSL2 and Ubuntu 24.04; `/etc/wsl.conf` automount options; Podman in Ubuntu; `/run/user/<uid>`; Ubuntu packages (`podman python3 build-essential unzip openjdk-21-jre libxml2-utils`): [windows-podman-setup.md](windows-podman-setup.md) sections 2 and 3 |
| 2 | Run your AI coding agent | natively | from the Ubuntu shell: [windows-podman-setup.md](windows-podman-setup.md) section 14 |
| 3 | `.env` | `scripts/create_env.py`, then `set -a; source .env; set +a` | same |
| 4 | Network and database | [database-23ai.md](database-23ai.md) | same |
| 5 | Installers: download, stage, response files | [installing-forms-14c.md](installing-forms-14c.md) steps 2 and 3 | same; on WSL `chmod +x` the `.bin` |
| 6 | Where the Oracle Home goes (decide BEFORE installing) | default (the repo folder) | **Ubuntu's disk, never the repo folder:** set `OFW_ORACLE_HOME=/home/<you>/oracle_home_14c` in `.env` and create it first: [installing-forms-14c.md](installing-forms-14c.md), [oracle-home-location.md](oracle-home-location.md) |
| 7 | Build the image and install | [installing-forms-14c.md](installing-forms-14c.md) steps 4 to 6 | same, plus the Windows fixes in [windows-podman-setup.md](windows-podman-setup.md) section 10 |
| 8 | Create the domain and apply the configuration | [installing-forms-14c.md](installing-forms-14c.md) step 7 (`create_forms_domain.py`, `apply_forms14c_config.sh`) | same |
| 9 | Start the Forms server and check it | [installing-forms-14c.md](installing-forms-14c.md) step 8 (15 to 45 minutes on Windows on the Windows drive; about 2 minutes on a Mac) | same |
| 10 | Rust workbench | [rust-workbench-setup.md](rust-workbench-setup.md) | inside Ubuntu, `CARGO_TARGET_DIR=$HOME/ofw-target` |
| 11 | Confirm everything | `python3 start_testbed.py` (or `--status`) | same |
| 12 | Test forms and conversions | [forms-and-reports-testing-guide.md](forms-and-reports-testing-guide.md); `scripts/health_check.py`; converting skill `test_conversions.py` | same |
| 13 | Optional: the Forms Builder GUI | [forms-builder-gui.md](forms-builder-gui.md) (XQuartz on macOS; WSLg, nothing to install, on Windows) | same |

After a reboot or `wsl --shutdown`, only `python3 start_testbed.py` is needed (it recreates the Windows runtime directory and prompts for the WSL password). Problems: [troubleshooting.md](troubleshooting.md).

Not verified: the whole list has never been run end to end on a clean machine. The commands are from the setups that were run (macOS in November 2026; Windows in WSL2 on 2026-11-13 and 14), and `apply_forms14c_config.sh` and `provision_oracle_home.sh` have not been run on a clean Oracle Home.
