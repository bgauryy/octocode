The check is in `Model._prepare_related_fields_for_save()` at `django/db/models/base.py:1276`. It raises `ValueError` when a foreign key holds an unsaved instance.

**Callers** (all pass an `operation_name` that ends up in the error message):
- `Model.save()` calls it at `base.py:864` with `"save"`.
- `bulk_create()` calls it at `query.py:794` with `"bulk_create"`.
- `bulk_update()` calls it at `query.py:1042` with `"bulk_update"` and the `fields` argument.

**How the check works** (`base.py:1281-1315`):
1. It loops over `self._meta.concrete_fields`. If `fields` was given, it skips any field not in that list (`:1282`).
2. It only looks at relation fields whose related object is already in the instance's cache (`:1286`). If nothing was assigned to the field, there is nothing to check.
3. It gets the related object with `getattr(self, field.name)` (`:1287`) and skips it if it is falsy.
4. If `not obj._is_pk_set()` (`:1296`), the related object has no primary key, meaning it is unsaved. It then:
   - removes the object from the reverse cache when the relation isn't `multiple`, i.e. a one-to-one (`:1298-1299`);
   - raises `ValueError("%s() prohibited to prevent data loss due to unsaved related object '%s'.")` (`:1300-1303`).
5. If the object does have a pk, there is no error. If the FK attname is empty or an `Expression`, the object is re-assigned with `setattr` so the pk is picked up after a late save (`:1304-1309`). If the target's value no longer matches the FK attname, the cached relation is cleared (`:1312-1315`).
6. `GenericForeignKey` fields in `_meta.private_fields` get the same pk check with a similar `ValueError` (`:1317-1330`). They are only checked if they are cached and have `fk_field`.

The comment at `:1290-1295` says a manually assigned pk, or an auto-generated one such as a `UUIDField` default, passes the check even if the row isn't saved. Django then relies on the database to raise `IntegrityError`.

I read all the code cited here. I did not check the tests.