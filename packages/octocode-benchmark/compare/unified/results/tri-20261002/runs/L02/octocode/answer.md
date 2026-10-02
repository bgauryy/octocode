Django blocks this in `Model._prepare_related_fields_for_save()` in `django/db/models/base.py:1276-1330`. It raises `ValueError`.

**Where it is called**
- `Model.save()` calls it at `base.py:864` with `operation_name="save"`.
- `bulk_create` calls it at `query.py:794` with `"bulk_create"`.
- A third call at `query.py:1042` passes `operation_name` across more than one line. I didn't read its context, so I'm not sure which method it belongs to. It is probably `bulk_update`.

**How the check works** (`base.py:1281-1303`)
1. It loops over `self._meta.concrete_fields`. If a `fields` subset was passed, it skips fields outside that subset (`1282-1283`).
2. It only looks at relation fields whose related object is already cached on the instance (`field.is_relation and field.is_cached(self)`, `1286`). If nothing was assigned, there is nothing to check.
3. It gets the related object with `getattr(self, field.name, None)` and skips it if it is falsy (`1287-1289`).
4. If `not obj._is_pk_set()` (`1296`), the related object is unsaved. In that case:
   - For one-to-one relations (where `field.remote_field.multiple` is false), it first removes the object from the related instance's cache with `field.remote_field.delete_cached_value(obj)` (`1298-1299`).
   - It then raises `ValueError("<op>() prohibited to prevent data loss due to unsaved related object '<field>'.")` (`1300-1303`).
5. The check tests whether the primary key is set, not whether the row exists in the database. A manually assigned or auto-generated pk (for example a `UUIDField`) passes. The code comments (`1290-1295`) say the database is then left to raise `IntegrityError`.

**Other behaviour in the same method**
- If the related object was saved after being assigned, the FK attname value is empty (or an `Expression`). `setattr(self, field.name, obj)` then copies the object's pk into the FK column (`1304-1309`).
- If the related object's target field value no longer matches the FK value, the cached relation is cleared (`1312-1315`).
- Private fields are handled separately, which covers `GenericForeignKey` (`1317-1330`). The check there is the same, except it only applies to fields that have `fk_field`, and it raises with an f-string message.

I didn't verify the line 1042 call site or the field descriptors, which are the other place that handles assignment.