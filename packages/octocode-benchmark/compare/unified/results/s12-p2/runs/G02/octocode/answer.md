**Short answer:** after PR #13824, `Counter` fields are validated by a new dedicated `counter` core schema. Before, they went through the generic mapping path (`_mapping_schema(obj, ..., int)`). Valid input still ends up as a `Counter` with `int` values. What changes is strict mode, the error type, and constraint support. I reviewed the patches only. I did not run the code, and I did not see the "before" `_mapping_schema` internals or the PR's tests.

**Before** (`pydantic/_internal/_generate_schema.py`, removed lines in the diff):
- `collections.Counter` was built with `self._mapping_schema(obj, Any, int)`, or `_mapping_schema(collections.Counter, <key type>, int)` for the parametrized form.
- `_internal/_validators.py` listed `typing.Counter` and `collections.Counter` in `MAPPING_ORIGIN_MAP`. That entry is removed.
- The PR body says the change lets constraints "apply properly" and links issue #13704. This implies they did not apply properly before. I did not read that issue.

**After:**
- **Schema:** `_counter_schema(keys_type)` returns `core_schema.counter_schema(generate_schema(keys_type), core_schema.int_schema())` (`_generate_schema.py`, new lines in the diff). Unparametrized `Counter` uses `Any` keys. Values are always validated as `int`.
- **Lax mode** (`input_python.rs`, `lax_counter`):
  - A `Counter` instance is accepted as is.
  - An exact `dict` is accepted and coerced to a `Counter`.
  - Any other `Mapping` is also accepted and coerced.
  - Anything else fails.
- **Strict mode** (`strict_counter`): only `Counter` instances are accepted. In JSON, an object is accepted, since a `Counter` could not otherwise be created from JSON (`input_json.rs`).
- **Output:** the validator builds a plain `dict` of the validated items, then calls `collections.Counter(output)` (`validators/counter.rs`). Keys and values are validated per item, with `[key]` locations on key errors and `fail_fast` support.
- **Errors:** a non-mapping input such as the string `'test'` now raises the new `counter_type` error with the message "Input should be a valid Counter" (`errors/types.rs`; the docs example shows this for `Model(x='test')`). Previously it would have been the generic mapping error. I inferred this and did not see the old output.
- **Constraints:** `min_length` and `max_length` are now enforced on counters. `_known_annotated_metadata.py` adds `'counter'` to the dict constraints, and `validators/counter.rs` calls `length_check!` on the output. The JSON Schema keywords are `minProperties` and `maxProperties` per the docs. I did not see the `json_schema.py` patch.
- **Serialization:** serialization also gets a counter path in `infer.rs`, which rebuilds a `Counter` in python mode. The remaining serializer files were paginated out and I did not read them.

**Not verified:**
- Whether a strict-mode `Counter` failure behaves differently from before.
- The exact old error for non-mapping input.
- The new tests in `tests/types/test_counter.py`.
- The `json_schema.py` and `pipeline.py` changes.

The PR was merged on 2026-09-17, with source SHA `0398b829e190be1bc4c121abca812c49b726b276`.