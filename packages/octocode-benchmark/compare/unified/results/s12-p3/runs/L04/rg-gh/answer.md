With `parse_docstring=True`, the tool's argument schema and its description both come from the Google-style docstring. The `@tool` decorator passes the flag to `StructuredTool.from_function`, which calls `create_schema_from_function`. The description is then taken from the generated schema model's `__doc__`.

I couldn't run `git rev-parse` (sandbox error), so I didn't confirm the checkout is at 67ee6cb63d. Line numbers below are from the local checkout as it stands.

**Flow** (paths under `libs/core/langchain_core/`)

1. **The decorator.** `tools/convert.py:~315-327` calls `StructuredTool.from_function(...)` with `parse_docstring` and `error_on_invalid_docstring`. This happens when `infer_schema` is true or an `args_schema` is given. If `description=` is passed to `@tool`, it is forwarded as-is.

2. **Building the schema.** `tools/structured.py:~257-264` calls `create_schema_from_function` only if `args_schema is None and infer_schema`. It passes `filter_args=_filter_schema_args(source_function)`.

3. **Base model from the signature.** `tools/base.py:~263-296` (`create_schema_from_function`) runs `inspect.signature`. It wraps the function with pydantic `validate_arguments` (or the v1 variant if the annotations are pydantic v1) to get `validated.model`.

4. **Which arguments are kept.** `tools/base.py:~296-342` decides which arguments to drop:
   - `self` and `cls` are dropped for methods.
   - The default `FILTERED_ARGS` are dropped.
   - Injected args are dropped unless `include_injected` is set.
   - The placeholder `args`, `kwargs` and `v__duplicate_kwargs` fields are dropped.

5. **Docstring and annotation descriptions.** `tools/base.py:343-346` calls `_infer_arg_descriptions`. It is defined at `tools/base.py:170-201` and works as follows:
   - It reads the type hints with `get_type_hints(fn, include_extras=True)`.
   - With `parse_docstring`, it calls `_parse_python_function_docstring` (`base.py:126-150`). That takes `inspect.getdoc(fn)` and hands it to `_parse_google_docstring`.
   - Without `parse_docstring`, the description is just `inspect.getdoc(fn) or ""` and the arg descriptions are empty.
   - With `parse_docstring`, `_validate_docstring_args_against_annotations` (`base.py:153-167`) raises `ValueError` if the docstring documents an arg that isn't in the signature.
   - For any arg the docstring didn't describe, it falls back to `_get_annotation_description` (`base.py:105-123`). That reads a string or `FieldInfo.description` from `Annotated[...]` metadata.

6. **The docstring parser.** `utils/function_calling.py:735-815` (`_parse_google_docstring`):
   - It splits the docstring on `"\n\n"` into blocks.
   - The function description is every block before `Args:`. Blocks starting with `Returns:` or `Example:` are skipped. The kept blocks are joined with spaces and stripped (~771-782).
   - The `Args:` block is parsed line by line (~789-815). A `name: desc` line starts a new arg. A name written as `name (type)` has the type part dropped. Lines indented deeper than the first arg line are appended to the previous arg's description.
   - With `error_on_invalid_docstring`, it raises `ValueError("Found invalid Google-Style docstring.")`. That happens if there is no docstring, if there are fewer than `_MIN_DOCSTRING_BLOCKS` blocks, or if no `Args:` block follows the first block. The args `run_manager`, `callbacks`, `runtime` and `return` are exempt from that check (~756-767).

7. **The final model.** `tools/base.py:~362-368` calls `_create_subset_model(model_name, inferred_model, valid_properties, descriptions=arg_descriptions, fn_description=description)`. In `utils/pydantic.py`:
   - Each field's description is set from `descriptions` (~225 for v1, ~247 for v2).
   - `rtn.__doc__ = textwrap.dedent(fn_description or model.__doc__ or "")` (~230 and ~276).

**Tool description**

In `StructuredTool.from_function` (`tools/structured.py:~267-297`):

- If `description` is None and `parse_docstring` is true, the raw `__doc__` is not used (`structured.py:268`).
- `description_` then falls back to `args_schema.__doc__` (`structured.py:270-272`). That is the parsed summary, with the `Args:` section removed.
- A pydantic boilerplate docstring ("A base class for creating Pydantic models") becomes `""` (`structured.py:274-277`).
- If the result is still None, it raises `ValueError("Function must have a docstring if description not provided.")` (`structured.py:288-290`).
- The result is dedented and stripped only when no explicit `description` was given (`structured.py:291-293`).

**Precedence:** the explicit `description` argument wins, then the parsed docstring summary, then the `args_schema` description (`convert.py:126-131`).

**Not verified:** I didn't run the code. I also didn't read the `_create_subset_model` dispatch in `utils/pydantic.py` (~289-310) in full, so which branch handles v1 versus v2 is inferred from the line pairs at ~225 and ~247.