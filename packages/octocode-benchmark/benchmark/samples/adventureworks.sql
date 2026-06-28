-- AdventureWorks sample schema (simplified) — public domain reference schema
-- Source: Microsoft SQL Server AdventureWorks, adapted for PostgreSQL

CREATE SCHEMA production;
CREATE SCHEMA sales;
CREATE SCHEMA person;
CREATE SCHEMA hr;

CREATE TABLE person.address (
    address_id     SERIAL PRIMARY KEY,
    address_line1  VARCHAR(60) NOT NULL,
    address_line2  VARCHAR(60),
    city           VARCHAR(30) NOT NULL,
    state_province VARCHAR(50) NOT NULL,
    postal_code    VARCHAR(15) NOT NULL,
    modified_date  TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE production.product (
    product_id         SERIAL PRIMARY KEY,
    name               VARCHAR(50) NOT NULL,
    product_number     VARCHAR(25) NOT NULL UNIQUE,
    color              VARCHAR(15),
    standard_cost      NUMERIC(19,4) NOT NULL,
    list_price         NUMERIC(19,4) NOT NULL,
    size               VARCHAR(5),
    weight             NUMERIC(8,2),
    product_category   INTEGER,
    sell_start_date    DATE NOT NULL,
    sell_end_date      DATE,
    discontinued_date  TIMESTAMP,
    modified_date      TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE sales.customer (
    customer_id     SERIAL PRIMARY KEY,
    person_id       INTEGER,
    store_id        INTEGER,
    territory_id    INTEGER,
    account_number  VARCHAR(10) NOT NULL UNIQUE,
    modified_date   TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE sales.sales_order_header (
    sales_order_id          SERIAL PRIMARY KEY,
    revision_number         SMALLINT NOT NULL DEFAULT 0,
    order_date              TIMESTAMP NOT NULL DEFAULT NOW(),
    due_date                TIMESTAMP NOT NULL,
    ship_date               TIMESTAMP,
    status                  SMALLINT NOT NULL DEFAULT 1,
    online_order_flag       BOOLEAN NOT NULL DEFAULT TRUE,
    sales_order_number      VARCHAR(25) NOT NULL,
    purchase_order_number   VARCHAR(25),
    account_number          VARCHAR(15),
    customer_id             INTEGER NOT NULL REFERENCES sales.customer(customer_id),
    sales_person_id         INTEGER,
    territory_id            INTEGER,
    bill_to_address_id      INTEGER NOT NULL REFERENCES person.address(address_id),
    ship_to_address_id      INTEGER NOT NULL REFERENCES person.address(address_id),
    ship_method_id          INTEGER NOT NULL,
    credit_card_id          INTEGER,
    sub_total               NUMERIC(19,4) NOT NULL DEFAULT 0.00,
    tax_amt                 NUMERIC(19,4) NOT NULL DEFAULT 0.00,
    freight                 NUMERIC(19,4) NOT NULL DEFAULT 0.00,
    total_due               NUMERIC(19,4),
    comment                 VARCHAR(128),
    modified_date           TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE sales.sales_order_detail (
    sales_order_id          INTEGER NOT NULL REFERENCES sales.sales_order_header(sales_order_id),
    sales_order_detail_id   SERIAL,
    carrier_tracking_number VARCHAR(25),
    order_qty               SMALLINT NOT NULL,
    product_id              INTEGER NOT NULL REFERENCES production.product(product_id),
    special_offer_id        INTEGER NOT NULL,
    unit_price              NUMERIC(19,4) NOT NULL,
    unit_price_discount     NUMERIC(19,4) NOT NULL DEFAULT 0.0,
    line_total              NUMERIC(38,6),
    modified_date           TIMESTAMP NOT NULL DEFAULT NOW(),
    PRIMARY KEY (sales_order_id, sales_order_detail_id)
);

CREATE INDEX ix_address_state ON person.address (state_province);
CREATE INDEX ix_product_name ON production.product (name);
CREATE INDEX ix_product_number ON production.product (product_number);
CREATE INDEX ix_customer_territory ON sales.customer (territory_id);
CREATE INDEX ix_order_customer ON sales.sales_order_header (customer_id);
CREATE INDEX ix_order_date ON sales.sales_order_header (order_date);
CREATE INDEX ix_order_detail_product ON sales.sales_order_detail (product_id);

CREATE VIEW sales.v_sales_order_detail AS
    SELECT
        soh.sales_order_id,
        soh.sales_order_number,
        soh.order_date,
        soh.status,
        sod.sales_order_detail_id,
        sod.order_qty,
        sod.unit_price,
        sod.line_total,
        p.name AS product_name,
        p.product_number,
        p.color,
        p.list_price
    FROM sales.sales_order_header soh
    JOIN sales.sales_order_detail sod ON soh.sales_order_id = sod.sales_order_id
    JOIN production.product p ON sod.product_id = p.product_id;

CREATE FUNCTION sales.fn_get_order_total(p_order_id INTEGER)
RETURNS NUMERIC AS $$
DECLARE
    v_total NUMERIC;
BEGIN
    SELECT SUM(line_total)
    INTO v_total
    FROM sales.sales_order_detail
    WHERE sales_order_id = p_order_id;
    RETURN COALESCE(v_total, 0);
END;
$$ LANGUAGE plpgsql;

CREATE PROCEDURE sales.usp_insert_order(
    IN p_customer_id    INTEGER,
    IN p_ship_address   INTEGER,
    IN p_bill_address   INTEGER,
    IN p_ship_method    INTEGER,
    OUT p_order_id      INTEGER
)
LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO sales.sales_order_header (
        customer_id, bill_to_address_id, ship_to_address_id,
        ship_method_id, due_date
    ) VALUES (
        p_customer_id, p_bill_address, p_ship_address,
        p_ship_method, NOW() + INTERVAL '7 days'
    ) RETURNING sales_order_id INTO p_order_id;
END;
$$;

SELECT
    c.customer_id,
    c.account_number,
    COUNT(soh.sales_order_id)  AS order_count,
    SUM(soh.total_due)         AS total_spent
FROM sales.customer c
LEFT JOIN sales.sales_order_header soh ON c.customer_id = soh.customer_id
WHERE soh.order_date >= '2023-01-01'
  AND soh.status = 5
GROUP BY c.customer_id, c.account_number
HAVING SUM(soh.total_due) > 1000
ORDER BY total_spent DESC
LIMIT 100;
