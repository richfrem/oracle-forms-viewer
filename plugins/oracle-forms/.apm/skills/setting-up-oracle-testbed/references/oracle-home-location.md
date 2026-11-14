# Where the Oracle Home lives: why it should be on Ubuntu's disk, how to move it, how to prove the copy

**Status (2026-11-14): designed and scripted, not yet run.** `oracle-reference-stack/provision_oracle_home.sh` was written and syntax-checked, but no copy, verify, or backup has run yet. Nothing below is measured until a run has been recorded in the change log.

## On Windows: install there from the start

Do not install into `oracle-reference-stack/oracle_home_14c` (on the slow Windows drive) and move it later. Set `OFW_ORACLE_HOME=/home/<you>/oracle_home_14c` in `.env` **before** the installs, create the folder (`mkdir -p "$OH" && chmod 777 "$OH"`), and follow [installing-forms-14c.md](installing-forms-14c.md); every command there uses `$OH`. The installers can stay on the Windows drive. The sections below are for an Oracle Home that already exists in the repo folder.

## Contents

- [Why](#why)
- [The setting](#the-setting)
- [The protocol](#the-protocol)
- [Repairing a copy made with Windows Explorer](#repairing-a-copy-made-with-windows-explorer)
- [How to test that it was copied correctly](#how-to-test-that-it-was-copied-correctly)
- [Risks and unknowns](#risks-and-unknowns)

## Why

The Oracle Home (`oracle-reference-stack/oracle_home_14c/`) is the installed Forms and Reports 14c software, the WebLogic domain, and every adjustment that took hours to get right (domain hooks, libraries, `tnsnames.ora`). It is 5.4 GB. Containers mount it as `/oracle_home_14c`.

By default it sits inside the repo, and on Windows the repo is on the Windows drive, seen from Ubuntu as `/mnt/c` (a network-style 9p mount). That mount is slow for the huge number of small file reads that WebLogic makes while it boots. On this machine the Forms server took **15, 18, and 44 minutes** to reach RUNNING, against about 2 minutes on a Mac. An earlier thread dump showed one thread doing file-existence checks for most of that time. The same effect is why the Rust build output already lives on Ubuntu's disk (`~/ofw-target`). Putting the Oracle Home on Ubuntu's own disk (ext4) removes that "Windows tax". **The speed-up is expected, not yet measured here.**

What is in git is the **procedure**, not the data: the 5.4 GB cannot be committed. So the Oracle Home can always be rebuilt or moved by one script instead of by memory:

| In the repo | Not in git |
|---|---|
| `oracle-reference-stack/provision_oracle_home.sh` | the Oracle Home itself |
| `oracle-reference-stack/apply_forms14c_config.sh` and `forms14c-config/` | the installers (`oracle-reference-stack/installer/`, git-ignored) |
| the response files in `installer-templates/` | backup archives |

**Updating or recreating containers and images never touches the Oracle Home.** It is a host folder mounted in, not part of any image or container. What can lose it is deleting or resetting the Ubuntu distro (`wsl --unregister`) or disk failure, which is why there is a backup step and a rebuild path.

## The setting

`OFW_ORACLE_HOME` (in the git-ignored `.env`, or the environment) is the host folder of the Oracle Home. Empty means the default, `oracle-reference-stack/oracle_home_14c`, so macOS does not change. Use an absolute path, for example `/home/<you>/oracle_home_14c`. It is read by `provision_oracle_home.sh`, `apply_forms14c_config.sh`, `converter_runtime.py` (so the converters and `test_conversions.py` follow it), and `compose.yaml`. Not yet updated: `testing-forms-parity/scripts/test_parity.py`, which still has the repo path in two places.

## The protocol

Run everything from the repo root in the Ubuntu shell, after `set -a; source .env; set +a`.

1. **Dry run.** Prints the plan and changes nothing:
   ```bash
   bash oracle-reference-stack/provision_oracle_home.sh --dry-run --from-copy oracle-reference-stack/oracle_home_14c --target ~/oracle_home_14c
   ```
2. **Copy.** A throwaway container runs as root (which maps to your own user, so it can read files owned by the container's mapped user IDs) and streams `tar` from the source, mounted read-only, to the target. It refuses a target that is not empty and checks free space first:
   ```bash
   bash oracle-reference-stack/provision_oracle_home.sh --from-copy oracle-reference-stack/oracle_home_14c --target ~/oracle_home_14c
   ```
   The copy is never a move; the original stays until you delete it. A plain `cp` would lose the ownership the containers need.
3. **Verify.** The copy step runs this automatically; run it again at any time:
   ```bash
   bash oracle-reference-stack/provision_oracle_home.sh --verify --target ~/oracle_home_14c --against oracle-reference-stack/oracle_home_14c
   ```
4. **Switch the stack to it.** Set `OFW_ORACLE_HOME=/home/<you>/oracle_home_14c` in `.env`, then recreate `forms-server-14c` with that folder as the mount source. **This needs a new container but not a new image:** a container's mounts are fixed when it is created. The script does not do this; it prints the step. The container's current settings are saved in `temp/image-backup/forms-server-14c.inspect.json` (command, mounts, network `oracle-net`, ports 7001 and 9001, environment). `oracle23ai-lite` is unaffected. Removing or recreating a container is the owner's decision.
5. **Back up:**
   ```bash
   bash oracle-reference-stack/provision_oracle_home.sh --backup temp/backups --target ~/oracle_home_14c
   ```
   `temp/` is git-ignored and on the Windows drive, so the archive survives losing the Ubuntu distro.
6. **Rebuild from scratch** (if both copies are lost): `provision_oracle_home.sh --from-installers --target PATH` runs the two silent installs, creates the domain, and applies the configuration. **Unverified on the Ubuntu disk.**

## Repairing a copy made with Windows Explorer

Copying the Oracle Home with Windows Explorer (or any Windows tool) loses what Linux needs: the **execute permission** on the programs (`frmcmp_batch`, `frmweb`, `rwconverter`, `startWebLogic.sh` arrive as `-rw-r--r--`), the **404 symbolic links** (the copy has none), and the owner the container expects. It may also leave files missing. Observed on 2026-11-14 after a hand copy. **Do not use Explorer for this.** If it was done anyway, repair it in WSL with the original as the reference:

```bash
bash oracle-reference-stack/restore_oracle_home_attributes.sh --dry-run --from oracle-reference-stack/oracle_home_14c --target ~/oracle_home_14c   # shows what would change
bash oracle-reference-stack/restore_oracle_home_attributes.sh            --from oracle-reference-stack/oracle_home_14c --target ~/oracle_home_14c
bash oracle-reference-stack/provision_oracle_home.sh --verify --target ~/oracle_home_14c --against oracle-reference-stack/oracle_home_14c
```

It runs `rsync` inside `podman unshare` (adds missing files, restores permissions and symbolic links; same-size files are not re-compared), then sets the owner to the container's `oracle` user (1000:1000). It never changes the original or deletes anything in the copy. **Status: written, not yet run.**

## How to test that it was copied correctly

`--verify` reports these, in order. Read them like this:

| Line | Meaning | Pass when |
|---|---|---|
| `OK <file>` / `MISSING <file>` | Ten required files exist (`frmcmp_batch`, `frmweb`, `rwconverter`, `frmsal.jar`, `frmxmltools.jar`, `libfrmjapi.so.0`, `startWebLogic.sh`, `setUserOverrides.sh`, `boot.properties`, `tnsnames.ora`) | all OK |
| `frmweb has no missing libraries` | `ldd` on `frmweb` shows nothing `not found` | OK |
| `target : files=N links=N bytes=N tree=HEX` | A fingerprint of the copy: counts, total bytes, and a hash of the sorted (path, type, size) list. Independent of owners and timestamps | informational |
| `source : files=... tree=...` | The same for the source (with `--against`) | informational |
| `tree fingerprints are IDENTICAL` | Source and copy have the same files, symlinks, and sizes | IDENTICAL |
| `target : walked N files in Ns` and `source : walked N files in Ns` | A timed walk of each tree. This is a **direct measure of the disk difference** that slows WebLogic | the target is much faster |
| `VERIFIED.` | Every check above passed | VERIFIED |

What it does **not** prove is that WebLogic boots from the copy. That final test is to recreate `forms-server-14c` on the new folder, run `python3 start_testbed.py`, and read the "Forms ... took" time in its summary. Compare it with the 15 to 44 minutes recorded before, and write the numbers in the change log.

Extra checks after the stack runs from the new folder: `python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/test_conversions.py` (converters use `OFW_ORACLE_HOME` automatically), and `python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/health_check.py`.

## Risks and unknowns

- **Not yet run.** The copy, fingerprint, and backup commands use only standard tools in the image (`tar`, `find`, `sha256sum`), but the first run is the test. Record what it reports.
- **Ownership.** Files created by the containers are owned by Podman's mapped user IDs, so the host user cannot read them directly (`podman unshare` is needed). On ext4 that stays true. The copy preserves it.
- **Host paths inside the domain.** The WebLogic domain should contain only in-container paths (`/oracle_home_14c/...`), so moving the folder should not break it. **Not checked.**
- **Same filesystem rules.** Ubuntu's disk is ext4, so symlinks and execute bits work without the `metadata` mount option that `/mnt/c` needs.
- **The repo itself is also on `/mnt/c`.** Git and file reads pay the same cost. Moving it is a bigger change and is not part of this.
