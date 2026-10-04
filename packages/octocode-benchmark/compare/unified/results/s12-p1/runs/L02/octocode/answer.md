The check is in `Model._prepare_related_fields_for_save()` in `django/db/models/base.py:1276`. It raises a `ValueError` when a cached related object has no primary key. I read the code but did not run it.

**How the check works**
- **Which fields:** it loops over `self._meta.concrete_fields` (`base.py:1281`). If `fields` is given, as with `update_fields`, it skips the others (`:1282-1283`).
- **Cached objects only:** it only looks at fields where `field.is_relation and field.is_cached(self)` (`:1286`). If no instance was ever assigned, the cache is empty and nothing is checked. The related object is read with `getattr(self, field.name, None)` (`:1287`), and a falsy value is skipped (`:1288-1289`).
- **The test:** `if not obj._is_pk_set()` (`:1296`). A related object counts as unsaved when it has no primary key.
- **What happens on failure:**
  - It first removes the object from the related instance cache for one-to-one fields (`:1298-1299`).
  - It then raises `ValueError("%s() prohibited to prevent data loss due to unsaved related object '%s'.")` (`:1300-1303`).
  - The comment at `:1277-1280` explains why: a nullable FK would otherwise be saved as NULL, which silently loses data.
- **Manual pks:** an object with a manually assigned pk passes the check even if it isn't in the database (`:1290-1295`). The comment says Django relies on the database to raise an `IntegrityError` in that case.
- **Stale FK values:** if the FK attname is empty or an `Expression` and the related object now has a pk, the FK is set from the object (`:1304-1309`). A changed target value clears the cached relation (`:1310-1315`).
- **Generic foreign keys:** a second loop over `self._meta.private_fields` does the same check for GenericForeignKeys, with the same message (`:1317-1330`).

**Where it is called**
- `Model.save()` at `base.py:864`, with `operation_name="save"`.
- `QuerySet.bulk_create()` at `query.py:794`, with `operation_name="bulk_create"`.
- `query.py:1042`, a third call whose arguments I didn't read, so I can't say which operation it is.

**Uncertainty:** I didn't read the surrounding code at the three call sites or at `query.py:1042`.