The check is in `Model._prepare_related_fields_for_save()` in `django/db/models/base.py`, defined at line 1276. It raises a `ValueError` before anything is written to the database.

**Where it is called from**
- `Model.save()` calls it at `base.py:864` with `operation_name="save"`, before the `router.db_for_write` lookup and before any insert or update.
- `QuerySet._prepare_for_bulk_create` calls it at `query.py:797` with `operation_name="bulk_create"`.
- `QuerySet.bulk_update` calls it at `query.py:1045` with `operation_name="bulk_update"` and `fields=fields`.

**How the check works**
1. It loops over `self._meta.concrete_fields` (`base.py:1281`). If `fields` was passed, it skips any field not in that set (1282–1283).
2. It only looks at a field if `field.is_relation and field.is_cached(self)` (1286). A related object has to have been assigned to the field. If only the raw `<fk>_id` value was set, the cache is empty and nothing is checked.
3. It gets the related object with `getattr(self, field.name, None)` and skips it if it is falsy (1287–1289).
4. It calls `obj._is_pk_set()` (1296). If the primary key is not set, the object is treated as unsaved. Before raising, it drops the reverse cache entry with `field.remote_field.delete_cached_value(obj)`, but only when `not field.remote_field.multiple` (1298–1299). It then raises:
   `ValueError("<op>() prohibited to prevent data loss due to unsaved related object '<field>'.")` (1300–1303).
5. The check tests whether the primary key is set, not whether the object is actually in the database. The comments at 1290–1295 say a manually assigned pk, or an auto-generated one such as a `UUIDField`, is allowed. In that case the database raises an `IntegrityError` if the row is missing.
6. If the check passes, the code after line 1304 (I read it only up to 1315) re-syncs the FK attname. If the FK attname value is empty, or an `Expression`, it sets the related object again with `setattr` (1305–1309). If the related object's target-field value no longer matches the FK attname, it clears the cached relation (1312–1315).

**Generic foreign keys**
A second loop over `self._meta.private_fields` (1317–1330) applies the same `_is_pk_set()` test to cached `GenericForeignKey` fields. It identifies them by `hasattr(field, "fk_field")` and raises the same message.

**Uncertainty**
I read the code between lines 1304 and 1315 only in part, so the re-sync behaviour in step 6 rests on the lines I saw. I did not check where `bulk_create` and `bulk_update` sit inside their methods beyond the two call sites.