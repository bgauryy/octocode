Thirty-one lines in `QuerySet` call `self._chain()` directly, in 26 methods. All are in `django/db/models/query.py`. I took the method names from the `def` line each call sits under and did not open each body, so a call nested in an inner function would be mis-attributed.

| Method | Line(s) of `self._chain()` |
|---|---|
| `__getitem__` | 520, 532 |
| `__and__` | 547 |
| `get` | 736 (conditional: `self._chain() if self.query.combinator else self.filter(...)`) |
| `_earliest` | 1223 |
| `delete` | 1396 |
| `_values` | 1575 |
| `none` | 1708 |
| `all` | 1721 |
| `_filter_or_exclude` | 1742 |
| `complex_filter` | 1770 |
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

Caveats:
- `_chain` is defined at line 2288. Methods such as `filter`, `exclude`, `values` and `annotate` reach it only through helpers like `_filter_or_exclude`, `_values` and `_annotate`, so they don't call it directly.
- I didn't confirm the checkout's HEAD is 4fab678a07. `git` failed in this sandbox because it couldn't open `/dev/null`.