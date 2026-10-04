**Short answer:** In `Model._save_table` (`django/db/models/base.py:1075`), Django tries an UPDATE first whenever the primary key is set and an insert isn't forced. If that UPDATE matches no row, it falls back to an INSERT. I didn't run anything. This comes from reading the code. `git rev-parse` failed in this sandbox, so I couldn't confirm the checkout is at 4fab678a07.

**How it decides**
1. It resolves the PK. If the PK isn't set and isn't a `DatabaseDefault`, it calls `get_pk_value_on_save` and assigns the result (`base.py:1103-1108`). `pk_set` is then computed from `_is_pk_set`.
2. If the PK is unset and `force_update` or `update_fields` was given, it raises `ValueError("Cannot force an update in save() with no primary key.")` (`base.py:1109-1110`).
3. It may skip the UPDATE altogether. If all of these hold, it sets `force_insert = True` (`base.py:1113-1120`):
   - the save isn't raw;
   - neither `force_insert` nor `force_update` was passed;
   - `self._state.adding` is true;
   - every PK field has a default or a DB default.
4. It attempts the UPDATE only when `pk_set and not force_insert` (`base.py:1123`). `_do_update` (`base.py:1214`) runs `base_qs.filter(pk=pk_val)` and then `_update(values, returning_fields)`. A non-empty result means a row matched, so `updated = True` (`base.py:1157`).

**Special cases inside `_do_update`**
- **No values to set** (`base.py:1230-1238`): this happens for a PK-only model, or when `update_fields` doesn't touch this table in an inheritance chain.
  - It returns `[()]` (success) if `update_fields is not None` or `filtered.exists()`.
  - Otherwise it returns `[]`.
- **`select_on_save` set and the update isn't forced** (`base.py:1239-1251`):
  - It first checks `filtered.exists()`.
  - If the UPDATE then reports 0 rows, it checks `exists()` again. That covers a database that reports 0 rows even though the row was matched and updated.

**When the UPDATE matches no row** (`base.py:1157-1165`)
- **`force_update=True`:** raises `self.NotUpdated("Forced update did not affect any rows.")`.
- **`update_fields` given:** raises `NotUpdated("Save with update_fields did not affect any rows.")`.
- **Neither:** `updated` stays False and the `if not updated:` block (`base.py:1165`) runs an INSERT.
  - If the model has `order_with_respect_to`, it first sets `_order` to the current max plus 1, or 0 if there are no rows (`base.py:1166-1182`).
  - It builds `insert_fields` from the non-generated concrete fields. The auto field is excluded unless the PK is set (`base.py:1183-1187`).
  - It then calls `_do_insert` and assigns any returned values (`base.py:1207-1210`).

`_save_table` returns `updated`, which is False when the row was inserted.