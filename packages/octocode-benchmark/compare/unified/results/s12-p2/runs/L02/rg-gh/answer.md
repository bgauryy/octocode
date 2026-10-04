**Answer:** The check is `Model._prepare_related_fields_for_save()` in `django/db/models/base.py:1276`. `save()` calls it at `base.py:864` with `operation_name="save"`. If a cached related object has no primary key, it raises `ValueError`. I read the code in the local checkout. I didn't run `git rev-parse` to confirm it is at 4fab678a07, because the sandbox blocked it.

**How it works**
- **Which fields:** it loops over `self._meta.concrete_fields`, skipping any field not in the optional `fields` argument (`base.py:1281-1283`).
- **Only assigned relations:** it acts only when `field.is_relation and field.is_cached(self)` (`base.py:1286`). If no related instance was ever assigned, so only the raw `_id` value is set, nothing is checked. The related object is then read with `getattr(self, field.name)`, and falsy values are skipped (`base.py:1287-1289`).
- **The test:** `if not obj._is_pk_set():` (`base.py:1295`). If true, it does three things:
  - For a reverse side that isn't a multi-valued relation (`not field.remote_field.multiple`, e.g. `OneToOneField`), it removes the cached value from the related instance (`base.py:1297-1298`).
  - It raises `ValueError("save() prohibited to prevent data loss due to unsaved related object '<field>'.")` (`base.py:1299-1302`).
- **Manually assigned PKs pass.** The comment at `base.py:1290-1294` says an unsaved object that has a pk set (manually, or auto-generated like a `UUIDField`) is allowed. In that case the database raises `IntegrityError` if there is a problem.
- **If the related object was saved after assignment:**
  - If the FK attname value is empty (or an `Expression`), the code calls `setattr(self, field.name, obj)` to copy the new pk across (`base.py:1303-1307`).
  - If the related object's target-field value differs from the FK value on `self`, it clears the cached relation (`base.py:1309-1313`).
- **GenericForeignKey:** the same check runs over `self._meta.private_fields` for fields with `fk_field` (`base.py:1315-1330`). It raises the same kind of `ValueError`, using an f-string.

**Other callers:** the same method runs in `QuerySet.bulk_create` (`django/db/models/query.py:794`, `operation_name="bulk_create"`). It also runs at `query.py:1042`, where I didn't read the surrounding code to see which operation that is.

**Uncertainty:** I didn't check the pinned commit SHA. I also didn't trace the `save()` code at `base.py:864` beyond the call itself.