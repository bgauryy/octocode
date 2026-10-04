Django blocks the save in `Model._prepare_related_fields_for_save()`, at `django/db/models/base.py:1276`. `Model.save()` calls it first, at `base.py:864`, before any database work. When a related object has no primary key, the method raises `ValueError`.

**How the check works** (`base.py:1276-1330`):
1. It loops over `self._meta.concrete_fields`. If a `fields` argument was passed, it skips fields not in that list.
2. It only looks at relation fields whose related object is already cached on the instance (`field.is_relation and field.is_cached(self)`). If nothing was assigned, there is nothing to check.
3. It fetches the object with `getattr(self, field.name, None)` and skips falsy values.
4. If `not obj._is_pk_set()`, it raises `ValueError("%s() prohibited to prevent data loss due to unsaved related object '%s'.")`. The first `%s` is the operation name, such as `save`. Before raising, for non-`multiple` relations (one-to-one), it removes the cached reverse value with `field.remote_field.delete_cached_value(obj)`.
5. If the related object has a pk, the method then does some cache upkeep:
   - If the FK attname value is empty or an `Expression`, it re-runs `setattr(self, field.name, obj)`. This picks up an object that was saved after being assigned.
   - If the target's pk or `to_field` value no longer matches the FK value, it clears the cached relation with `field.delete_cached_value(self)`.
6. A second loop over `self._meta.private_fields` does the same pk check for `GenericForeignKey` fields (those with `fk_field`). It raises the same `ValueError` message, in the f-string form.

**Notes**
- The code comments say the check exists because saving a nullable FK pointing at an unsaved object would silently lose data.
- The test is "is the pk set", not "was the object saved". A manually assigned pk, or an auto-generated one such as a `UUIDField` default, passes the check. The code comment says the database is left to raise an `IntegrityError` if needed.
- The same method is also called from `django/db/models/query.py:794` (`bulk_create`, with `operation_name="bulk_create"`) and `query.py:1042`. I did not read what operation name is passed at `query.py:1042`.