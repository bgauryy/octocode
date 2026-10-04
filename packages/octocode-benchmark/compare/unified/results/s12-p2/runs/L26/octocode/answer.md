The decision is made per table in `Model._save_table` (`django/db/models/base.py:1075`). `save()` tries an UPDATE first when it can, and falls back to an INSERT if that UPDATE matches no row. I read this code but didn't run it.

**How it chooses**
1. If the pk isn't set and isn't a `DatabaseDefault`, it calls `meta.pk.get_pk_value_on_save(self)` and assigns the result to the pk attribute (`:1103-1107`). It then computes `pk_set` (`:1108`).
2. **Skipped UPDATE:** `force_insert` is switched on (`:1113-1120`) when all of these hold:
   - `raw`, `force_insert` and `force_update` are all false.
   - `self._state.adding` is true.
   - Every pk field has a default or a db default.
3. **Attempted UPDATE:** it runs only if `pk_set and not force_insert` (`:1123`). A pk is set and nothing forces an insert.
4. The UPDATE is `_do_update` (`:1214`). It filters `base_qs.filter(pk=pk_val)` and calls `filtered._update(values, returning_fields)` (`:1229`, `:1252`). It returns a truthy result if a row matched (`:1157`).
5. **Special cases inside `_do_update`:**
   - If there are no values to set, it returns `[()]` when `update_fields is not None` or the row exists (`:1230-1238`).
   - If `Meta.select_on_save` is on and the update isn't forced, it checks `exists()` before the UPDATE (`:1239-1251`). It checks again if the UPDATE reports 0 rows.

**When the UPDATE matches no row** (`:1159-1165`)
- With `force_update`, it raises `NotUpdated("Forced update did not affect any rows.")`.
- With `update_fields`, it raises `NotUpdated("Save with update_fields did not affect any rows.")`.
- Otherwise `updated` stays False and the `if not updated:` block (`:1165`) runs an INSERT:
  - It first sets `_order` if the model uses `order_with_respect_to` (`:1166-1182`).
  - It builds `insert_fields`, which excludes generated fields and excludes the auto field if the pk is unset (`:1183-1187`).
  - It then calls `_do_insert` (`:1207`), which calls `manager._insert` (`:1259`). Returned column values are assigned back to the instance (`:1210-1211`).

`_save_table` returns `updated`.

**Uncertainty:** I didn't read `save_base` or `_save_parents`, which call `_save_table` once per table. So I haven't confirmed how `force_insert` and `force_update` get passed down to each table.