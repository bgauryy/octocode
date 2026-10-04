**Short answer:** `Counter` fields now validate with a dedicated `counter` core schema. Before the PR they went through the generic `_mapping_schema` path, which wrapped a `dict` schema in a validator function. The PR was merged on 2026-09-17. The diff shows what changes, but I did not run any code before or after.

**Before.** The mapping handlers in `pydantic/_internal/_generate_schema.py` (about lines 387 and 428, per the patch hunk headers) used `self._mapping_schema(collections.Counter, key_type, int)`. `MAPPING_ORIGIN_MAP` in `_validators.py` mapped `typing.Counter` and `collections.Counter` to `collections.Counter`. I did not read `_mapping_schema` itself. Issue #13704 says constraints didn't apply properly under this path. I have not read the issue, and the PR body only links it.

**After:**
- **Schema generation.** `_counter_schema(keys_type)` returns `core_schema.counter_schema(generate_schema(keys_type), core_schema.int_schema())`. Keys use the generic parameter, or `Any` if there is none, and values are always `int`. The `Counter` entries were removed from `MAPPING_ORIGIN_MAP`. (`_generate_schema.py`, `_validators.py` hunks)
- **Lax mode** (`input_python.rs`, `lax_counter`):
  - A `Counter` instance is accepted as an exact match.
  - An exact `dict` is accepted as a lax match.
  - Any other `Mapping` is accepted as a lax match.
  - Anything else fails with the new `counter_type` error, "Input should be a valid Counter". This includes strings.
  - The result is always a new `Counter`, built from an intermediate dict (`validators/counter.rs`).
- **Strict mode:** Python input must be a `Counter` instance, and plain dicts are rejected. JSON objects are still accepted, since otherwise a `Counter` couldn't be created from JSON (`input_json.rs`). The docs say strict mode does not propagate to keys and values.
- **Constraints.** `min_length` and `max_length` now apply natively to `counter` schemas. `'counter'` was added to `DICT_CONSTRAINTS` in `_known_annotated_metadata.py`. The docs map them to JSON Schema `minProperties` and `maxProperties`. There is also a `fail_fast` option, and per-key errors use the `[key]` location convention (`counter.rs`).
- **Serialization.** `ObType::Counter` serializes by rebuilding a `Counter` in Python mode (`infer.rs`). Other files for the serializer, the JSON schema (`json_schema.py`) and tests were changed, but I did not read their patches.

**Uncertainty.** The patch output was truncated, so I did not read the serializer, JSON schema, pipeline or test diffs. I inferred the "before" behavior from the removed lines rather than reading `_mapping_schema`.