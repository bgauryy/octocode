The check is `Model._prepare_related_fields_for_save()` in `django/db/models/base.py`, defined at line 1276. It raises `ValueError` when a cached related object has no primary key set. I read the body at lines 1280–1320 and the call sites from a text search. I did not read the top of the method (1276–1279) or the code around each call site.

**Where it is called**
- `Model.save()` calls it as `self._prepare_related_fields_for_save(operation_name="save")` at `base.py:864`.
- `QuerySet.bulk_create()` calls it with `operation_name="bulk_create"` at `query.py:794`.
- A third call is at `query.py:1042`. I only saw the start of that call, so I don't know which operation it belongs to.

**How the check works**
1. **Concrete fields (`base.py:1281–1310`):**
   - It loops over `self._meta.concrete_fields`, skipping any field not in `fields` when `fields` is given.
   - It only looks at relation fields that are cached on the instance (`field.is_relation and field.is_cached(self)`). If no related object was ever assigned, there is nothing to check.
   - It reads the assigned object with `getattr(self, field.name, None)` and skips falsy values.
   - If `not obj._is_pk_set()`, it raises `ValueError("%s() prohibited to prevent data loss due to unsaved related object '%s'." % (operation_name, field.name))`.
   - Before raising, for non-`multiple` relations (`not field.remote_field.multiple`), it also clears the reverse cache with `field.remote_field.delete_cached_value(obj)`.
   - The test is "pk is set", not "saved in the database". A manually assigned pk, or an auto-generated one such as a `UUIDField`, passes. The comment says Django then relies on the database to raise `IntegrityError`.
2. **Related object saved after assignment (`base.py:1301–1317`):**
   - If the FK attname value is in `field.empty_values` or is an `Expression`, it re-runs `setattr(self, field.name, obj)`. This copies the now-populated pk onto the FK column.
   - If the target's pk or `to_field` value no longer matches the FK attname value, it calls `field.delete_cached_value(self)` to clear the stale cache.
3. **Generic foreign keys (`base.py:1319–1329`):**
   - It loops over `self._meta.private_fields` and checks cached fields that have `fk_field` (GenericForeignKey).
   - It raises the same `ValueError`, worded as an f-string, if the cached object has no pk.