**Short answer:** PR #13824 is merged (merge commit `617abdb1`). `Counter[K]` fields now validate through a dedicated `counter` core schema in pydantic-core. Before, they went through the generic mapping path. The accepted inputs look largely the same. What changes is where length constraints apply, the error type, strict mode, and the JSON Schema output.

**Before (partly inferred from the diff's removed lines)**
- `Counter` used `self._mapping_schema(collections.Counter, <key type>, int)`, at `pydantic/_internal/_generate_schema.py` (removed in the diff hunks at ~387 and ~428).
- `collections.Counter` and `typing.Counter` were entries in `MAPPING_ORIGIN_MAP` in `_validators.py`.
- Issue #13704 reports that `min_length`/`max_length` on `Counter` emitted `minLength` instead of `minProperties` in the JSON Schema.
- I did not open `_mapping_schema` itself, so the exact old validator chain is not verified.

**After**
- Schema generation: `_counter_schema(keys_type)` returns `core_schema.counter_schema(generate_schema(keys_type), core_schema.int_schema())`. Values are always validated as `int`. Bare `Counter` uses `Any` keys.
- Lax mode (`input_python.rs`, `lax_counter`):
  - A `Counter` instance is accepted as an exact match.
  - An exact `dict` is accepted as a lax match.
  - Any other `Mapping` is accepted as a lax match.
  - Anything else raises the new `counter_type` error: "Input should be a valid Counter". The docs example shows `Model(x='test')` failing this way.
- Strict mode (`strict_counter`): only `Counter` instances pass. Strict does not extend to keys or values; those need their own strict annotations.
- Output: the validator, `validators/counter.rs`, collects items into a `dict` and then builds `Counter(output)`. The result is always a real `Counter`.
- Errors:
  - Key errors get `[key]` locations.
  - Value errors are located by key.
  - `fail_fast` is supported.
  - Partial validation (`allow_partial`) is handled.
- Constraints: `min_length`/`max_length` are now checked inside the core validator via `length_check!`.
- JSON Schema: `json_schema.py` adds `counter_schema`, which reuses `_common_dict_schema`. That produces `type: object` with `minProperties`/`maxProperties`, which is the #13704 fix. The docs table lists these keywords.
- Serialization: there is a new `CounterSerializer`. It rebuilds a `Counter` in Python mode and emits a plain dict in JSON mode. A value that isn't a `Counter` falls back to inferred serialization with a warning.
- Other changes: `Counter` was added to the type-to-constructor map in `_validators.py`. New tests are in `tests/types/test_counter.py` and `pydantic-core/tests/validators/test_counter.py`.

**Not verified**
- I read only the PR diffs, not the checked-out tree.
- I did not open the `input_json.rs` and `input_string.rs` changes, so JSON-input and string-input behavior is unverified.
- I did not run any tests.
- The exact before-behavior of strict mode and error types is not confirmed.