**Short answer:** `Counter` fields now use a dedicated `counter` core schema. Before, they used the generic mapping schema with int values. Valid inputs still end up as a `Counter` with int-validated values. What changes is strictness, error type, constraint handling, and JSON schema. I read the PR diff, not the merged tree. I didn't see the tests, `pydantic/json_schema.py`, `pydantic/experimental/pipeline.py`, or the serializer files. I also didn't run anything.

**Before**
- `Counter` and `Counter[K]` went through `self._mapping_schema(obj, Any, int)` and `self._mapping_schema(collections.Counter, <first arg>, int)`. These lines are removed in `pydantic/_internal/_generate_schema.py` (the `@@ -387` and `@@ -428` hunks).
- `typing.Counter` and `collections.Counter` were also entries in `MAPPING_ORIGIN_MAP` in `pydantic/_internal/_validators.py`, and the PR removes those two entries (`@@ -517`). This is the generic mapping path, which is what the PR's discussion of #13704 says stopped constraints from applying properly.

**After**
- The generator calls the new `_counter_schema(keys_type)`. It builds `core_schema.counter_schema(generate_schema(keys_type), core_schema.int_schema())` (`_generate_schema.py`, `@@ -485`). Values are always validated as `int`, and keys follow the type parameter.
- `collections.Counter: collections.Counter` is added to a different map in `_validators.py` (`@@ -435`, next to `OrderedDict`). I only saw the hunk, so I can't say what that map is used for.
- `'counter'` is added to the dict-constraint schema types in `_known_annotated_metadata.py`. This means `min_length` and `max_length` (via `Field` or `MinLen`/`MaxLen`) now apply to the counter schema.
- In pydantic-core's `CounterValidator` (`pydantic-core/src/validators/counter.rs`):
  - Keys and values are validated and collected into a `dict`, errors are located per key, and `fail_fast` is supported.
  - `length_check!` enforces `min_length` and `max_length`.
  - The result is built with `Counter(output)`.

**Input behavior**
- Lax mode (`input_python.rs`, `lax_counter`): the validator accepts a `Counter` instance, an exact `dict`, or any `Mapping`, and coerces the last two to a `Counter`. Anything else raises the new `counter_type` error ("Input should be a valid Counter"). The new docs give `Model(x='test')` as the example.
- Strict mode (`strict_counter`): only `Counter` instances are accepted. A plain `dict` is rejected.
- JSON (`input_json.rs`): a JSON object is accepted even in strict mode, since otherwise a `Counter` couldn't be created from JSON.
- `docs/api/standard_library_types.md` says strict mode does not apply to keys and values. Strictness has to be set on the parameter types.

**Other effects**
- The PR adds a new `counter_type` error code (`errors/types.rs`, `docs/errors/validation_errors.md`).
- Serializers gain a Counter type. In Python mode, `infer.rs` rebuilds a `Counter` from the serialized pairs. In JSON mode, a Counter is serialized as a dict.

**Not verified:** exactly how the old generic path behaved for each input. I inferred the before/after contrast from the removed lines and the PR description. I'm also unsure what the "strict" rows in the old conversion table were.