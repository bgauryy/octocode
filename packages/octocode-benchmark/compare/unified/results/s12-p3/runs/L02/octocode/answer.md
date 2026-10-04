Django blocks it in `Model._prepare_related_fields_for_save()` in `django/db/models/base.py:1276`. It raises `ValueError("save() prohibited to prevent data loss due to unsaved related object '<field>'.")`. The `raise` is at `base.py:1300-1303`.

**Where it is called**
- `Model.save()` calls it at `base.py:864` with `operation_name="save"`.
- `bulk_create` calls it at `query.py:794` with `operation_name="bulk_create"`, so the message names that operation instead.
- A third call is at `query.py:1042`. I didn't read its arguments, so I can't say which operation name it passes.

**How the check works** (`base.py:1281-1315`)
1. The method loops over `self._meta.concrete_fields`. If `fields` was passed, it skips any field not in that list (1282-1283).
2. It only looks at relation fields whose related object is cached on the instance (`field.is_relation and field.is_cached(self)`, 1286). If you only assigned the raw `_id` value, nothing is cached and the check is skipped.
3. It reads the cached object with `getattr(self, field.name, None)` and skips falsy values (1287-1289).
4. The test is `if not obj._is_pk_set()` (1296). If the related object has no primary key, it first clears the reverse cache for one-to-one relations (`if not field.remote_field.multiple`, 1298-1299). It then raises the `ValueError`.
5. A pk that was assigned by hand on an unsaved object counts as set. The comment at 1290-1295 says Django lets those saves proceed and relies on the database to raise an `IntegrityError` if needed.
6. If the related object has a pk, the method fixes up the local FK. If the FK attname is empty or an `Expression`, it re-runs `setattr(self, field.name, obj)` (1304-1309). This handles an object that was saved after being assigned. If the target pk no longer matches the FK value, it clears the cached relation (1312-1315).
7. `GenericForeignKey` fields are checked the same way over `_meta.private_fields`. They raise the same `ValueError` at 1326-1330 when the cached object has no pk.