Twenty-three `QuerySet` methods call `self._chain()` directly, all in `django/db/models/query.py`. I took the 24 call sites from a text search and AST match, then mapped each to its enclosing `def` using the method start lines. I did not open the bodies, so the line-to-method mapping is by position only.

| Method | Call line |
|---|---|
| `__getitem__` | 520 and 532 |
| `__and__` | 547 |
| `get` | 736 |
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

**Conditional calls:**
- `get` (line 736) only calls it when the query has a combinator: `clone = self._chain() if self.query.combinator else self.filter(*args, **kwargs)`.
- `complex_filter` (line 1770) calls it inside a nested block (the line is indented one level deeper than the others), not unconditionally.

**Scope:** I counted only methods that call `self._chain()` themselves. Methods that reach it through `self.filter()`, `self._values()` and similar are not listed. `EmptyQuerySet`, `RawQuerySet` and the other classes in the file are out of scope. The `RawQuerySet` defs from line 2447 on have no matches anyway.