**Short answer:** `@tool(parse_docstring=True)` builds an inferred Pydantic model from the function signature. It then rebuilds that model as a subset model, with argument descriptions taken from the Google-style `Args:` block and the model's `__doc__` set to the docstring summary. `StructuredTool.from_function` then uses that summary as the tool description. All paths are under `libs/core/langchain_core/`.

**1. Decorator path**
- `tools/convert.py:317-327`: `_tool_factory` calls `StructuredTool.from_function(...)`. It passes `parse_docstring` and `error_on_invalid_docstring` when `infer_schema` is true or an `args_schema` is given.
- `tools/structured.py:258-265`: if `args_schema is None and infer_schema`, it calls `create_schema_from_function(name, source_function, parse_docstring=..., filter_args=_filter_schema_args(...))`.

**2. Schema construction** (`tools/base.py:263-368`, `create_schema_from_function`)
- It runs `validate_arguments(func)`, or the v1 variant when the annotations are Pydantic v1 (`base.py:~308-318`). The resulting `validated.model` is the inferred model.
- It builds a filter list: `self`/`cls` for methods, plus `FILTERED_ARGS`. Injected args are added only when `include_injected` is false.
- It calls `_infer_arg_descriptions(...)` at `base.py:343`.
- It drops Pydantic's placeholder fields (`args`, `kwargs`, `v__duplicate_kwargs`) and any filtered args.
- It calls `_create_subset_model(model_name, inferred_model, valid_properties, descriptions=arg_descriptions, fn_description=description)` at `base.py:363-368`.

**3. Docstring parsing**
- `_infer_arg_descriptions` (`base.py:170-201`) reads the type hints with `get_type_hints(include_extras=True)`.
  - With `parse_docstring`, it calls `_parse_python_function_docstring`, which runs `inspect.getdoc` and then `_parse_google_docstring` (`base.py:126-150`).
  - It then checks that every documented arg exists in the annotations (`_validate_docstring_args_against_annotations`, `base.py:152-167`). An unknown arg raises `ValueError`.
  - Finally, for args the docstring doesn't cover, it falls back to the `Annotated[..., "str"]` or `FieldInfo.description` annotation (`base.py:196-200`, using `_get_annotation_description` at `base.py:105`).
- `_parse_google_docstring` (`utils/function_calling.py:735-815`):
  - It splits the docstring on blank lines (`"\n\n"`).
  - The function description is the blocks before `Args:`, skipping `Returns:`/`Example:` blocks, joined with spaces (`:771-782`). If there is no docstring, the description is `""`.
  - It parses the `Args:` block line by line. The first line containing a colon latches the base indent. Deeper-indented lines are appended to the previous arg as continuations. Otherwise the line is split on the first `:` into a name and description. A `name (type)` form is reduced to `name` (`:789-815`).
  - With `error_on_invalid_docstring`, it raises `ValueError("Found invalid Google-Style docstring.")` if there is no docstring, or if annotated args exist but no `Args:` block follows the first block (`:756-767`, `:784-786`). It ignores `run_manager`, `callbacks`, `runtime` and `return` for this check.

**4. Subset model** (`utils/pydantic.py:234-277`, `_create_subset_model_v2`)
- Each field gets `description = descriptions.get(name, field.description)`, so the docstring description wins over any existing one. The default or `default_factory` and any metadata are kept.
- It creates the model with `arbitrary_types_allowed=True`.
- It sets `rtn.__doc__ = textwrap.dedent(fn_description or model.__doc__ or "")`, so the parsed summary becomes the schema's docstring.

**5. Tool description** (`tools/structured.py:267-297`)
- An explicit `description` wins.
- If `description is None and not parse_docstring`, it uses the raw `func.__doc__` (`:268-269`). With `parse_docstring=True` this is skipped, so the description falls through to `args_schema.__doc__`, which is the parsed summary from step 4 (`:270-273`). For a dict schema it uses `args_schema["description"]`.
- It raises `ValueError` if the description is still `None`. When the description came from the docstring, it is dedented and stripped (`:288-297`).

**Uncertainty:** I didn't run the code. The v1 versus v2 branch in `create_schema_from_function` and `_create_subset_model_v1` (`utils/pydantic.py:205`) are described only from a partial read. The `Tool` fallback in `convert.py:331-351` (used when `infer_schema=False`) doesn't parse docstrings.