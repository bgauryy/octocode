# SQL (.sql)

Source sample: `sql/00-postgres-select.sql`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 8415 | - | - |
| content-view | 5419 | 35.6% | 0.269 ms |
| applyMinification | 5447 | 35.3% | 0.227 ms |
| sync minify | 5447 | 35.3% | 0.225 ms |
| async minify | 5447 | 35.3% | 0.23 ms |
| symbols | 454 | 94.6% | 0.213 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```sql
--
-- SELECT
--

-- btree index
-- awk '{if($1<10){print;}else{next;}}' onek.data | sort +0n -1
--
SELECT * FROM onek
   WHERE onek.unique1 < 10
   ORDER BY onek.unique1;

--
-- awk '{if($1<20){print $1,$14;}else{next;}}' onek.data | sort +0nr -1
--
SELECT onek.unique1, onek.stringu1 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using >;

--
-- awk '{if($1>980){print $1,$14;}else{next;}}' onek.data | sort +1d -2
--
SELECT onek.unique1, onek.stringu1 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY stringu1 using <;

--
-- awk '{if($1>980){print $1,$16;}else{next;}}' onek.data |
-- sort +1d -2 +0nr -1
--
SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY string4 using <, unique1 using >;

--
-- awk '{if($1>980){print $1,$16;}else{next;}}' onek.data |
-- sort +1dr -2 +0n -1
--
SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY string4 using >, unique1 using <;

--
-- awk '{if($1<20){print $1,$16;}else{next;}}' onek.data |
-- sort +0nr -1 +1d -2
--
SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using >, string4 using <;

--
-- awk '{if($1<20){print $1,$16;}else{next;}}' onek.data |
-- sor

... [truncated 6615 chars] ...

it's effectively X IS NOT NULL assuming = is strict
-- (see bug #5084)
select * from (values (2),(null),(1)) v(k) where k = k order by k;
select * from (values (2),(null),(1)) v(k) where k = k;

-- Test partitioned tables with no partitions, which should be handled the
-- same as the non-inheritance case when expanding its RTE.
create table list_parted_tbl (a int,b int) partition by list (a);
create table list_parted_tbl1 partition of list_parted_tbl
  for values in (1) partition by list(b);
explain (costs off) select * from list_parted_tbl;
drop table list_parted_tbl;

```

## Content-View Excerpt

```sql
SELECT * FROM onek
   WHERE onek.unique1 < 10
   ORDER BY onek.unique1;

SELECT onek.unique1, onek.stringu1 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using >;

SELECT onek.unique1, onek.stringu1 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY stringu1 using <;

SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY string4 using <, unique1 using >;

SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY string4 using >, unique1 using <;

SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using >, string4 using <;

SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using <, string4 using >;

ANALYZE onek2;

SET enable_seqscan TO off;
SET enable_bitmapscan TO off;
SET enable_sort TO off;

SELECT onek2.* FROM onek2 WHERE onek2.unique1 < 10;

SELECT onek2.unique1, onek2.stringu1 FROM onek2
    WHERE onek2.unique1 < 20
    ORDER BY unique1 using >;

SELECT onek2.unique1, onek2.stringu1 FROM onek2
   WHERE onek2.unique1 > 980;

RESET enable_seqscan;
RESET enable_bitmapscan;
RESET enable_sort;

SELECT p.name, p.age FROM person* p;

SELECT p.name, p.age FROM pe

... [truncated 3619 chars] ...

AS x ORDER BY x;

create function sillysrf(int) returns setof int as
  'values (1),(10),(2),($1)' language sql immutable;

select sillysrf(42);
select sillysrf(-1) order by 1;

drop function sillysrf(int);

select * from (values (2),(null),(1)) v(k) where k = k order by k;
select * from (values (2),(null),(1)) v(k) where k = k;

create table list_parted_tbl (a int,b int) partition by list (a);
create table list_parted_tbl1 partition of list_parted_tbl
  for values in (1) partition by list(b);
explain (costs off) select * from list_parted_tbl;
drop table list_parted_tbl;
```

## Apply Minification Excerpt

```sql


SELECT * FROM onek
   WHERE onek.unique1 < 10
   ORDER BY onek.unique1;


SELECT onek.unique1, onek.stringu1 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using >;


SELECT onek.unique1, onek.stringu1 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY stringu1 using <;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY string4 using <, unique1 using >;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY string4 using >, unique1 using <;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using >, string4 using <;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using <, string4 using >;


ANALYZE onek2;

SET enable_seqscan TO off;
SET enable_bitmapscan TO off;
SET enable_sort TO off;


SELECT onek2.* FROM onek2 WHERE onek2.unique1 < 10;


SELECT onek2.unique1, onek2.stringu1 FROM onek2
    WHERE onek2.unique1 < 20
    ORDER BY unique1 using >;


SELECT onek2.unique1, onek2.stringu1 FROM onek2
   WHERE onek2.unique1 > 980;

RESET enable_seqscan;
RESET enable_bitmapscan;
RESET enable_sort;


SELECT p.name, p.age FROM person* p;


SELECT p.name,

... [truncated 3647 chars] ...

x ORDER BY x;


create function sillysrf(int) returns setof int as
  'values (1),(10),(2),($1)' language sql immutable;

select sillysrf(42);
select sillysrf(-1) order by 1;

drop function sillysrf(int);


select * from (values (2),(null),(1)) v(k) where k = k order by k;
select * from (values (2),(null),(1)) v(k) where k = k;


create table list_parted_tbl (a int,b int) partition by list (a);
create table list_parted_tbl1 partition of list_parted_tbl
  for values in (1) partition by list(b);
explain (costs off) select * from list_parted_tbl;
drop table list_parted_tbl;
```

