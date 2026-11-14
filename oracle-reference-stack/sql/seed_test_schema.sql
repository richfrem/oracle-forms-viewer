-- Seed script for the Oracle Forms testbed (Oracle Database Free, PDB FREEPDB1).
-- Run as SYSTEM. Local-development defaults only: change the passwords for anything beyond a throwaway container.
-- Usage (from the host, Ubuntu WSL or macOS):
--   (printf 'DEFINE db_user=%s\nDEFINE db_password=%s\n' "$DB_USER" "$DB_PASSWORD"; cat sql/seed_test_schema.sql) | podman exec -i oracle23ai-lite sqlplus -s system/$ORACLE_PWD@localhost/FREEPDB1

SET ECHO ON
SET FEEDBACK ON
WHENEVER SQLERROR CONTINUE

-- Schema owner and password are substitution variables (&db_user, &db_password), set from .env by the command above.
-- Recreate the user from scratch (ignore "user does not exist" on the first run)
DROP USER &db_user CASCADE;

WHENEVER SQLERROR EXIT SQL.SQLCODE

-- FREEPDB1 has no USERS tablespace; create a dedicated one for the test schema (ignore "already exists" on rerun)
WHENEVER SQLERROR CONTINUE
CREATE TABLESPACE forms_data DATAFILE '/opt/oracle/oradata/FREE/FREEPDB1/forms_data01.dbf' SIZE 100M AUTOEXTEND ON NEXT 25M;
WHENEVER SQLERROR EXIT SQL.SQLCODE

CREATE USER &db_user IDENTIFIED BY "&db_password" DEFAULT TABLESPACE forms_data QUOTA UNLIMITED ON forms_data;
GRANT CREATE SESSION, CREATE TABLE, CREATE SEQUENCE, CREATE VIEW TO &db_user;

-- Tables are created inside the schema owner's schema
CREATE TABLE &db_user..departments (
  dept_id    NUMBER(4)     CONSTRAINT departments_pk PRIMARY KEY,
  dept_name  VARCHAR2(60)  NOT NULL,
  location   VARCHAR2(60)
);

CREATE TABLE &db_user..employees (
  emp_id     NUMBER(6)     CONSTRAINT employees_pk PRIMARY KEY,
  first_name VARCHAR2(40)  NOT NULL,
  last_name  VARCHAR2(40)  NOT NULL,
  email      VARCHAR2(80)  UNIQUE,
  dept_id    NUMBER(4)     CONSTRAINT employees_dept_fk REFERENCES &db_user..departments (dept_id),
  hire_date  DATE          DEFAULT SYSDATE,
  salary     NUMBER(10,2)
);

CREATE TABLE &db_user..test_items (
  item_id    NUMBER(6)     CONSTRAINT test_items_pk PRIMARY KEY,
  item_name  VARCHAR2(60)  NOT NULL,
  qty        NUMBER(6)     DEFAULT 0,
  price      NUMBER(10,2)
);

-- Sample records
INSERT INTO &db_user..departments VALUES (10, 'Engineering', 'Building A');
INSERT INTO &db_user..departments VALUES (20, 'Sales', 'Building B');
INSERT INTO &db_user..departments VALUES (30, 'Support', 'Remote');

INSERT INTO &db_user..employees VALUES (1001, 'Ada',   'Lovelace', 'ada@example.test',   10, DATE '2024-01-15', 125000);
INSERT INTO &db_user..employees VALUES (1002, 'Grace', 'Hopper',   'grace@example.test', 10, DATE '2023-06-01', 118000);
INSERT INTO &db_user..employees VALUES (1003, 'Linus', 'Torvalds', 'linus@example.test', 20, DATE '2025-02-10',  96000);
INSERT INTO &db_user..employees VALUES (1004, 'Mary',  'Shelley',  'mary@example.test',  30, DATE '2022-09-20',  72000);
INSERT INTO &db_user..employees VALUES (1005, 'Alan',  'Turing',   'alan@example.test', 10, DATE '2025-07-01', 131000);
INSERT INTO &db_user..employees VALUES (1006, 'Edsger','Dijkstra', 'edsger@example.test', 20, DATE '2024-11-11', 104000);

INSERT INTO &db_user..test_items VALUES (1, 'Widget',    25, 9.99);
INSERT INTO &db_user..test_items VALUES (2, 'Gadget',     5, 49.50);
INSERT INTO &db_user..test_items VALUES (3, 'Gizmo',     12, 19.25);
INSERT INTO &db_user..test_items VALUES (4, 'Doohickey',  0, 3.75);
INSERT INTO &db_user..test_items VALUES (5, 'Thingamajig', 40, 2.10);

COMMIT;

-- Queries
SET LINESIZE 200
SET PAGESIZE 100
COLUMN dept_name FORMAT A16
COLUMN full_name FORMAT A22
COLUMN item_name FORMAT A14

SELECT 'departments' AS table_name, COUNT(*) AS row_count FROM &db_user..departments
UNION ALL SELECT 'employees', COUNT(*) FROM &db_user..employees
UNION ALL SELECT 'test_items', COUNT(*) FROM &db_user..test_items;

SELECT e.emp_id, e.first_name || ' ' || e.last_name AS full_name, d.dept_name, e.salary
FROM &db_user..employees e
JOIN &db_user..departments d ON d.dept_id = e.dept_id
ORDER BY e.salary DESC;

SELECT d.dept_name, COUNT(e.emp_id) AS headcount, ROUND(AVG(e.salary), 2) AS avg_salary
FROM &db_user..departments d
LEFT JOIN &db_user..employees e ON e.dept_id = d.dept_id
GROUP BY d.dept_name
ORDER BY d.dept_name;

SELECT item_id, item_name, qty, price, qty * price AS stock_value
FROM &db_user..test_items
WHERE qty > 0
ORDER BY stock_value DESC;

EXIT
