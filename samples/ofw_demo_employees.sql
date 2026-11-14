-- Demo table for samples/ofw_demo_employees_fmb.xml (Oracle Forms Workbench).
-- Run as the DB_USER from .env, e.g. inside the reference stack:
--   podman exec -i oracle23ai-lite sqlplus $DB_USER/$DB_PASSWORD@localhost:1521/FREEPDB1 @- < samples/ofw_demo_employees.sql
CREATE TABLE ofw_demo_employees (
  emp_id      NUMBER(6)     PRIMARY KEY,
  first_name  VARCHAR2(30)  NOT NULL,
  last_name   VARCHAR2(30)  NOT NULL,
  job_title   VARCHAR2(40),
  department  VARCHAR2(30),
  salary      NUMBER(10,2),
  hire_date   DATE
);
INSERT INTO ofw_demo_employees VALUES (101, 'Ada',       'Lovelace', 'Principal Engineer',   'Engineering', 145000.00, DATE '2019-03-11');
INSERT INTO ofw_demo_employees VALUES (102, 'Grace',     'Hopper',   'Compiler Lead',        'Engineering', 152500.50, DATE '2017-07-01');
INSERT INTO ofw_demo_employees VALUES (103, 'Alan',      'Turing',   'Research Scientist',   'Research',    138000.00, DATE '2020-01-20');
INSERT INTO ofw_demo_employees VALUES (104, 'Katherine', 'Johnson',  'Flight Analyst',       'Research',    121750.25, DATE '2018-09-04');
INSERT INTO ofw_demo_employees VALUES (105, 'Edsger',    'Dijkstra', 'Architect',            'Engineering', 149900.00, DATE '2016-05-16');
INSERT INTO ofw_demo_employees VALUES (106, 'Barbara',   'Liskov',   'Distinguished Fellow', 'Research',    161000.00, DATE '2015-11-30');
INSERT INTO ofw_demo_employees VALUES (107, 'Margaret',  'Hamilton', 'Software Director',    'Operations',  158250.00, DATE '2014-02-03');
INSERT INTO ofw_demo_employees VALUES (108, 'Dennis',    'Ritchie',  'Systems Engineer',     'Operations',  133400.75, DATE '2021-08-23');
COMMIT;
