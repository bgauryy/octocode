Thirty `QuerySet` methods in `django/db/models/query.py` call `self._chain()` directly. I grepped the checkout. I didn't run `git rev-parse`, because `/dev/null` was blocked in the sandbox, so I didn't confirm it is at 4fab678a07. No `class` line falls between the first and last hit, so all of them are in `QuerySet`.

| Method | Line of `self._chain()` |
|---|---|
| `__getitem__` | 520 and 532 (two calls) |
| `__and__` | 547 (`combined = self._chain()`) |
| `get` | 736 (only when `self.query.combinator` is set; otherwise it uses `self.filter(...)`) |
| `_earliest` | 1223 |
| `delete` | 1396 (`del_query`) |
| `_values` | 1575 |
| `none` | 1708 |
| `all` | 1721 (`return self._chain()`) |
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

**Caveats:**
- This list only covers the literal text `self._chain()`. Public methods that reach `_chain()` indirectly are excluded, such as `filter`, `exclude`, `annotate` and `values`.
- I read the enclosing method for each hit from grep context and didn't open every body. The `def` line shown for each hit is the nearest one above it.