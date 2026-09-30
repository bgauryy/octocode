With `parse_docstring=True`, the `@tool` decorator gets the argument schema and the description from one parse of the function's Google-style docstring. Paths below are under `libs/core/langchain_core/`, at 67ee6cb63d.

**Call chain**
1. `tools/convert.py:~320-326`: `@tool` calls `StructuredTool.from_function(..., parse_docstring=parse_docstring, error_on_invalid_docstring=...)`.
2. `tools/structured.py:~260-266`: if `args_schema is None and infer_schema`, it calls `create_schema_from_function(name, source_function, parse_docstring=..., filter_args=_filter_schema_args(source_function))`.
3. `tools/base.py:263`, `create_schema_from_function`:
   - It builds a base Pydantic model from `inspect.signature` using `validate_arguments`, or the v1 variant for v1 annotations.
   - It drops `self` and `cls` for methods, the `FILTERED_ARGS` entries, injected args (`_is_injected_arg_type`), and the placeholder `args`, `kwargs` and `v__duplicate_kwargs` fields.
   - It calls `_infer_arg_descriptions(...)` (`base.py:343`).
   - It passes the surviving fields, `descriptions=arg_descriptions` and `fn_description=description` to `_create_subset_model` (`base.py:~362-368`).
4. `_infer_arg_descriptions` (`base.py:170-201`):
   - It gets the annotations with `get_type_hints(fn, include_extras=True)`.
   - When `parse_docstring` is true, it calls `_parse_python_function_docstring` (`base.py:126`), which takes `inspect.getdoc(fn)` and passes it to `_parse_google_docstring` (`utils/function_calling.py:735`).
   - It then raises `ValueError` if the docstring documents an arg that isn't in the annotations (`base.py:153-167`, called at `base.py:194-195`).
   - Any arg the docstring didn't describe falls back to a description from its `Annotated[...]` metadata (`base.py:196-200`). `_get_annotation_description` (`base.py:105`) reads a string or a `FieldInfo.description`.
5. `_parse_google_docstring` (`utils/function_calling.py:735-818`):
   - It splits the docstring on `"\n\n"` into blocks.
   - The description is every block before the first `Args:` block, skipping any block that starts with `Returns:` or `Example:`. The kept blocks are joined with spaces and stripped (`:771-782`).
   - Each line of the `Args:` block becomes `name: desc`. A `name (type):` form is reduced to `name`, and lines indented deeper than the first arg line are appended as continuations (`:790-817`).
   - If `error_on_invalid_docstring` is set, it raises `ValueError("Found invalid Google-Style docstring.")` when args exist but the docstring has too few blocks or no `Args:` block (`:756-767`), or when there is no docstring at all (`:784-786`). `run_manager`, `callbacks`, `runtime` and `return` are excluded from that check.
6. `_create_subset_model` in `utils/pydantic.py:284` dispatches to the v2 version (`:234`) or the v1 version (`:205`).
   - Each field's `description` comes from `descriptions_.get(field_name, field.description)`, so the docstring wins over an existing description (`:247`).
   - The model's `__doc__` is `textwrap.dedent(fn_description or model.__doc__ or "")` (`:276`; the v1 version does the same at `:230`).

**Tool description** (`tools/structured.py:267-297`)
- If `description` is `None` and `parse_docstring` is false, it uses the raw `func.__doc__`.
- With `parse_docstring=True`, that raw-docstring step is skipped. If no `description` was passed, it falls back to the generated `args_schema.__doc__`, which is the parsed description (`:270-279`). In that case the `Args:` section is not part of the tool description.
- A description passed explicitly always wins.
- If the result is still `None`, it raises `ValueError("Function must have a docstring if description not provided.")`.
- When the description came from the docstring, it is run through `textwrap.dedent(...).strip()`.

**Uncertainty:** I read all of these paths directly. I did not run the code. I also didn't trace the `infer_schema=False` path (`convert.py` builds a plain `Tool`), because it never parses the docstring.