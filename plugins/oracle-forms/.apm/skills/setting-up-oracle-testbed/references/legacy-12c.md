# Legacy: Oracle Forms 12c Standalone (optional, macOS-era)

The original container used Oracle Forms 12.2.1.4 "Standalone Forms Builder". It is **optional and superseded** by the 14c stack ([installing-forms-14c.md](installing-forms-14c.md)). The converter scripts still support it: set `OFW_CONVERTER_IMAGE=forms-converter:latest` (the Oracle Home path inside the container is then `/oracle_home`, from the host folder `oracle-reference-stack/oracle_home/`). It was set up on macOS only and was **not re-run on Windows**.

| | 12c Standalone | 14c (current) |
|---|---|---|
| Image / Containerfile | `forms-converter:latest` / `oracle-reference-stack/Containerfile` | `forms-converter-14c:latest` / `Containerfile.14c` |
| Java | OpenJDK 8 (`java-1.8.0-openjdk-headless`) | Oracle JDK 17 (the installer rejects OpenJDK) |
| Installer | `V983392-01_1of2.zip` (2.4 GB) and `V983392-01_2of2.zip` (630 MB), Oracle Forms 12.2.1.4.0, Linux x86-64 | `V1045135-01.zip` and `V1045121-01.zip` |
| Response file | `forms_silent.rsp`: `ORACLE_HOME=/oracle_home`, `INSTALL_TYPE=Standalone Forms Builder` | two files in `installer-templates/` |
| What it contains | Forms Builder and the batch converters; **no Reports, no WebLogic domain** | Forms and Reports, WebLogic, the Forms servlet |
| Module version written | `122010400` (12.2.1.4.0) | `141020000` |

Facts worth keeping:
- The 12c `.bin` needs its companion archive `fmw_12.2.1.4.0_fr_linux64-2.zip` in the same folder as `fmw_12.2.1.4.0_fr_linux64.bin` during a silent install.
- The same installer rules apply as for 14c: run as the non-root `oracle` user, keep `inventory_loc` outside the Oracle Home (`INST-07319`), and pass `--platform linux/amd64`.
- 12c needs `libnsl` (`libnsl.so.1`) and `libXt`; the Forms Java bridge expects a `libfrmjapi.so -> libfrmjapi.so.0` symlink in `oracle_home/lib/` and `-Djava.library.path=/oracle_home/lib`.
- The Forms Builder GUI command for 12c is in `forms-builder-gui.md` section 4.
- Converting with 12c on macOS: `OFW_CONVERTER_IMAGE=forms-converter:latest python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/convert_fmb.py form.fmb`.
