**Short answer:** With `@tool(parse_docstring=True)`, the decorator builds a pydantic model from the function signature. It then takes the argument descriptions and the model's description from the Google-style docstring. The tool's own description is not taken from the raw docstring in this mode. It is whatever `description=` was passed, or else the schema model's `__doc__`, which is the parsed summary. Paths below are under `libs/core/langchain_core/`. I did not read the remainder of the Args-parsing loop in `_parse_google_docstring`, which starts after line 800.

**Flow**

1. **Decorator.** `tools/convert.py` `_tool_factory` (lines 282–329) takes the plain function and calls `StructuredTool.from_function(...)`. It passes `parse_docstring`, `error_on_invalid_docstring`, `infer_schema` and `args_schema` through (lines 316–329). `description` is passed as given by the caller, which is `None` by default.

2. **Schema creation.** `tools/structured.py:258-266` runs when `args_schema is None and infer_schema`. It calls `create_schema_from_function(name, source_function, parse_docstring=..., error_on_invalid_docstring=..., filter_args=_filter_schema_args(source_function))`.

3. **Model from the signature.** `create_schema_from_function` is at `tools/base.py:263-368`.
   - It builds a base model from the signature with pydantic's `validate_arguments`. It uses `validate_arguments_v1` if the annotations are pydantic v1 (lines 294–303).
   - It drops `self` and `cls` for methods, the default `FILTERED_ARGS`, injected args when `include_injected` is false, and pydantic's placeholder `args`, `kwargs` and `v__duplicate_kwargs` fields (lines 320–360).

4. **Descriptions.** `tools/base.py:343` calls `_infer_arg_descriptions` (lines 170–201). With `parse_docstring` on, it does the following:
   - It reads the annotations with `get_type_hints(fn, include_extras=True)`.
   - It calls `_parse_python_function_docstring` (lines 126–149), which calls `inspect.getdoc` and then `_parse_google_docstring`. That function is in `utils/function_calling.py:735`.
   - It runs `_validate_docstring_args_against_annotations` (lines 152–167). This raises `ValueError` if an arg in the docstring is not in the function signature.
   - For any arg the docstring did not describe, it falls back to `Annotated[...]` metadata, either a string or a `FieldInfo.description` (lines 196–200, helper at lines 105–123).

5. **`_parse_google_docstring`** (`utils/function_calling.py:735-…`):
   - It splits the docstring on blank lines (`\n\n`).
   - If `error_on_invalid_docstring` is set, it raises `ValueError("Found invalid Google-Style docstring.")` in two cases. One is when there are not enough blocks or no `Args:` block after the first block, and the function has args other than `run_manager`, `callbacks`, `runtime` and `return`. The other is when the docstring is empty.
   - The function description is the blocks before `Args:`, joined with spaces. Blocks starting with `Returns:` or `Example:` are skipped.
   - The `Args:` block is parsed line by line into `{arg: description}`. Continuation lines are detected by indentation (the loop starts at line 800).

6. **Final model.** `create_schema_from_function` ends with `_create_subset_model(...)` (`tools/base.py:362-368`). The call passes the valid field names, `descriptions=arg_descriptions` and `fn_description=description`.
   - In `utils/pydantic.py:234-277`, `_create_subset_model_v2` rebuilds each field with `description = descriptions_.get(field_name, field.description)` (line 247).
   - It sets `rtn.__doc__ = textwrap.dedent(fn_description or model.__doc__ or "")` (line 276). The parsed summary therefore becomes the schema model's docstring. `_create_subset_model` (line 284) picks the v1 or v2 variant to match the model's pydantic version.

7. **Tool description** (`tools/structured.py:267-297`):
   - `description_ = description`.
   - The raw-`__doc__` fallback at line 268 is skipped when `parse_docstring` is true (`if description is None and not parse_docstring`).
   - If `description_` is still `None` and there is an `args_schema`, it uses `args_schema.__doc__` (lines 270–272). That is the parsed summary from step 6. If that doc contains "A base class for creating Pydantic models" it becomes `""` (lines 273–277); otherwise an empty doc becomes `None` (lines 278–279).
   - If the description is still `None`, it raises `ValueError("Function must have a docstring if description not provided.")` (lines 288–290).
   - The description is only dedented and stripped when no explicit `description` was passed (lines 291–293). It is then used for the `StructuredTool` (lines 298–307).

**Uncertainty:** I traced the pydantic v2 path (`_create_subset_model_v2`) in detail and did not read `_create_subset_model_v1`.