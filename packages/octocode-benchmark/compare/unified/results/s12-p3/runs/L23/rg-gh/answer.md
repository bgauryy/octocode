At commit 4fab678a07, 24 call sites in `django/db/models/query.py` call `self._chain()` directly. They sit in 24 `QuerySet` methods. I mapped each line to its enclosing `def` with awk, and every one falls inside `class QuerySet`. I couldn't confirm the checkout's HEAD SHA because `git` failed in the sandbox, so I'm relying on you that it is the pinned commit.

Methods, with the line of each call:
- `__getitem__`: 520 and 532 (two calls)
- `__and__`: 547
- `get`: 736. This one is conditional: `self._chain() if self.query.combinator else self.filter(...)`.
- `_earliest`: 1223
- `delete`: 1396
- `_values`: 1575
- `none`: 1708
- `all`: 1721 (`return self._chain()`)
- `_filter_or_exclude`: 1742
- `complex_filter`: 1770
- `_combinator_query`: 1778
- `select_for_update`: 1823
- `select_related`: 1847
- `prefetch_related`: 1874
- `_annotate`: 1924
- `order_by`: 1966
- `distinct`: 1980
- `extra`: 1997
- `reverse`: 2005
- `defer`: 2019
- `only`: 2043
- `using`: 2049
- `fetch_mode`: 2055

Several public methods clone indirectly and are not in this list. For example, `filter` and `exclude` go through `_filter_or_exclude`, and `annotate` goes through `_annotate`.