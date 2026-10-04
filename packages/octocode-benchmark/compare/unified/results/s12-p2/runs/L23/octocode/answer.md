In `django/db/models/query.py` at 4fab678a07, `QuerySet` has 24 `self._chain()` call sites. I matched them with a text search for `self\._chain\(\)`. I took the method names from a symbol listing and read only `fetch_mode` and `using` directly, so the other line-to-method mappings are inferred from the declaration ranges.

**Methods that call `self._chain()` directly (line of the call):**
- `__getitem__` — 520 and 532 (two call sites)
- `__and__` — 547
- `_earliest` — 1223
- `delete` — 1396
- `_values` — 1575
- `none` — 1708
- `all` — 1721
- `_filter_or_exclude` — 1742
- `complex_filter` — 1770
- `_combinator_query` — 1778
- `select_for_update` — 1823
- `select_related` — 1847
- `prefetch_related` — 1874
- `_annotate` — 1924
- `order_by` — 1966
- `distinct` — 1980
- `extra` — 1997
- `reverse` — 2005
- `defer` — 2019
- `only` — 2043
- `using` — 2049
- `fetch_mode` — 2055

**Borderline case:** `get` calls it conditionally at line 736: `clone = self._chain() if self.query.combinator else self.filter(*args, **kwargs)`. It is a direct call, but only for combinator queries. Otherwise `get` goes through `filter`.

**Not direct callers:** `filter`, `exclude`, `union`, `intersection`, `difference`, `annotate`, `alias`, `values` and `values_list`. They delegate to the methods above.

**Uncertainty:** `__or__` and `__xor__` don't appear in the search results. They likely get their clone through `__and__` or `self.query`, but I didn't read them.