After PR #13824, `Counter` fields are validated by a dedicated `counter` core schema (`CounterValidator`). Before, they went through the generic mapping path. Much of the "before" side is inferred from the diff, not read from source.

**Before (inferred from the removed lines)**
- `collections.Counter` was built with `self._mapping_schema(collections.Counter, <key type>, int)` (`pydantic/_internal/_generate_schema.py`, old lines around 387 and 428). That is a dict-style schema.
- `pydantic/_internal/_validators.py` converted the result to `Counter` through `MAPPING_ORIGIN_MAP`, which held `typing.Counter` and `collections.Counter` entries. The PR removes those entries.
- The PR body says the change lets constraints "apply properly" and links issue #13704. I did not read that issue or the old `_mapping_schema`, so I can't say exactly what broke.

**After (seen in the diff)**
- **Schema:** `_counter_schema(keys_type)` returns `core_schema.counter_schema(self.generate_schema(keys_type), core_schema.int_schema())`. Keys use the annotated type, and values are always validated as `int`. A bare `Counter` uses `Any` keys.
- **Strict mode:** `strict_counter` in `input_python.rs` accepts only `Counter` instances (including subclasses). Anything else raises the new `counter_type` error.
- **Lax mode:** `lax_counter` accepts a `Counter` as an exact match. It accepts an exact `dict`, or any `Mapping`, as a lax match. Anything else, such as the string `'test'`, raises `counter_type`. The new docs page shows `Model(x='test')` giving `'counter_type'`.
- **Iteration and errors:**
  - Items are validated one by one into a temporary `dict`, and then `Counter(output)` is built at the end.
  - Key errors get a `[key]` location suffix and value errors get the key as their location.
  - `fail_fast` and partial validation are supported.
- **Constraints:** `min_length` and `max_length` are checked through `length_check!` on the `Counter` (`pydantic-core/src/validators/counter.rs`). `'counter'` is added to the `DICT_CONSTRAINTS` group in `_known_annotated_metadata.py`, so annotated dict-style length constraints now apply to it.
- **Other files:** The PR also touches the serializer, JSON schema generation, the docs conversion table, and `input_json.rs` and `input_string.rs`. I did not read those, so I can't describe JSON or string-input behavior.

**Uncertainty**
- I did not diff the old `_mapping_schema` behavior directly. I did not read the test changes (`tests/types/test_counter.py` has +147 lines), so any behavior change on edge cases (for example, non-int values or subclass handling) is unverified.