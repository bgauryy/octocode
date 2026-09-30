**Short answer:** Before the PR, `Counter` fields were validated as a generic mapping with `int` values and then converted to `Counter`. After the PR, they use a dedicated `counter` core schema, `CounterValidator`, in pydantic-core. It accepts the same lenient inputs as before, but constraints now apply properly, strict mode is precise, and the error type is different. I read the merged PR, #13824 (merge commit `617abdb1`), only through its diffs and the new tests. I did not run anything.

**Before (from the removed lines in the diff)**
- `collections.Counter` and `Counter[K]` went through `self._mapping_schema(..., Any or K, int)` in `pydantic/_internal/_generate_schema.py`.
- `typing.Counter` and `collections.Counter` were in `MAPPING_ORIGIN_MAP` in `pydantic/_internal/_validators.py`, so they were handled as a mapping with a `Counter` origin.
- I did not read the old `_mapping_schema` internals, so the mechanics are inferred from the diff.
- The PR body says this change lets constraints "apply properly" (issue #13704). That implies `min_length` and `max_length` did not work correctly before. I did not verify the old constraint behavior directly.

**After**
- **Schema generation:** `_counter_schema(keys_type)` returns `core_schema.counter_schema(generate_schema(keys_type), core_schema.int_schema())`. Keys use the type parameter, or `Any` if there is none. Values are always validated as `int` (`_generate_schema.py`, diff hunks at lines ~387, ~428 and ~485).
- **Accepted input (lax mode):**
  - A `Counter` is accepted, and so are a `dict` and any `Mapping`. Each is validated and rebuilt as a plain `Counter`.
  - Input is always copied, and `Counter` subclasses come back as plain `Counter`. See `test_counter_input` and `test_counter_subclass` in `pydantic-core/tests/validators/test_counter.py`.
  - String keys and values are coerced, for example `{'a': '1'}` becomes `Counter({'a': 1})`.
- **Rejected input:** a list, a tuple, a list of pairs, a string, or an arbitrary object all fail with the new error type `counter_type` ("Input should be a valid Counter").
- **Strict mode:**
  - Only real `Counter` instances pass in Python. A plain `dict` or a `dict` subclass fails with `counter_type`.
  - A JSON object is still allowed in strict mode, because it is the only way to build a `Counter` from JSON.
  - Strictness does not propagate to keys or values.
- **Constraints:** `min_length` and `max_length` (`Field(...)`, `MinLen` and `MaxLen`) are now supported by the validator.
  - The check runs after the items are validated.
  - Failures give `too_short` or `too_long` with `field_type: 'Counter'`.
  - The JSON schema gets `minProperties` and `maxProperties`.
  - `fail_fast` is also supported.
- **Errors:** key errors get the location `(key, '[key]')` and value errors get `(key,)`, as with dict validation. A missing or bad key gives `string_type` with `loc (1, '[key]')` for `Counter[str]`.
- **Values:** zero and negative counts are kept as is. The validator builds a `dict` first and then calls `Counter(dict)`, which does not sum or drop entries. The tests check this.
- **Other behavior in the PR:** a JSON schema of `{'type': 'object', 'additionalProperties': {'type': 'integer'}}` is tested. So are `validate_strings` and smart-union behavior, where a `Counter` input still comes out as `Counter` and a plain `dict` as `dict`.

**Uncertainty**
- The exact pre-PR behavior for constraints and error types is inferred from the diff and the PR body. I did not read the pre-PR source or run it.
- The PR changed the Python input layer (`pydantic-core/src/input/input_python.rs`). I did not open that file, so the strict and lax rules above come from the tests, not the implementation.