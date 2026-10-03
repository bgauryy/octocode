**Answer:** The check is `Model._prepare_related_fields_for_save()` in `django/db/models/base.py:1276`. `Model.save()` calls it first, at `base.py:864` (`operation_name="save"`), before it resolves the database or does any writing. If a related object has no primary key, it raises `ValueError`.

**How it works:**
- **Which fields:** It loops over `self._meta.concrete_fields` (`base.py:1281`). If `fields` was passed, it skips any field not in that set (`base.py:1282-1283`).
- **Only assigned relations:** It only looks at fields where `field.is_relation and field.is_cached(self)` (`base.py:1286`). If no related instance was ever assigned, the field isn't cached and the check is skipped. It also skips fields whose cached object is falsy (`base.py:1287-1289`).
- **The test:** `if not obj._is_pk_set():` (`base.py:1296`). When that is true, it does two things:
  - If the relation isn't `multiple` (the one-to-one case), it removes the object's reverse cache with `field.remote_field.delete_cached_value(obj)` (`base.py:1298-1299`).
  - It raises `ValueError("save() prohibited to prevent data loss due to unsaved related object '<field>'.")` (`base.py:1300-1303`).
- **Manual PKs are allowed:** The comment at `base.py:1290-1295` says a PK assigned by hand, or auto-generated as with a `UUIDField`, passes the check. The database then raises `IntegrityError` if the row doesn't exist.
- **Object saved after assignment:** If the parent was saved after being assigned, the child's FK attname may still be empty or an `Expression`. In that case `setattr(self, field.name, obj)` re-syncs it (`base.py:1304-1308`). If the parent's target-field value differs from the child's FK value, the cached relation is cleared (`base.py:1311-1315`).
- **Generic foreign keys:** A second loop covers `self._meta.private_fields` that have `fk_field`. It raises the same kind of `ValueError` for an unsaved cached object (`base.py:1318-1330`).

**Other callers:** `bulk_create` calls it at `django/db/models/query.py:794` and another call is at `query.py:1042`. I did not read the context of that second call, so I don't know which operation it belongs to.