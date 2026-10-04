**Short answer:** After PR #13824 (merged 2026-09-17), `collections.Counter` fields are validated by a dedicated `counter` core schema. That schema always produces a real `Counter`, rejects non-mapping input with a new `counter_type` error, and supports `min_length`/`max_length`. Before, `Counter` was handled as a generic mapping with `int` values. I read the PR's patches and did not run the code. The "before" behavior comes from the lines the PR removes and replaces.

**Before**
- `_generate_schema.py` built `Counter` as `self._mapping_schema(obj, Any, int)` for bare `Counter`, and `_mapping_schema(collections.Counter, <first arg or Any>, int)` for `Counter[K]`.
- `_validators.py` listed `typing.Counter` and `collections.Counter` in `MAPPING_ORIGIN_MAP`, which the PR removes. The PR's description links it to issue #13704, about constraints not applying properly.

**After**
- **Schema generation** (`pydantic/_internal/_generate_schema.py`): both `Counter` and `Counter[K]` now call a new `_counter_schema(keys_type)`. It returns `core_schema.counter_schema(generate_schema(keys_type), core_schema.int_schema())`. Keys are validated by the type parameter, which defaults to `Any`. Values are always validated as `int`.
- **Lax mode** (`input_python.rs`, `lax_counter`):
  - A `Counter` instance is accepted as is.
  - An exact `dict` or any `Mapping` is accepted and converted to a `Counter`.
  - Anything else fails with the new `counter_type` error, "Input should be a valid Counter". For example, `[1, 2, 3]` and `'test'` both fail.
  - The doc example `Model(x={'foo': '1'})` gives `Counter({'foo': 1})`.
- **Strict mode** (`strict_counter`): only `Counter` instances are accepted from Python, so a plain `dict` gives `counter_type`. JSON objects are still allowed in strict mode, because a `Counter` can't otherwise be built from JSON (`input_json.rs`). Strictness isn't applied to the keys and values. You have to set it on the parameter types.
- **Constraints**: `'counter'` is added to `DICT_CONSTRAINTS` in `_known_annotated_metadata.py`. `Field(min_length=..., max_length=...)` now works and raises `too_short` or `too_long`. The JSON schema gets `minProperties` and `maxProperties`.
- **Value preservation** (`pydantic-core/src/validators/counter.rs`): the validator collects items into a `dict` and then calls `Counter(dict)`. The PR's test `test_counter_values_preserved` expects zero and negative counts to be kept as is.
- **Serialization**: new Rust serializer code serializes a `Counter` back to a `Counter` in Python mode and to a plain object in JSON mode. The `tests/types/test_counter.py` hunk I read shows `model_dump()['x']` stays a `Counter` and `model_dump(mode='json')['x']` is a `dict`.
- **Error locations**: key errors are reported at `(key, '[key]')`. For example, `Counter[str]` with `{1: 1}` gives `string_type` at `(1, '[key]')`.
- **Unchanged**: `typing.Counter` and `typing_extensions.Counter` follow the same path as `collections.Counter`, and the JSON schema is still `{'type': 'object', 'additionalProperties': {'type': 'integer'}}`.

**Uncertainty**
- I didn't read the old `_mapping_schema` implementation. So the exact old lax-mode details are inferred and not verified. That covers which inputs were accepted, whether the old validator returned a `Counter`, and what error it raised.
- I also didn't read the serializer files in full, the second page of changed files, or the end of the test file.