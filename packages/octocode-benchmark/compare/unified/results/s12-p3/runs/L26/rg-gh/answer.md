**Short answer:** In `Model._save_table` (`django/db/models/base.py:1075`), Django tries an UPDATE first if the primary key is set and an insert isn't forced. If that UPDATE matches no row, it falls through to an INSERT. The exceptions are forced updates and `update_fields`, which raise `NotUpdated` instead.

**Decision logic (`base.py`)**
- **Pk is set or defaulted.** If the pk isn't set and isn't a `DatabaseDefault`, `get_pk_value_on_save` fills it in, e.g. from a field default. Then `pk_set = self._is_pk_set(meta)` (1103-1108).
- **Cannot update without a pk.** If there's no pk and `force_update` or `update_fields` is given, it raises `ValueError("Cannot force an update in save() with no primary key.")` (1109-1110).
- **Insert is forced when the pk has a default.** `force_insert` is set to True if all of these hold (1113-1120):
  - the call isn't `raw`;
  - neither `force_insert` nor `force_update` was passed;
  - `self._state.adding` is true;
  - every pk field has a Python default or a DB default.
- **UPDATE is attempted** when `pk_set and not force_insert` (1123). It builds the values from the non-pk, non-generated fields and calls `_do_update` (1148). `updated` is `bool(results)` (1157).

**`_do_update` (1214-1252)**
- It runs `base_qs.filter(pk=pk_val)` and then `filtered._update(values, returning_fields)` (1229, 1252). The result is falsy when no row matched.
- If there are no values to set, it returns `[()]` when `update_fields is not None` or the row exists. Otherwise it returns `[]` (1230-1238).
- With `Meta.select_on_save` and no forced update, it first checks `filtered.exists()` and returns `[]` if the row is missing (1239-1251). If the UPDATE then returns 0, it checks `exists()` again to tell "row deleted" apart from "database reported 0 rows".

**When the UPDATE matches no row (1157-1165)**
- **Forced update:** `force_update` raises `self.NotUpdated("Forced update did not affect any rows.")` (1159-1160).
- **`update_fields`:** raises `self.NotUpdated("Save with update_fields did not affect any rows.")` (1161-1164).
- **Plain `save()`:** `updated` stays False, so the `if not updated:` block runs (1165). It then does an INSERT:
  - it sets `_order` first for models with `order_with_respect_to` (1166-1181);
  - it builds `insert_fields` (1182-1186);
  - it calls `_do_insert` (1205), which uses `manager._insert` (1254-1259).
- `_save_table` returns `updated`, which is False after an INSERT.

**Not checked:** I didn't read the caller `save_base`, which handles inheritance parents. Each table in the chain goes through `_save_table`. I also didn't confirm the pinned commit with `git log`, because git failed in this sandbox. The line numbers come from the checkout you pointed me to.