**Short answer:** `@tool(parse_docstring=True)` passes the flag to `StructuredTool.from_function`. That calls `create_schema_from_function`, which calls `_infer_arg_descriptions`. Together they build a Pydantic subset model. Its field descriptions come from the Google-style `Args:` section of the docstring. The model's `__doc__` is the docstring's description text. I did not read `_parse_google_docstring`, so the exact split between description and args is inferred from its call site.

**Flow (all paths under `libs/core/langchain_core/tools/`)**
1. `convert.py:316-329`: when `infer_schema` is true or an `args_schema` is given, `tool` calls `StructuredTool.from_function(...)`. It forwards `parse_docstring` and `error_on_invalid_docstring`.
2. `structured.py:258-266`: if `args_schema is None and infer_schema`, it calls `create_schema_from_function(name, source_function, parse_docstring=..., error_on_invalid_docstring=..., filter_args=_filter_schema_args(...))`.
3. `base.py:292-303`: `create_schema_from_function` runs `inspect.signature`. It then builds a Pydantic model with `validate_arguments` (the v1 variant if the annotations are Pydantic v1). The model comes from `validated.model`.
4. `base.py:343-347`: it calls `_infer_arg_descriptions(func, parse_docstring=..., error_on_invalid_docstring=...)`.
5. `base.py:186-201`, `_infer_arg_descriptions`:
   - It reads `get_type_hints(fn, include_extras=True)`.
   - With parsing on, `_parse_python_function_docstring` runs. It takes `inspect.getdoc(fn)` and calls `_parse_google_docstring(docstring, list(annotations), error_on_invalid_docstring=...)` (`base.py:144-149`). That returns `(description, arg_descriptions)`.
   - `_validate_docstring_args_against_annotations` (`base.py:152-167`) raises `ValueError("Arg X in docstring not found in function signature.")` for any documented arg that isn't in the annotations.
   - For args the docstring didn't describe, it falls back to `_get_annotation_description` (`base.py:105-123`). That uses a string or a `FieldInfo.description` found in `Annotated[...]` metadata.
   - With parsing off, the description is the whole `inspect.getdoc(fn)` and `arg_descriptions` starts empty. The `Annotated` fallback still applies.
6. `base.py:348-368`:
   - It drops the placeholder fields `args`, `kwargs` and `v__duplicate_kwargs`, plus the filtered args (`self`/`cls` for methods, `FILTERED_ARGS`, and injected args when `include_injected=False`).
   - It then calls `_create_subset_model(model_name, inferred_model, valid_properties, descriptions=arg_descriptions, fn_description=description)`.
7. `utils/pydantic.py:234-277` (`_create_subset_model_v2`; the v1 version is at 205-231):
   - Each kept field is rebuilt as a `FieldInfo` with `description = descriptions.get(name, field.description)`. The default or `default_factory` and any metadata are preserved.
   - The model's `__doc__` is set to `textwrap.dedent(fn_description or model.__doc__ or "")` (line 276).

**Tool description** (`structured.py:267-297`)
- If `description is None and not parse_docstring`, the description is the raw `func.__doc__`.
- With `parse_docstring=True` that branch is skipped. The description then falls through to `args_schema.__doc__` (lines 270-279), which is the parsed description text set in step 7. If the schema's docstring is the generic Pydantic base-class text, the description becomes an empty string.
- If it is still `None`, it raises `ValueError("Function must have a docstring if description not provided.")`.
- An explicit `description=` passed to `@tool` wins over all of this.
- When the description came from the function's docstring, it is dedented and stripped.

**Uncertainty:** I did not read `_parse_google_docstring`, so I can't say how it handles malformed docstrings beyond the `error_on_invalid_docstring` flag being passed to it.