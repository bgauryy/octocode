The check is in `Model._prepare_related_fields_for_save()` in `django/db/models/base.py`, which starts at line 1276. `Model.save()` calls it at `base.py:864` with `operation_name="save"`.

**How it works**
- **Which fields are checked:** it loops over the model's concrete fields. For each one that `field.is_relation` and `field.is_cached(self)`, it fetches the assigned object with `getattr(self, field.name, None)`. A FK you only set by raw id has no cached object, so it isn't checked. The loop start (before line 1285) wasn't in the window I read, so the first lines of the loop are not quoted here. It also skips fields outside `fields` when `fields` is given.
- **The test:** if `not obj._is_pk_set()`, it raises a `ValueError` with the message `"save() prohibited to prevent data loss due to unsaved related object '<field>'."`. The code is at about `base.py:1296–1303`.
- **Cache cleanup:** before raising, for a one-to-one (`not field.remote_field.multiple`), it calls `field.remote_field.delete_cached_value(obj)` to drop the reverse cache on the unsaved object.
- **Manually set pks are allowed:** the comments just above the raise say a pk assigned by hand, or auto-generated as with `UUIDField`, lets the save proceed. The database is left to raise an `IntegrityError` if the row is invalid.
- **Saved after assignment:** if the related object was saved after you assigned it and the FK attname is still empty, it does `setattr(self, field.name, obj)`. This copies the new pk into the FK column.
- **Stale cache:** if the related object's target field value differs from the FK value on `self`, it clears the cached relation with `field.delete_cached_value(self)`.
- **GenericForeignKey:** a second loop over `_meta.private_fields` handles `GenericForeignKey`. For cached relations that have `fk_field`, it raises the same kind of `ValueError` if the object's pk isn't set (`base.py:1322–1329`).

**Other callers:** `bulk_create` calls it per object (`query.py:794`, `operation_name="bulk_create"`). There is another call at `query.py:1042`, where I didn't read the `operation_name` argument.

**Not verified:** I didn't read the lines between 864 and the top of the function, or the `bulk_update` call site at `query.py:1042`.