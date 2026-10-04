Twenty-five `QuerySet` methods call `self._chain()` directly. I searched `django/db/models/query.py` for `self._chain(` and matched each hit's line to its method's line range.

| Method | `_chain()` line |
|---|---|
| `__getitem__` | 520 and 532 (two calls) |
| `__and__` | 547 (`combined = self._chain()`) |
| `get` | 736 (`self._chain() if self.query.combinator else …`, so only when a combinator is set) |
| `_earliest` | 1223 |
| `delete` | 1396 (`del_query`) |
| `_values` | 1575 |
| `none` | 1708 |
| `all` | 1721 (`return self._chain()`) |
| `_filter_or_exclude` | 1742 |
| `complex_filter` | 1770 (inside a conditional branch) |
| `_combinator_query` | 1778 |
| `select_for_update` | 1823 |
| `select_related` | 1847 |
| `prefetch_related` | 1874 |
| `_annotate` | 1924 |
| `order_by` | 1966 |
| `distinct` | 1980 |
| `extra` | 1997 |
| `reverse` | 2005 |
| `defer` | 2019 |
| `only` | 2043 |
| `using` | 2049 |
| `fetch_mode` | 2055 |

- **Nothing found for `__or__` and `__xor__`:** the search found no `self._chain(` call in either. I did not read their bodies, so I don't know how they build their result.
- **Other methods that don't call it directly:** `filter`, `exclude`, `union`, `intersection`, `difference`, `annotate`, `alias`, `values`, `values_list`, `dates` and `datetimes` also had no hit. I did not read these either, but they likely reach `_chain()` through other methods such as `_filter_or_exclude`, `_combinator_query`, `_annotate` and `_values`.
- **Search limits:** I matched the literal text `self._chain(`, so a call written another way (an alias, or a call on another object) would be missed. The method names come from line ranges, not from reading each body.