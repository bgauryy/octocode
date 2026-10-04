**Answer:** `Model._save_table()` in `django/db/models/base.py` (starts at line 1075) makes the decision, once per table. It tries an UPDATE when a primary key value is set and no insert is forced. If that UPDATE matches no row, it falls through to an INSERT.

**How it decides (lines 1103–1123)**
- If the PK isn't set and isn't a `DatabaseDefault`, it calls `meta.pk.get_pk_value_on_save(self)` and sets the result on the instance (1103–1107). Then `pk_set = self._is_pk_set(meta)` (1108).
- Without a PK, `force_update` or `update_fields` raises `ValueError("Cannot force an update in save() with no primary key.")` (1109–1110).
- Insert is forced (1113–1120) when all of these hold:
  - the save is not `raw`;
  - neither `force_insert` nor `force_update` is set;
  - `self._state.adding` is true;
  - every PK field has a default or a db default.
- The UPDATE is attempted only if `pk_set and not force_insert` (1123). Otherwise it goes straight to INSERT.
- `save_base` (around line 1075) passes `force_insert or parent_inserted` as `force_insert`, so a child is inserted when its parent was just inserted.

**The UPDATE path (1124–1164)**
- `_do_update` runs `base_qs.filter(pk=pk_val)._update(values, returning_fields)` (1229, 1252).
- A truthy result means a row matched, so `updated = True` (1157).
- A model with no non-PK values gets special handling (1230–1238). It returns `[()]` if `update_fields is not None` or the row exists, otherwise `[]`.
- If `select_on_save` is set and the update isn't forced, it first checks `filtered.exists()` (1239–1251). If the UPDATE then returns 0, it re-checks existence to tell a deleted row from a database that reports 0 despite updating.

**When the UPDATE matches no row (1157–1165, 1183–1212)**
- If the update isn't forced, `updated` stays `False` and the code enters the `if not updated:` block (1165). Here it:
  - sets `_order` for `order_with_respect_to` models (1166–1182);
  - builds `insert_fields` and `returning_fields`, then calls `self._do_insert(...)` (1183–1209);
  - assigns any returned values (1210–1211).
- If the update is forced, it raises `NotUpdated` instead of inserting:
  - with `force_update`, the message is "Forced update did not affect any rows." (1159–1160);
  - with `update_fields`, it is "Save with update_fields did not affect any rows." (1161–1164).
- `_save_table` returns `updated` (1212). `save_base` then sends `post_save` with `created=(not updated)`.

I read this directly from the checkout. I did not run it.