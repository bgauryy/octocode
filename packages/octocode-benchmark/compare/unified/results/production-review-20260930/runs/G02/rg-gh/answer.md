**Short answer:** `Counter` fields now validate with a dedicated `counter` core schema. Before, pydantic built a `dict` schema and wrapped it in Python-level validators. Valid inputs still give a `Counter`. The changes are the error type and message for bad input, strict mode (now implemented natively), and support for length constraints. The merged PR is #13824, merge commit `617abdb`.

**Before** (old `_generate_schema.py:660-704`, from the parent commit)
- `collections.Counter` went through `_mapping_schema(Counter, keys, int)`. That built `dict_schema(keys, int, strict=False)`.
- It was wrapped in `lax_or_strict_schema`. The lax branch was an after-validator `Counter(...)`. The strict branch was `chain(is_instance(Counter), lax)`.
- Bad input therefore failed in the inner dict validator. I infer the error was `dict_type` ("Input should be a valid dictionary"). I didn't run the old code.
- The PR description says the goal is to let constraints "apply properly" and links #13704. I infer the old schema wasn't a `dict` type, so `min_length` and `max_length` had nothing to attach to.

**After**
- `_counter_schema(keys)` returns `core_schema.counter_schema(generate_schema(keys), int_schema())`. Keys are validated against the type parameter and values are always `int`. Bare `Counter` uses `Any` keys (`_generate_schema.py` hunks at ~387 and ~428, plus new `_counter_schema` at ~485).
- **Lax mode, Python input** (`input_python.rs`, `lax_counter`):
  - A `Counter` instance is accepted as-is.
  - An exact `dict` or any `Mapping` is accepted and coerced.
  - Anything else fails with the new `counter_type` error, "Input should be a valid Counter". `[1,2,3]` is now `counter_type`, not `dict_type`.
- **Strict mode** (`strict_counter`): only `Counter` instances are accepted, so a plain `dict` gives `counter_type`. JSON objects are still accepted in strict mode, since otherwise a `Counter` couldn't be created from JSON (`input_json.rs`, `strict_counter`). Strictness does not propagate to keys or values.
- **Validator** (`validators/counter.rs`):
  - Items are collected into a `dict`, then `Counter(dict)` is built at the end.
  - Key errors get loc `(key, '[key]')`, for example `(1, '[key]')` with `string_type`. Value errors get loc `(key,)`.
  - `fail_fast` is supported.
  - Zero and negative counts are preserved (`test_counter_values_preserved`).
- **Constraints:**
  - `min_length` and `max_length` now work natively. `'counter'` was added to `DICT_CONSTRAINTS` in `_known_annotated_metadata.py`, and `too_short` and `too_long` errors are raised from the validator.
  - `pipeline.py` also treats `counter` as length-constrainable.
- **JSON schema:** `counter_schema` reuses `_common_dict_schema`. The output is `{'type': 'object', 'additionalProperties': {'type': 'integer'}}`, plus `minProperties` and `maxProperties` when set.
- **Serialization:** a new `CounterSerializer` and `ObType::Counter` were added. Python mode dumps a `Counter`. JSON mode dumps a plain `dict` or JSON object. Any-typed fields holding a `Counter` now also round-trip as `Counter`.
- **Cleanup:** `Counter` was removed from `MAPPING_ORIGIN_MAP` in `_validators.py`. `collections.Counter` was added to the allowed `defaultdict` default-factory types.

**Evidence:** the PR diff (`gh pr diff 13824`). The behaviour is pinned by the new `tests/types/test_counter.py` and `pydantic-core/tests/validators/test_counter.py`.

**Uncertainty:**
- I didn't build or run either version. The "before" error type and the constraint behaviour are inferred from the old schema code and the PR description.
- I didn't read the `pydantic-core/tests/validators/test_counter.py` assertions in detail.
- I can't cite exact line numbers in the merged tree, because I read the diff and not a checkout.