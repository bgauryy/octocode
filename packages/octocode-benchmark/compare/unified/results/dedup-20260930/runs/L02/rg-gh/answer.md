The check is in `Model._prepare_related_fields_for_save()` in `django/db/models/base.py:1276`. If a cached related object has no primary key set, it raises `ValueError`.

**Where it's called**
- `Model.save()` calls it at `django/db/models/base.py:864` with `operation_name="save"`.
- `bulk_create` calls it at `django/db/models/query.py:794`.
- A third call is at `query.py:1042`. I didn't read that code, so I don't know which operation it belongs to (probably `bulk_update`).

**How the check works** (`base.py:1281-1330`)
1. It loops over `self._meta.concrete_fields`. If `fields` was passed, it skips fields not in that list.
2. It only checks fields where `field.is_relation and field.is_cached(self)`. If nothing was assigned to the relation, the cache is empty and the check is skipped (comment at 1283-1284).
3. It gets the related object with `getattr(self, field.name, None)` and skips falsy values.
4. The test is `if not obj._is_pk_set()` (line 1293). If it's true, it does two things:
   - For a one-to-one relation (`multiple` is false), it clears the reverse cache on the related object with `field.remote_field.delete_cached_value(obj)`.
   - It then raises `ValueError("save() prohibited to prevent data loss due to unsaved related object '<field>'.")`. The message is built at lines 1300-1303, and the `%s` is the operation name.
5. The check looks at whether the pk is set, not whether the row exists in the database. The comment at 1287-1292 says a manually assigned or auto-generated pk (such as a `UUIDField`) is allowed. In that case Django relies on the database to raise an `IntegrityError`.
6. If the related object does have a pk, the method syncs the FK value:
   - If the FK attname value is empty or an `Expression`, it re-runs `setattr(self, field.name, obj)`. This picks up an object that was saved after being assigned.
   - If the target's `target_field.attname` no longer matches the FK value, it clears the cached relation with `field.delete_cached_value(self)`.

**Generic foreign keys** (`base.py:1312-1329`)
- It also loops over `self._meta.private_fields`, which covers `GenericForeignKey`.
- It looks at cached fields that have `fk_field`. If the cached object's pk isn't set, it raises the same `ValueError`.

**Uncertainty**
- I read only `base.py:1276-1330` and confirmed the call sites by grep. I didn't open `query.py` around lines 794 and 1042.