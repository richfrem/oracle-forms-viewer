-- Demo schema for samples/ofw_sales_fmb.xml ("Customer Orders Inquiry").
-- Customers -> Orders -> Order lines, plus a product catalogue.
-- Run as DB_USER from .env against FREEPDB1, e.g.:
--   podman exec -i oracle23ai-lite sqlplus -s "$DB_USER/$DB_PASSWORD"@localhost:1521/FREEPDB1 < samples/ofw_demo_sales.sql
-- Safe to re-run: drops and recreates the four OFW_ tables.

WHENEVER SQLERROR CONTINUE
DROP TABLE ofw_order_lines PURGE;
DROP TABLE ofw_orders PURGE;
DROP TABLE ofw_products PURGE;
DROP TABLE ofw_customers PURGE;
WHENEVER SQLERROR EXIT FAILURE

CREATE TABLE ofw_customers (
  customer_id    NUMBER(6)     PRIMARY KEY,
  customer_name  VARCHAR2(60)  NOT NULL,
  contact_name   VARCHAR2(60),
  email          VARCHAR2(80),
  phone          VARCHAR2(30),
  city           VARCHAR2(40),
  region         VARCHAR2(20)  NOT NULL,
  credit_limit   NUMBER(10,2)  NOT NULL,
  customer_since DATE          NOT NULL
);

CREATE TABLE ofw_products (
  product_id   NUMBER(6)    PRIMARY KEY,
  product_name VARCHAR2(60) NOT NULL,
  category     VARCHAR2(30) NOT NULL,
  list_price   NUMBER(10,2) NOT NULL
);

CREATE TABLE ofw_orders (
  order_id    NUMBER(8)    PRIMARY KEY,
  customer_id NUMBER(6)    NOT NULL REFERENCES ofw_customers,
  order_date  DATE         NOT NULL,
  status      VARCHAR2(12) NOT NULL CHECK (status IN ('NEW','SHIPPED','DELIVERED','CANCELLED')),
  sales_rep   VARCHAR2(30)
);

CREATE TABLE ofw_order_lines (
  order_id   NUMBER(8)    NOT NULL REFERENCES ofw_orders,
  line_no    NUMBER(3)    NOT NULL,
  product_id NUMBER(6)    NOT NULL REFERENCES ofw_products,
  quantity   NUMBER(6)    NOT NULL,
  unit_price NUMBER(10,2) NOT NULL,
  CONSTRAINT ofw_order_lines_pk PRIMARY KEY (order_id, line_no)
);

CREATE INDEX ofw_orders_cust_ix ON ofw_orders (customer_id);