## Sync Minify Excerpt

```sql


SELECT * FROM onek
   WHERE onek.unique1 < 10
   ORDER BY onek.unique1;


SELECT onek.unique1, onek.stringu1 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using >;


SELECT onek.unique1, onek.stringu1 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY stringu1 using <;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY string4 using <, unique1 using >;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY string4 using >, unique1 using <;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using >, string4 using <;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using <, string4 using >;


ANALYZE onek2;

SET enable_seqscan TO off;
SET enable_bitmapscan TO off;
SET enable_sort TO off;


SELECT onek2.* FROM onek2 WHERE onek2.unique1 < 10;


SELECT onek2.unique1, onek2.stringu1 FROM onek2
    WHERE onek2.unique1 < 20
    ORDER BY unique1 using >;


SELECT onek2.unique1, onek2.stringu1 FROM onek2
   WHERE onek2.unique1 > 980;

RESET enable_seqscan;
RESET enable_bitmapscan;
RESET enable_sort;


SELECT p.name, p.age FROM person* p;


SELECT p.name,

... [truncated 3647 chars] ...

x ORDER BY x;


create function sillysrf(int) returns setof int as
  'values (1),(10),(2),($1)' language sql immutable;

select sillysrf(42);
select sillysrf(-1) order by 1;

drop function sillysrf(int);


select * from (values (2),(null),(1)) v(k) where k = k order by k;
select * from (values (2),(null),(1)) v(k) where k = k;


create table list_parted_tbl (a int,b int) partition by list (a);
create table list_parted_tbl1 partition of list_parted_tbl
  for values in (1) partition by list(b);
explain (costs off) select * from list_parted_tbl;
drop table list_parted_tbl;
```

## Async Minify Excerpt

```sql


SELECT * FROM onek
   WHERE onek.unique1 < 10
   ORDER BY onek.unique1;


SELECT onek.unique1, onek.stringu1 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using >;


SELECT onek.unique1, onek.stringu1 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY stringu1 using <;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY string4 using <, unique1 using >;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 > 980
   ORDER BY string4 using >, unique1 using <;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using >, string4 using <;


SELECT onek.unique1, onek.string4 FROM onek
   WHERE onek.unique1 < 20
   ORDER BY unique1 using <, string4 using >;


ANALYZE onek2;

SET enable_seqscan TO off;
SET enable_bitmapscan TO off;
SET enable_sort TO off;


SELECT onek2.* FROM onek2 WHERE onek2.unique1 < 10;


SELECT onek2.unique1, onek2.stringu1 FROM onek2
    WHERE onek2.unique1 < 20
    ORDER BY unique1 using >;


SELECT onek2.unique1, onek2.stringu1 FROM onek2
   WHERE onek2.unique1 > 980;

RESET enable_seqscan;
RESET enable_bitmapscan;
RESET enable_sort;


SELECT p.name, p.age FROM person* p;


SELECT p.name,

... [truncated 3647 chars] ...

x ORDER BY x;


create function sillysrf(int) returns setof int as
  'values (1),(10),(2),($1)' language sql immutable;

select sillysrf(42);
select sillysrf(-1) order by 1;

drop function sillysrf(int);


select * from (values (2),(null),(1)) v(k) where k = k order by k;
select * from (values (2),(null),(1)) v(k) where k = k;


create table list_parted_tbl (a int,b int) partition by list (a);
create table list_parted_tbl1 partition of list_parted_tbl
  for values in (1) partition by list(b);
explain (costs off) select * from list_parted_tbl;
drop table list_parted_tbl;
```

## Symbols

```txt
147| CREATE TEMP TABLE nocols();
155| CREATE TEMP TABLE foo (f1 int);
166| CREATE INDEX fooi ON foo (f1);
175| CREATE INDEX fooi ON foo (f1 DESC);
183| CREATE INDEX fooi ON foo (f1 DESC NULLS LAST);
240| create index onek2_index_full on onek2 (stringu1, unique2);
254| create function sillysrf(int) returns setof int as
269| create table list_parted_tbl (a int,b int) partition by list (a);
270| create table list_parted_tbl1 partition of list_parted_tbl
```
