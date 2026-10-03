**Short answer:** `Counter` fields now validate through a dedicated `counter` core schema (`CounterSchema`, `counter_schema()`). Before, they went through the generic mapping schema. Per-item validation is the same: keys are validated by the key type and values as `int`. What changes is the error type, strict mode, constraints, JSON input and serialization. The PR is merged (merge commit `617abdb1`). Everything below comes from the PR diff, and I did not run the code.

**Before (from the removed lines)**
- `Counter` and `Counter[K]` used `self._mapping_schema(obj, Any, int)` and `self._mapping_schema(collections.Counter, K, int)` (`pydantic/_internal/_generate_schema.py`, hunks at about lines 387 and 428).
- `MAPPING_ORIGIN_MAP` in `_validators.py` had `Counter` entries, so validation coerced into a `Counter` through that generic mapping path.
- The PR description says the goal is "this allows constraints to apply properly" (issue #13704). That suggests constraints such as `min_length` and `max_length` did not apply properly before. I did not check the old behaviour directly.

**After**
- **Schema generation:** `Counter` and `Counter[K]` both call `_counter_schema(keys_type)`. It builds `core_schema.counter_schema(generate_schema(K), int_schema())` (`_generate_schema.py`, new `_counter_schema`). The `typing.Counter`, `typing_extensions.Counter` and `collections.Counter` spellings are all tested.
- **Lax mode** (`input_python.rs`, `lax_counter`):
  - A `Counter` instance is accepted as is.
  - An exact `dict` is accepted and coerced.
  - Any `Mapping` is accepted and coerced.
  - Anything else raises the new error `counter_type`, "Input should be a valid Counter". For example, a list or a string gives `counter_type`.
  - The result is always a real `Counter`, built with `Counter(validated_dict)`.
  - Items are first collected into a dict, so zero and negative counts are kept as given (`validators/counter.rs`, and the `test_counter_values_preserved` test).
- **Strict mode** (`strict_counter`, plus the docs): only `Counter` instances are accepted in Python, so a plain `dict` now fails with `counter_type`. JSON objects are still accepted in strict mode, because otherwise a `Counter` could not be created from JSON (`input_json.rs`). Strictness does not propagate to keys and values.
- **Constraints:** `counter` is added to the `DICT_CONSTRAINTS` targets (`_known_annotated_metadata.py`), so `Field(min_length=..., max_length=...)` and `MinLen`/`MaxLen` apply. Violations give `too_short` or `too_long` (`test_constrained_counter`). The experimental pipeline's `_LENGTH_SCHEMA_TYPES` also includes `counter`.
- **Error locations:** a bad key gives, for example, `string_type` at `loc=(1, '[key]')`. Bad values are reported under the key, the same as for dicts.
- **JSON schema:** `counter_schema` reuses `_common_dict_schema`. The output is `{'type': 'object', 'additionalProperties': {'type': 'integer'}}`, plus `minProperties` and `maxProperties` when those constraints are set.
- **Serialization:** there is a new `CounterSerializer` and an `ObType::Counter`. Python mode returns a `Counter`, and JSON mode returns a plain dict. A `Counter` serialized as `Any` now round-trips as a `Counter` in Python mode.

**Uncertainty**
- I read only the diff, not the pre-PR code or the full repo, so I can't say exactly how the old path behaved.
- In particular, I did not confirm how the old generic mapping path treated strict mode, exact `Counter` subclasses or length constraints.
- I also did not check how the old version reported a non-mapping input. It was presumably `dict_type` or `mapping_type`, not `counter_type`.
- I did not build the PR or run its tests.