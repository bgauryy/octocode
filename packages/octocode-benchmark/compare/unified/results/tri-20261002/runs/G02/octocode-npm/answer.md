**Short answer:** Counter fields now validate through a dedicated `counter` core schema instead of the generic mapping schema. Constraints (`min_length`/`max_length`) now apply, and strict mode and error messages change. Lax-mode coercion of dicts and mappings into a `Counter` is described as unchanged. I read the merged patches, not the resulting files or the tests, and I did not run any code.

**Before**
- `collections.Counter` was built with `self._mapping_schema(obj, Any, int)`, or with the first type argument as the key type (`pydantic/_internal/_generate_schema.py`, patch hunks `@@ -387` and `@@ -428`).
- `collections.Counter` and `typing.Counter` were in `MAPPING_ORIGIN_MAP` in `pydantic/_internal/_validators.py`. That is the Python-side mapping-validation path.
- The PR body says the point of the change is to let constraints "apply properly" (issue #13704), which implies they did not before. The PR doesn't spell out the old failure mode, and I didn't inspect the old code to confirm it.

**After**
- **Schema generation:** `_counter_schema(keys_type)` builds `core_schema.counter_schema(self.generate_schema(keys_type), core_schema.int_schema())` (`_generate_schema.py`, patch `@@ -485`). Keys use the first generic argument, or `Any` for a bare `Counter`. Values are always validated as `int`.
- **Core validator:** The new `pydantic-core/src/validators/counter.rs` validates each key and value and collects them into a `dict`.
  - Key errors get a `[key]` location suffix. Value errors are located by key.
  - `fail_fast` is supported.
  - Length is checked with `length_check!(..., "Counter", min_length, max_length, ...)`.
  - The result is built as `Counter(output_dict)`.
- **Constraints:** `'counter'` was added to the `DICT_CONSTRAINTS` group in `_known_annotated_metadata.py`. `min_length`, `max_length`, `MinLen` and `MaxLen` now apply to counters. The docs add a Counters section saying these map to JSON Schema `minProperties`/`maxProperties`.
- **Python input, lax mode** (`input_python.rs`, `lax_counter`): an existing `Counter` is accepted as is. An exact `dict` is accepted and coerced. Any other `Mapping` is accepted and coerced. Everything else fails with `counter_type`.
- **Python input, strict mode** (`strict_counter`): only `Counter` instances pass. The docs state that strictness doesn't extend to keys and values; it has to be set on the parameter types.
- **JSON input:** `strict_counter` accepts a JSON object. The inline comment says this is "since otherwise it would be impossible to create a Counter from JSON". A string-mapping input accepts only a mapping, and a plain string is rejected.
- **New error:** `counter_type`, with the message "Input should be a valid Counter" (`errors/types.rs`). Before, a bad input such as `x='test'` would have produced the generic dict or mapping error. I didn't check the exact old message.
- **Also touched:** serialization (`serializers/type_serializers/counter.rs`), JSON schema generation, the error docs, and tests.

**Uncertainty**
- I did not read the 31-file diff in full. I skipped the serializer, the JSON schema change, the `pipeline.py` change, the second page of changed files, and all of the tests.
- The exact "before" behaviour of constraints and strict mode is inferred from the PR body and the replaced code. I did not run the old version.
- I did not check whether `typing.Counter` still resolves to the new schema once its `MAPPING_ORIGIN_MAP` entry was removed. The docs call it a "deprecated alias", so it probably does.