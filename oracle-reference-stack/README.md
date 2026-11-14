# Oracle reference stack (Podman)

The Podman recipe, installer templates, configuration and provisioning scripts, and seed SQL for the optional Oracle reference environment: **Oracle Forms 14c with WebLogic** and **Oracle Database Free**, in containers, on macOS and on Windows 11 (inside Ubuntu WSL2, x86-64). The Rust workbench does not need any of it; Preview in Oracle, the converters, and the FSAL runner do.

**How to set it up, run it, and fix it lives once, in the setup skill's references** (not here):

- Start here, from a fresh clone: [`plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/clean-clone-checklist.md`](../plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/clean-clone-checklist.md)
- Every page, grouped: [`.../references/index.md`](../plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/index.md)
- What each file in this folder is for (including `compose.yaml` and its known problems): [`.../references/oracle-reference-stack-map.md`](../plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/oracle-reference-stack-map.md)

**Windows:** everything (Podman, the containers, Python, Java 21, the Rust app, your coding agent) runs inside the Ubuntu WSL2 distro; no Visual Studio. Credentials come from the git-ignored `.env` (copy `example.env`; the setup skill creates it). `python3 start_testbed.py` (repo root) starts and confirms the containers and the Rust app.

## What is in this folder

| Entry | What it is |
|---|---|
| `Containerfile.14c`, `Containerfile` | Image recipes: Forms 14c (`forms-converter-14c`) and the legacy 12c image. No Oracle software inside either |
| `installer-templates/` | The committed response files for the silent installs |
| `installer/`, `oracle_home_14c/`, `oracle_home/` | **Git-ignored, never commit:** the downloaded installers, the installed Oracle Home (5+ GB, holds the WebLogic domain and all configuration; its location is the setting `OFW_ORACLE_HOME`; on Windows set it to Ubuntu's disk before installing and never install here), and the 12c Oracle Home |
| `apply_forms14c_config.sh`, `forms14c-config/` | Apply every post-install adjustment to the Oracle Home |
| `provision_oracle_home.sh` | Create, copy, verify, and back up the Oracle Home on a chosen disk (`--dry-run` first) |
| `sql/seed_test_schema.sql` | Test schema owner, tablespace, and tables |
| `compose.yaml` | Declarative description of the containers and install steps (the original macOS route; not used on Windows) |

The skills that run against this stack are in `plugins/oracle-forms/.apm/skills/`: `setting-up-oracle-testbed`, `converting-oracle-artifacts`, `compiling-and-running-oracle-forms`, `testing-forms-parity`, and `comparing-forms-features`.
