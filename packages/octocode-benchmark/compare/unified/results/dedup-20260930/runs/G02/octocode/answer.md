After PR #13824, `Counter` fields are validated by a dedicated `counter` core schema instead of a generic mapping schema. I read the PR diff, not the merged code or a running build. I did not open the pre-PR `_mapping_schema` body or the rest of `tests/types/test_counter.py`.

**Before.** `pydantic/_internal/_generate_schema.py` built `Counter` through `self._mapping_schema(collections.Counter, <key type>, int)`. A bare `Counter` went through `_mapping_schema(obj, Any, int)`. `typing.Counter` and `collections.Counter` were mapped to `collections.Counter` in `MAPPING_ORIGIN_MAP` in `pydantic/_internal/_validators.py`. So it was a dict-style schema plus Python-side conversion to `Counter`. The PR description says the change lets constraints apply properly, linking issue #13704.

**After.**
- `_generate_schema.py` now calls `self._counter_schema(key_type)`. That returns `core_schema.counter_schema(generate_schema(keys_type), core_schema.int_schema())`, so values are always validated as `int`.
- `Counter` moves out of `MAPPING_ORIGIN_MAP` in `_validators.py` and into the list that holds the other concrete mapping types, such as `OrderedDict`.
- The new Rust `CounterValidator` (`pydantic-core/src/validators/counter.rs`) does the following:
  - It validates each key and value, then collects the results into a `dict`.
  - On success it checks `min_length` and `max_length`, then calls `Counter(dict)`. So the output is a real `Counter`, and zero and negative counts are kept as they are.
  - Key errors get the location `(key, '[key]')` and value errors get `(key,)`.
  - It supports `fail_fast` and partial validation.
- Strict mode (from the `test_counter_strict` test):
  - A plain `dict` input is rejected with `counter_type`.
  - A `Counter` instance is accepted.
  - A JSON object is still accepted, because it is the only way to build a `Counter` from JSON.
- New error type `counter_type`, with the message "Input should be a valid Counter". For example, `[1, 2, 3]` now fails with it. The PR documents it in `docs/errors/validation_errors.md`.
- `Field(min_length=..., max_length=...)` now works on `Counter`, raising `too_short` or `too_long`. The PR also touches `_known_annotated_metadata.py`, which I didn't read.
- Lax mode still coerces `{'a': '1'}` into `Counter({'a': 1})`. A bad key fails with `string_type` at `(1, '[key]')`.
- The JSON schema is `{'type': 'object', 'additionalProperties': {'type': 'integer'}}`. The PR changed `pydantic/json_schema.py` (I didn't read that diff).
- Serialization also changed, with a new `counter` serializer in `pydantic-core/src/serializers/type_serializers/counter.rs`. Python-mode dumps keep a `Counter`, and JSON-mode dumps give a `dict`.

**Uncertainty.** I did not read the old `_mapping_schema` implementation, so I can't say exactly which old behaviors differed. The constraint fix and the new `counter_type` error are what the PR's code and tests show. I also did not read the last part of `tests/types/test_counter.py`, which is cut off at `test_counter_json_schema`.