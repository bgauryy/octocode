**Short answer:** with `parse_docstring=True`, `@tool` builds the argument schema by running `create_schema_from_function`. That function makes a Pydantic model from the signature and fills in per-argument descriptions from the Google-style `Args:` block. The tool's description is not taken from the raw docstring. It is the docstring text before `Args:`, and `StructuredTool.from_function` only uses it when no explicit `description` is given. I read the code but did not run it.

**Flow**

1. `tool` in `libs/core/langchain_core/tools/convert.py`:
   - `_tool_factory` calls `StructuredTool.from_function(...)` and passes `parse_docstring`, `error_on_invalid_docstring`, `args_schema` and `infer_schema` (`convert.py:316-329`).
   - A `Runnable` input is handled separately (`convert.py:286-306`).
2. `StructuredTool.from_function` in `structured.py`:
   - If `args_schema is None and infer_schema`, it calls `create_schema_from_function(name, source_function, parse_docstring=..., error_on_invalid_docstring=..., filter_args=_filter_schema_args(...))` (`structured.py:258-266`).
   - With `parse_docstring` on, it skips the raw `__doc__` fallback for the tool description (`structured.py:268-269`).
   - If there is no explicit description, it falls back to `args_schema.__doc__`, which is where the docstring-derived text arrives (`structured.py:270-279`). A dict schema uses its `"description"` key instead (`structured.py:280-281`).
   - It raises `ValueError` if the description is still `None` (`structured.py:288-290`). A description that came from the function's docstring, not an explicit one, is dedented and stripped (`structured.py:291-293`).
3. `create_schema_from_function` in `base.py:263-368`:
   - It builds a base model with `validate_arguments` (the v1 variant if the annotations are Pydantic v1), using `_SchemaConfig` with `extra="forbid"` and `arbitrary_types_allowed=True` (`base.py:292-303`).
   - It filters out `run_manager` and `callbacks` (`FILTERED_ARGS`, `base.py:74`). It also filters `self` or `cls` for methods, any injected args when `include_injected` is false, and the virtual `args`, `kwargs` and `v__duplicate_kwargs` fields (`base.py:320-360`).
   - It calls `_infer_arg_descriptions(...)` (`base.py:343`), then `_create_subset_model(model_name, inferred_model, valid_properties, descriptions=arg_descriptions, fn_description=description)` (`base.py:362-368`).
4. `_infer_arg_descriptions` in `base.py:170-201`:
   - It reads the type hints with `get_type_hints(fn, include_extras=True)`.
   - If `parse_docstring` is true, it calls `_parse_python_function_docstring`. That calls `inspect.getdoc` and then `_parse_google_docstring` (`base.py:144-149`).
   - It then calls `_validate_docstring_args_against_annotations`, which raises `ValueError` if the docstring documents an arg that isn't in the signature (`base.py:152-167`, called at `base.py:194-195`).
   - For args the docstring didn't describe, it falls back to `Annotated[...]` metadata: a string, or a `FieldInfo.description` (`base.py:105-123`, `base.py:196-200`).
   - Without `parse_docstring`, the description is just `inspect.getdoc(fn)` and there are no arg descriptions (`base.py:191-193`).
5. `_parse_google_docstring` in `utils/function_calling.py:735-…`:
   - It splits the docstring on `"\n\n"` into blocks (`function_calling.py:755`).
   - The description is every block before the first `Args:` block, joined with spaces. Blocks starting with `Returns:` or `Example:` are skipped, and the loop does not stop there in case `Args:` comes later (`function_calling.py:768-782`).
   - If `error_on_invalid_docstring` is set, it raises `ValueError("Found invalid Google-Style docstring.")` in two cases:
     - The function has real args, ignoring `run_manager`, `callbacks`, `runtime` and `return`, and the docstring has fewer than `_MIN_DOCSTRING_BLOCKS` blocks or no `Args:` block after the first (`function_calling.py:756-767`).
     - The docstring is empty (`function_calling.py:784-786`).
   - The `Args:` block is parsed line by line into `name: description`. The base indentation is latched from the first argument line, and more deeply indented lines are treated as continuations of the previous argument (`function_calling.py:790-800`).
6. `_create_subset_model_v2` in `utils/pydantic.py:234-277`:
   - For each kept field it makes a `FieldInfo` whose description is `descriptions.get(name, field.description)`. It keeps the field's default or `default_factory` and its metadata.
   - It sets `rtn.__doc__ = textwrap.dedent(fn_description or model.__doc__ or "")` (`pydantic.py:276`). That `__doc__` is what `from_function` later uses as the tool description.

**Uncertainty:** I didn't read `function_calling.py` past line 800, so the exact end of the `Args:` line parsing is unverified. I also didn't read `_create_subset_model_v1`.