DECLARE
  TYPE t_list IS TABLE OF VARCHAR2(60);
  names t_list := t_list(
    'Acme Industrial','Blue Harbor Foods','Cascade Outfitters','Delta Precision Tools','Evergreen Clinics',
    'Fjord Logistics','Granite Peak Builders','Harborview Hotels','Ironwood Furniture','Juniper Software',
    'Keystone Analytics','Lakeside Bakery','Meridian Marine','Northwind Traders','Orchard Valley Farms',
    'Pacific Rim Imports','Quantum Labs','Redwood Printing','Summit Health','Tidewater Energy',
    'Union Square Books','Vantage Media','Westbrook Schools','Yellowknife Mining','Zephyr Aviation',
    'Alpine Dairy','Bayside Motors','Cedar Creek Wines','Driftwood Studios','Elm Street Pharmacy',
    'Falcon Security','Golden Gate Textiles','Highland Coffee','Island Freight','Jasper Outdoor',
    'Kodiak Steel','Lighthouse Insurance','Maple Leaf Hardware','Nimbus Cloud','Oakridge Dental');
  contacts t_list := t_list('Ava Chen','Liam Patel','Sofia Garcia','Noah Kim','Mia Novak','Ethan Brooks',
    'Zara Ali','Lucas Moreau','Chloe Singh','Owen Walsh','Ivy Tanaka','Leo Rossi');
  cities t_list := t_list('Vancouver','Seattle','Portland','Calgary','Toronto','Boston','Denver','Victoria');
  regions t_list := t_list('WEST','WEST','WEST','PRAIRIES','EAST','EAST','MOUNTAIN','WEST');
  products t_list := t_list('Steel Bolts (box 100)','Hydraulic Pump','Safety Gloves','Laptop Stand','LED Panel 60W',
    'Office Chair','Thermal Printer','Packing Tape (case)','Forklift Battery','Wireless Scanner',
    'Coffee Beans 5kg','First Aid Kit','Workbench 2m','Ethernet Switch 24p','Paper A4 (pallet)',
    'Solar Charger','Pallet Jack','Label Maker','Ladder 3m','Desk Lamp');
  categories t_list := t_list('Hardware','Machinery','Safety','Office','Electrical','Office','Electronics',
    'Supplies','Machinery','Electronics','Food','Safety','Furniture','Electronics','Supplies',
    'Electrical','Machinery','Office','Hardware','Office');
  prices t_list := t_list('24.50','1850.00','12.75','89.99','64.00','329.00','245.50','48.00','2400.00','199.00',
    '79.50','35.25','699.00','410.00','899.00','129.99','560.00','54.95','189.00','42.50');
  reps t_list := t_list('J. Ortega','M. Lindqvist','R. Okafor','S. Haddad');
  statuses t_list := t_list('NEW','SHIPPED','DELIVERED','DELIVERED','DELIVERED','CANCELLED');
  v_order NUMBER := 50000;
  v_lines NUMBER;
  v_pid   NUMBER;
  v_name VARCHAR2(60); v_contact VARCHAR2(60); v_city VARCHAR2(40); v_region VARCHAR2(20);
  v_status VARCHAR2(12); v_rep VARCHAR2(30); v_price NUMBER;
BEGIN
  FOR p IN 1 .. products.COUNT LOOP
    INSERT INTO ofw_products VALUES (p, products(p), categories(p), TO_NUMBER(prices(p), '99999.99'));
  END LOOP;

  FOR c IN 1 .. names.COUNT LOOP
    v_name    := names(c);
    v_contact := contacts(MOD(c * 5, contacts.COUNT) + 1);
    v_city    := cities(MOD(c * 3, cities.COUNT) + 1);
    v_region  := regions(MOD(c * 3, cities.COUNT) + 1);
    INSERT INTO ofw_customers VALUES (
      1000 + c, v_name, v_contact,
      LOWER(REPLACE(v_contact, ' ', '.')) || '@' || LOWER(REPLACE(REPLACE(v_name, ' ', ''), '''', '')) || '.example',
      '+1-555-' || TO_CHAR(1000 + MOD(c * 379, 9000)),
      v_city, v_region,
      5000 * (1 + MOD(c * 7, 12)),
      DATE '2012-01-01' + MOD(c * 211, 4300));

    -- 0 to 7 orders per customer (a few customers have none).
    FOR o IN 1 .. MOD(c * 13, 8) LOOP
      v_order  := v_order + 1;
      v_status := statuses(MOD(c + o * 5, statuses.COUNT) + 1);
      v_rep    := reps(MOD(c + o, reps.COUNT) + 1);
      INSERT INTO ofw_orders VALUES (v_order, 1000 + c, DATE '2023-01-01' + MOD(c * 37 + o * 53, 680), v_status, v_rep);
      v_lines := 1 + MOD(c * 3 + o * 7, 5);
      FOR l IN 1 .. v_lines LOOP
        v_pid   := MOD(c * 7 + o * 11 + l * 3, products.COUNT) + 1;
        v_price := TO_NUMBER(prices(v_pid), '99999.99');
        INSERT INTO ofw_order_lines VALUES (v_order, l, v_pid, 1 + MOD(c + o * l, 12), v_price);
      END LOOP;
    END LOOP;
  END LOOP;
  COMMIT;
END;
/

SELECT (SELECT COUNT(*) FROM ofw_customers)   AS customers,
       (SELECT COUNT(*) FROM ofw_products)    AS products,
       (SELECT COUNT(*) FROM ofw_orders)      AS orders,
       (SELECT COUNT(*) FROM ofw_order_lines) AS order_lines
  FROM dual;
