**Where:** The check is `Model._prepare_related_fields_for_save()` at `django/db/models/base.py:1276-1329`. If a related object has no primary key, it raises `ValueError("save() prohibited to prevent data loss due to unsaved related object '<field>'.")`. The message is built at `base.py:1300-1303` for regular fields and at `base.py:1327-1330` for generic foreign keys.

**Callers:**
- `Model.save()` calls it at `base.py:864` with `operation_name="save"`.
- `bulk_create` calls it at `query.py:794`.
- The call at `query.py:1042` is also in `django/db/models/query.py`. I didn't read its context, so I haven't confirmed which operation it serves.

**How it works:**
1. For each of the model's `concrete_fields` (`base.py:1281`), it skips fields not in the optional `fields` list (`base.py:1282`). It only continues when the field is a relation and the related object is cached on the instance (`base.py:1286`). If you only set `fk_id` and never assigned an object, nothing is checked.
2. It reads the cached object (`base.py:1287`) and skips it if it's falsy (`base.py:1288`).
3. The test is `not obj._is_pk_set()` (`base.py:1296`). Before raising, it removes the object from the related instance's reverse cache for one-to-one fields (`if not field.remote_field.multiple`, `base.py:1298-1299`).
4. It only checks whether a pk is set, not whether the row exists in the database. An unsaved object with a manually assigned pk (or an auto-generated one such as a UUID) passes. The database's `IntegrityError` is relied on to catch a bad reference (comment at `base.py:1290-1295`).
5. If the object has a pk, it then does two things:
   - If the FK attname value is empty or an `Expression` (`base.py:1304-1306`), it re-sets the attribute so the now-saved object's pk is copied in (`base.py:1309`).
   - If the object's target-field value no longer matches the FK value (`base.py:1312-1313`), it clears the cached relation (`base.py:1315`).
6. For private fields (`base.py:1317-1330`), it applies the same unsaved-pk check to generic foreign keys. These are identified by having `fk_field` (`base.py:1320-1325`).

The comment at `base.py:1277-1280` gives the reason for the check. Saving a nullable FK that points to an unsaved instance would silently lose data.

I did not check that the checkout is at commit 4fab678a07, because `git` failed in this sandbox. These line numbers come from the checkout as I found it.