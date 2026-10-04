**Short answer:** `@tool(parse_docstring=True)` passes the flag through `StructuredTool.from_function` to `create_schema_from_function`. That function builds a Pydantic model from the signature and then rebuilds it as a subset model. Each field gets a description parsed from the Google-style `Args:` block, and the model's `__doc__` is set to the parsed summary text.

**Pipeline**

1. `tool` in `libs/core/langchain_core/tools/convert.py:316-327` calls `StructuredTool.from_function(...)` with `parse_docstring` and `error_on_invalid_docstring`. This happens when `infer_schema` is true or `args_schema` is given.

2. `StructuredTool.from_function`, in `tools/structured.py:258-266`, calls `create_schema_from_function(name, source_function, parse_docstring=..., error_on_invalid_docstring=..., filter_args=_filter_schema_args(...))`. This only runs when `args_schema is None and infer_schema`.

3. `create_schema_from_function`, in `tools/base.py:263-368`, does the following:
   - It gets `inspect.signature(func)` and builds a base model with pydantic's `validate_arguments`. It uses the v1 variant if the annotations are Pydantic v1 (`:294-303`).
   - It filters out `self`/`cls` for methods, `FILTERED_ARGS`, any `filter_args`, and injected args when `include_injected` is false (`:320-341`).
   - It calls `_infer_arg_descriptions` to get `(description, arg_descriptions)` (`:343-347`).
   - It drops pydantic's placeholder fields `args`, `kwargs` and `v__duplicate_kwargs`, and any filtered field (`:348-360`).
   - It returns `_create_subset_model(model_name, inferred_model, valid_properties, descriptions=arg_descriptions, fn_description=description)` (`:362-368`).

4. `_infer_arg_descriptions`, in `tools/base.py:170-201`:
   - It reads the type hints with `get_type_hints(fn, include_extras=True)` (`:186`).
   - With `parse_docstring` on, it calls `_parse_python_function_docstring`, which runs `inspect.getdoc` and then `_parse_google_docstring` (`:126-149`, `:187-190`).
   - It then calls `_validate_docstring_args_against_annotations`. This raises `ValueError` if the docstring names an arg that is not in the signature (`:152-167`, `:194-195`).
   - For each annotated arg with no docstring description, it falls back to `Annotated[...]` metadata via `_get_annotation_description` (`:196-200`). That helper takes a string annotation or a `FieldInfo.description` (`:105-123`).
   - Without `parse_docstring`, the description is the whole `inspect.getdoc(fn)` and there are no arg descriptions from the docstring (`:191-193`).

5. `_parse_google_docstring`, in `utils/function_calling.py:735-818`:
   - It splits the docstring on blank lines (`"\n\n"`) (`:755`).
   - With `error_on_invalid_docstring`, it raises `ValueError("Found invalid Google-Style docstring.")` if the docstring has too few blocks or no `Args:` block. This check ignores `run_manager`, `callbacks`, `runtime` and `return` (`:756-767`). An empty docstring also raises (`:783-786`).
   - The description is every block before the first `Args:` block, joined with spaces. Blocks starting with `Returns:` or `Example:` are skipped (`:768-782`).
   - Within the `Args:` block, each line containing `:` starts a new arg (`name: desc`). A `name (type)` prefix is reduced to `name` (`:809-815`). Lines indented deeper than the first arg line are treated as continuations and appended to the previous description (`:792-808`, `:816-817`).

6. `_create_subset_model_v2`, in `utils/pydantic.py:234-277` (v1 variant at `:205-231`; dispatcher `_create_subset_model` at `:284-311`):
   - For each kept field, the description is `descriptions_.get(field_name, field.description)`, so the docstring value wins over any existing field description (`:247`).
   - It copies the default or `default_factory` and any metadata (`:248-255`).
   - It creates the model with `ConfigDict(arbitrary_types_allowed=True)` and keeps the selected `__annotations__` (`:258-275`).
   - It sets `rtn.__doc__ = textwrap.dedent(fn_description or model.__doc__ or "")` (`:276`).

**Tool description** (`structured.py:267-297`)
- If `description is None and not parse_docstring`, the tool description is the raw `func.__doc__`.
- With `parse_docstring=True`, the raw docstring is not used. `description_` stays `None` and falls to the `args_schema` branch (`:270-287`). That takes the generated schema's `__doc__`, which is the parsed summary without the `Args:` section.
- A user-supplied `description` takes precedence. If `description_` is still `None`, it raises `ValueError("Function must have a docstring if description not provided.")` (`:288-290`).
- When the description comes from the docstring (`description is None`), it is dedented and stripped (`:291-297`).

**Uncertainty:** I did not run the code. The wiring to `from_function` is from `convert.py:316-327`. I did not read the earlier part of the `tool` decorator, which sets `tool_description`.