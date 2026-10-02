**Answer:** The check is `Model._prepare_related_fields_for_save()` in `django/db/models/base.py:1276`. It raises `ValueError("save() prohibited to prevent data loss due to unsaved related object '<field>'.")` at `base.py:1300-1303`. I couldn't confirm the checkout is at 4fab678a07, because `git rev-parse` failed in the sandbox. Line numbers are from the checkout as it stands.

**Callers**
- `save()` calls it at `base.py:864` with `operation_name="save"`.
- `bulk_create` calls it at `query.py:794`.
- A third call is at `query.py:1042`. I didn't read it, so I can't say which operation it covers (likely `bulk_update`).

**How the check works** (`base.py:1281-1310`)
1. It loops over `self._meta.concrete_fields`, skipping fields not in `fields` when a `fields` list is passed.
2. It only looks at relation fields whose related object is cached on the instance (`field.is_relation and field.is_cached(self)`). If you only set the raw `_id` value, nothing is cached and the check doesn't run (comment at `base.py:1283-1284`).
3. It gets the related object with `getattr(self, field.name)` and skips it if it is falsy.
4. The test for "unsaved" is `not obj._is_pk_set()` (`base.py:1295`). It checks whether the related object's primary key is set. It does not check whether the row exists in the database.
5. If the pk is not set, it first removes the object from the reverse related-instance cache when the relation is not `multiple` (`base.py:1297-1298`). It then raises the `ValueError`.
6. An object that has a manually assigned pk, or an auto-generated one such as a `UUIDField`, passes even if it was never saved. The code relies on the database to raise an `IntegrityError` in that case (comment at `base.py:1288-1294`).

**After the check passes** (`base.py:1304-1316`)
- If the FK attname value is empty or an `Expression`, the code re-runs `setattr(self, field.name, obj)`. This picks up an object that was saved after being assigned.
- If the related object's target field value differs from the FK value on `self`, the cached relation is cleared with `field.delete_cached_value(self)`.

**GenericForeignKey** (`base.py:1319-1330`)
- Private fields that have an `fk_field` get the same check. The check uses the cached object and `_is_pk_set()`.
- It raises a `ValueError` with the same message, written as an f-string.

**Uncertainty:** I did not read `query.py:1042` or the lines just before `base.py:1281`, so I haven't seen the method's docstring or its opening comment.