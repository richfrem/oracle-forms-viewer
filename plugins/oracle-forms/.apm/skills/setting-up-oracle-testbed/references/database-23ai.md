# Oracle Database Free container (`oracle23ai-lite`)

The same on macOS and Windows (inside Ubuntu WSL2). Run from the repo root with `.env` loaded (`set -a; source .env; set +a`). Passwords come from `.env` (`ORACLE_PWD`, `DB_USER`, `DB_PASSWORD`, `DB_SERVICE`); they are local-development values for a throwaway container, never for anything else.

## Create and start it (once)

```bash
podman network create oracle-net            # skip if it exists
podman run -d --name oracle23ai-lite --network oracle-net --network-alias oracle-db-23ai \
  -p 1521:1521 -e ORACLE_PWD="$ORACLE_PWD" \
  container-registry.oracle.com/database/free:latest-lite
podman logs -f oracle23ai-lite              # wait for "DATABASE IS READY TO USE!"
```

- **Image:** `container-registry.oracle.com/database/free:latest-lite`. Oracle publishes it for both `arm64` (native on Apple Silicon, no emulation) and `amd64`. It is labelled "Oracle AI Database 26ai Free"; `product_component_version` reports `23.0.0.0.0`. The repo's SQL and connections work with it.
- **First start:** about 6 minutes on Windows (WSL2); under a minute on Apple Silicon. Later starts: `podman start oracle23ai-lite`, or `python3 start_testbed.py`.
- **Network alias `oracle-db-23ai`:** other containers (the Forms server) reach the database at `oracle-db-23ai:1521`. From the host use `localhost:1521`.
- **Data:** the database's files live in the container's own storage. Recreating the container loses them; re-run the seed script.

Windows: Podman must be the one inside Ubuntu, and `/run/user/<uid>` must exist ([windows-podman-setup.md](windows-podman-setup.md) section 3). Port 1521 already in use means an older Oracle listener holds it (`Get-NetTCPConnection -LocalPort 1521` in PowerShell).

## Seed the test schema

`oracle-reference-stack/sql/seed_test_schema.sql` creates the tablespace `FORMS_DATA` (the PDB has no `USERS` tablespace), the user `$DB_USER`, and tables `departments` (3 rows), `employees` (6 rows), and `test_items` (5 rows, with an `amount` column that the sample forms need). It takes the owner and password as variables:

```bash
(printf 'DEFINE db_user=%s\nDEFINE db_password=%s\n' "$DB_USER" "$DB_PASSWORD"; cat oracle-reference-stack/sql/seed_test_schema.sql) \
  | podman exec -i oracle23ai-lite sqlplus -s system/"$ORACLE_PWD"@localhost/FREEPDB1
```

The first-run `ORA-01918` from `DROP USER` is expected. Then the demo schemas for the sample forms:

```bash
podman exec -i oracle23ai-lite sqlplus -s "$DB_USER"/"$DB_PASSWORD"@localhost/FREEPDB1 < samples/ofw_demo_employees.sql
podman exec -i oracle23ai-lite sqlplus -s "$DB_USER"/"$DB_PASSWORD"@localhost/FREEPDB1 < samples/ofw_demo_sales.sql
```

`ORA-00942` lines from `ofw_demo_sales.sql`'s `DROP TABLE` statements are expected the first time. A form failing with `must be declared AMOUNT` means `test_items` lacks the `amount` column (re-run the seed).

## Check it

```bash
printf "SELECT USER FROM dual;\nSELECT COUNT(*) FROM test_items;\nEXIT\n" | podman exec -i oracle23ai-lite sqlplus -s "$DB_USER"/"$DB_PASSWORD"@localhost/FREEPDB1
python3 start_testbed.py --status           # shows the container and whether port 1521 is open
```

From PowerShell use `Test-NetConnection 127.0.0.1 -Port 1521` (`localhost` tries IPv6 `::1` first and can report a false failure).

## Connection strings

| From | Use |
|---|---|
| The host (Rust app, CLI, FSAL) | `localhost:1521/FREEPDB1` |
| Another container on `oracle-net` (the Forms server) | `oracle-db-23ai:1521/FREEPDB1`; the domain's `tnsnames.ora` alias `FREEPDB1` points there |

On macOS a container can also reach the host's published port through `host.containers.internal`. Do not use a container's numeric IP address: it changes between runs.
