**Short answer:** `Model._save_table()` in `django/db/models/base.py` decides. If the primary key is set and no insert is forced, Django tries an UPDATE first. If that UPDATE matches no row, it falls back to an INSERT. The fallback is skipped when the caller forced the update or passed `update_fields`, in which case it raises `NotUpdated`.

I read the file at the checkout path. `git rev-parse` failed in this sandbox, so I did not confirm that the checkout is at 4fab678a07.

**How it decides (`_save_table`, lines 1075–1165)**
- **Primary key:** if the pk isn't set and isn't a `DatabaseDefault`, Django calls `meta.pk.get_pk_value_on_save(self)` and sets the result (1103–1107). Then `pk_set = self._is_pk_set(meta)` (1108).
- **Invalid combination:** `force_update` or `update_fields` with no pk raises `ValueError("Cannot force an update in save() with no primary key.")` (1109–1110).
- **Insert shortcut:** `force_insert` is switched on automatically when all of these hold (1113–1120):
  - the call is not `raw`;
  - neither `force_insert` nor `force_update` was passed;
  - `self._state.adding` is true;
  - every pk field has a Python default or a DB default.
- **Update attempt:** the UPDATE runs only when `pk_set and not force_insert` (1123). The call is `self._do_update(...)` (1148).
- **Insert path:** the INSERT block runs when `updated` is still `False` (1165). It optionally computes `_order` for `order_with_respect_to`, builds `insert_fields`, and calls `_do_insert` (1166–1204).

**What `_do_update` does (1214–1252)**
- **Normal case:** it filters `base_qs.filter(pk=pk_val)` and returns `filtered._update(values, returning_fields)`. An empty result means no row matched (1229, 1252).
- **No fields to update:** if `values` is empty, it returns `[()]` when `update_fields is not None` or the row still exists. Otherwise it returns `[]` (1230–1238).
- **`select_on_save`:** when the option is on and the update isn't forced, it first checks `filtered.exists()`. If the UPDATE then returns 0, it checks `exists()` again to tell a deleted row from a DB that reports 0 for a successful update (1239–1251).

**When the UPDATE matches no row (1157–1164)**
- `updated = bool(results)`. On success the returned values are assigned with `_assign_returned_values` (1157–1158).
- If `force_update` was set, it raises `self.NotUpdated("Forced update did not affect any rows.")` (1159–1160).
- If `update_fields` was given, it raises `self.NotUpdated("Save with update_fields did not affect any rows.")` (1161–1164).
- Otherwise `updated` stays `False`, and the INSERT block at 1165 runs.

`_save_table` returns `updated`, which is `True` if an UPDATE hit a row and `False` if an INSERT was done.

**Uncertainty:** I did not read `save()`, `save_base()` or `_save_parents()`. They call `_save_table` once per table in the inheritance chain, but I didn't confirm that here.