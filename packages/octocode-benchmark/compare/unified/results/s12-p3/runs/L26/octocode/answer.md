**Answer:** With neither force flag set, `Model._save_table` (`django/db/models/base.py:1075`) tries an UPDATE first whenever the primary key is set. If that UPDATE matches no row, it falls back to an INSERT. The one exception is a new instance whose PK has a default, which skips the UPDATE.

**How it decides**
- **PK handling (1103–1108).** If the PK isn't set (and isn't a `DatabaseDefault`), `get_pk_value_on_save` is called to fill it in. `pk_set` is then recomputed.
- **Skipping the UPDATE (1113–1120).** If none of `raw`, `force_insert` or `force_update` is set, `self._state.adding` is true, and every PK field has a default or db default, `force_insert` becomes `True`. So the UPDATE is skipped.
- **Attempting the UPDATE (1123).** The condition is `pk_set and not force_insert`.
  - It builds the field values, then calls `_do_update` (1148). `_do_update` filters `base_qs.filter(pk=pk_val)` (1229).
  - If there are no non-PK values, it returns `[()]` when `update_fields` is not None or the row still exists (`filtered.exists()`). Otherwise it returns `[]` (1230–1238).
  - If `select_on_save` is set and the update isn't forced, it first checks `filtered.exists()` and returns `[]` if the row is gone (1239–1248). I didn't read the rest of `_do_update` past line 1250.

**When the UPDATE matches no row** (`results` is empty, so `updated` is `False`, 1157–1164)
- If `force_update` is set, it raises `NotUpdated("Forced update did not affect any rows.")` (1159–1160).
- If `update_fields` is set, it raises `NotUpdated("Save with update_fields did not affect any rows.")` (1161–1164).
- Otherwise, `if not updated:` (1165) runs the INSERT path:
  - It sets `_order` if the model has `order_with_respect_to` (1166–1182).
  - It builds `insert_fields`, which excludes generated fields and the auto field when the PK isn't set (1183–1187).
  - It calls `_do_insert` (1207) and assigns any returned values (1210–1211).
- `_save_table` returns `updated`, which is `False` when the INSERT ran.

**Uncertainty:** I didn't read the lines after 1250, which cover the rest of `_do_update`. I also didn't read the caller `save_base`.