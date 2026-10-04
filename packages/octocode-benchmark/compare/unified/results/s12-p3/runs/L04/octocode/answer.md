**Short answer:** with `parse_docstring=True`, `@tool` hands the function to `StructuredTool.from_function`. That calls `create_schema_from_function`, which runs the docstring through a Google-style parser. The parser's output feeds a new Pydantic "subset" model. That model's `__doc__` is the parsed description and each field carries its parsed argument description. Paths below are under `libs/core/langchain_core/`.

**Call chain**
1. `tools/convert.py:316-329`: `tool` calls `StructuredTool.from_function(...)`. It passes `parse_docstring` and `error_on_invalid_docstring` through, along with `description=tool_description`.
2. `tools/structured.py:257-266`: if `args_schema is None and infer_schema`, it calls `create_schema_from_function(name, source_function, parse_docstring=..., error_on_invalid_docstring=..., filter_args=_filter_schema_args(...))`. The name defaults to `__name__`.
3. `tools/base.py:292-303`: the function signature is wrapped with pydantic's `validate_arguments`. The v1 variant is used when the annotations are v1 models, which is checked at `base.py:294`.
4. `tools/base.py:312-318`: it records whether the function has `*args` or `**kwargs`. It then takes `validated.model` as the inferred model.
5. `tools/base.py:320-341`: it builds the list of args to exclude, in three steps:
   - It starts from `filter_args` or `FILTERED_ARGS`.
   - It adds `self` or `cls` for methods.
   - With `include_injected=False`, it also adds injected args.
6. `tools/base.py:343-347`: it calls `_infer_arg_descriptions(func, parse_docstring=..., ...)`.
7. `tools/base.py:348-360`: it keeps the model fields that aren't filtered. It skips the virtual `args` and `kwargs` fields (unless the function really has them) and skips `v__duplicate_kwargs`.
8. `tools/base.py:362-368`: it calls `_create_subset_model(model_name, inferred_model, valid_properties, descriptions=arg_descriptions, fn_description=description)`.

**Description and argument descriptions** (`_infer_arg_descriptions`, `tools/base.py:170-201`)
- Type hints come from `get_type_hints(fn, include_extras=True)`.
- When `parse_docstring` is true, `_parse_python_function_docstring` (`base.py:126-149`) takes `inspect.getdoc(fn)` and calls `_parse_google_docstring` with the annotation names.
- It then calls `_validate_docstring_args_against_annotations` (`base.py:152-167`, called at `base.py:194-195`). This raises `ValueError` if the docstring documents an arg that isn't in the signature.
- Any annotated arg the docstring didn't cover falls back to `_get_annotation_description` (`base.py:105-123`, loop at `base.py:196-200`). That returns the first string metadata item in `Annotated[...]`, or a `FieldInfo.description`. So docstring descriptions take priority over `Annotated` ones.
- Without `parse_docstring`, the description is just `inspect.getdoc(fn) or ""` and there are no docstring arg descriptions (`base.py:191-193`).

**The Google-style parser** (`_parse_google_docstring`, `utils/function_calling.py:735-818`)
- It splits the docstring on `"\n\n"` into blocks.
- Description:
  - It collects blocks until one starts with `Args:`.
  - A block starting with `Returns:` or `Example:` stops further collection, but the scan continues in case `Args:` comes later.
  - The collected blocks are joined with a space and stripped into the description.
- Argument descriptions, from the `Args:` block:
  - It skips blank lines and latches a base indent from the first arg line.
  - A line indented deeper than that base is a continuation and is appended to the current arg.
  - Otherwise, a line containing `:` starts a new arg. The text is split on the first `:`, and a trailing `name (type)` annotation is stripped down to `name`.
  - A line with no colon is also appended to the current arg.
- `error_on_invalid_docstring=True` raises `ValueError("Found invalid Google-Style docstring.")` in two cases:
  - There is no docstring at all.
  - There are annotated args other than `run_manager`, `callbacks`, `runtime` and `return`, and the docstring has fewer than two blocks (`_MIN_DOCSTRING_BLOCKS = 2`) or no `Args:` block after the first.
- With no docstring and the flag off, the description is `""`.

**Building the schema model** (`_create_subset_model_v2`, `utils/pydantic.py:234-277`; `_create_subset_model` dispatches at `pydantic.py:284-311`)
- For each kept field it creates a `FieldInfo` whose `description` is `descriptions_.get(field_name, field.description)`. It also keeps the default or `default_factory`, and the field metadata.
- It builds the model with `arbitrary_types_allowed`. It sets `rtn.__doc__ = textwrap.dedent(fn_description or model.__doc__ or "")`, which is the schema-level description.
- The v1 variant is at `pydantic.py:205-231`. It sets each field's `field_info.description` and sets `__doc__` the same way.

**Tool description** (`tools/structured.py:267-297`)
- `convert.py` passes `tool_description` as `description`. I did not read how `tool_description` is derived at the top of the `tool` decorator, so I can't say what it holds when the caller gave no description.
- If the description is `None` and `parse_docstring` is true, the raw `__doc__` is not used. Instead it falls to `args_schema.__doc__` (`structured.py:270-272`), which is the parsed description from the subset model. An empty-string result is ignored by the `elif not description_: description_ = None` branch.
- If that is still `None`, it raises `ValueError("Function must have a docstring if description not provided.")`.
- When the description came from the docstring, it is dedented and stripped (`structured.py:291-293`).

**Uncertainty:** I did not read the top of `convert.py` (roughly lines 205-300), so I haven't verified how `tool_description` is initialised there. I also didn't check what `_filter_schema_args` returns.