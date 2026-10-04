**Short answer:** `collections.Counter` fields used to be validated by the generic mapping schema. After PR #13824 (merged 2026-09-17), they use a dedicated `counter` core schema. The input types that are accepted are largely the same. What changes is the error type, how constraints apply, JSON handling, and serialization.

I read the patches for most of the PR's 31 files. I did not read the tests, the JSON schema changes, or `pipeline.py`, so I can't say whether the PR changes anything there.

**Before**
- `Counter`, `Counter[K]` and `typing.Counter` were built with `self._mapping_schema(obj, K, int)` (`pydantic/_internal/_generate_schema.py`, the removed lines in the `@@ -387` and `@@ -428` hunks).
- `_validators.py` had `MAPPING_ORIGIN_MAP` entries for `typing.Counter` and `collections.Counter`. This PR removes them and adds `collections.Counter: collections.Counter` to another map in the same file, in the hunk at line 435.
- The PR body says the change is meant to let constraints "apply properly" (issue #13704), following #13757 and #13796. The description doesn't say what was broken before, and I didn't open those issues or PRs.

**After**
- `_counter_schema` calls `core_schema.counter_schema(generate_schema(keys_type), int_schema())`. Keys use the generic parameter, or `Any` if there isn't one. Values are always validated as `int`.
- **Lax mode** (`input_python.rs`, `lax_counter`):
  - `Counter` instances are accepted as-is.
  - Exact `dict` instances are accepted.
  - Any other `Mapping` is accepted.
  - Everything else, such as `'test'`, fails with the new `counter_type` error, "Input should be a valid Counter".
  - The result is always rebuilt as a `Counter`. `validators/counter.rs` collects the validated items into a dict, then calls `Counter(output)`.
- **Strict mode** (`strict_counter`): only `Counter` instances pass.
  - JSON objects are still accepted in strict mode, because otherwise a `Counter` couldn't be built from JSON (`input_json.rs`).
  - Strictness doesn't propagate to keys and values (docs).
- **Constraints:** `counter` is added to `DICT_CONSTRAINTS` in `_known_annotated_metadata.py`. `min_length` and `max_length` are therefore applied natively, with the length check run inside the validator. The docs list them as `minProperties` and `maxProperties` in JSON Schema.
- **Errors:**
  - Key errors are located as `[key]`.
  - Value errors are located by key.
  - `fail_fast` is supported.
- **Serialization:** a new `counter` serializer is added. `infer.rs` also treats `ObType::Counter` as a dict, and in Python mode it rebuilds a `Counter`. Example from the docs: `model_dump()` gives `Counter({'foo': 1})`.
- **Docs:** adds a "Counters" section, a `counter_type` entry in the validation errors page, and conversion-table rows.

**Caveats**
- I didn't run any code. The behavior above comes from reading the patches.
- I can't confirm the exact before-behavior for constraints or the old error message, because I didn't read the old validation path.