After this PR, `collections.Counter` fields are validated by a dedicated `counter` core schema, not by the generic mapping schema they used before. The PR is merged. I read the diff only and didn't run any code. I also didn't open the pre-PR `_mapping_schema` source, so the "before" side is inferred from the lines the PR removed and from its description.

**Before**
- `collections.Counter` went through `self._mapping_schema(collections.Counter, keys, int)` (removed at `pydantic/_internal/_generate_schema.py:387` and `:428`).
- `typing.Counter` and `collections.Counter` were listed in `MAPPING_ORIGIN_MAP` in `_validators.py`. The PR removes those entries.
- The PR description says the new schema "allows constraints to apply properly" and links issue #13704. That implies constraints such as `min_length` and `max_length` were not applied properly before.

**After**
- `collections.Counter` now maps to `_counter_schema(keys_type)`, which builds `core_schema.counter_schema(keys, int_schema())` (`_generate_schema.py`, new method near `:485`).
- `collections.Counter` was also added to the `_validators.py` mapping at `:435`.
- Keys are validated against the type parameter, or `Any` for a bare `Counter`. Values are always validated as `int`.
- Accepted inputs in lax mode (`input_python.rs`, `lax_counter`):
  - A `Counter` instance is accepted as is.
  - An exact `dict` or any `Mapping` is accepted and coerced to a `Counter`. For example, `{'a': '1'}` becomes `Counter({'a': 1})`.
  - Anything else raises the new `counter_type` error: "Input should be a valid Counter". The PR's tests show a list input producing this error.
- Strict mode (`input_python.rs`, `strict_counter`): only `Counter` instances are accepted, so a plain `dict` raises `counter_type`.
- JSON input: an object is accepted even in strict mode, because otherwise a `Counter` could never be created from JSON (`input_json.rs`, `strict_counter`).
- Constraints: `min_length` and `max_length` apply through the core schema's `length_check!`. They raise `too_short` or `too_long` (`validators/counter.rs`). `_known_annotated_metadata.py:75` adds `'counter'` to the dict-constraints group.
- Key errors: an invalid key gives a located error, for example `loc=(1, '[key]')` with `string_type`.
- Values are preserved as given. Zero and negative counts are kept, and the result is built with `Counter(dict)` after validation (`validators/counter.rs`, end of `consume_iterator`).
- JSON schema: the new `counter_schema` reuses `_common_dict_schema`, giving `{'type': 'object', 'additionalProperties': {'type': 'integer'}}`, plus `minProperties` and `maxProperties` when those constraints are set (`pydantic/json_schema.py`).
- Serialization: the PR adds a Rust `counter` serializer (`serializers/type_serializers/counter.rs`). Per the new tests, `model_dump()` returns a `Counter`, and JSON mode returns a plain dict.

**Uncertainty**
- I did not check the old `_mapping_schema` behavior directly, so I can't say exactly which constraints were ignored before or how the old error messages differed.
- I didn't verify the exact behavior for `Counter` subclasses.