**Short answer:** with `@tool(parse_docstring=True)` on a plain function, the decorator calls `StructuredTool.from_function`. That calls `create_schema_from_function`, which builds a Pydantic model from the signature. The Google-style docstring is parsed for a summary and per-argument descriptions, and those are written into the model's field descriptions and `__doc__`. I read the code but did not run it.

**Flow** (paths under `libs/core/langchain_core/`)

1. **Decorator.** In `tools/convert.py`, the plain-function branch (around lines 307–327) calls `StructuredTool.from_function(func, coroutine, name=..., description=tool_description, args_schema=schema, infer_schema=..., parse_docstring=parse_docstring, error_on_invalid_docstring=..., ...)`. The `parse_docstring` option is documented at `convert.py:151`.

2. **Schema creation.** In `tools/structured.py:~258-266`, if `args_schema is None and infer_schema`, it calls `create_schema_from_function(name, source_function, parse_docstring=..., error_on_invalid_docstring=..., filter_args=_filter_schema_args(source_function))`.

3. **Model from the signature.** `create_schema_from_function` is at `tools/base.py:263`.
   - It runs `inspect.signature`, then wraps the function in pydantic's `validate_arguments`. It uses the v1 variant if the annotations are Pydantic v1 models (`_function_annotations_are_pydantic_v1`, `base.py:221`).
   - The validated function's `.model` is the base model.
   - It drops `self`/`cls` for methods, the default `FILTERED_ARGS`, and any injected args when `include_injected` is false.
   - It also drops pydantic's placeholder `args`, `kwargs` and `v__duplicate_kwargs` fields.

4. **Descriptions.** `_infer_arg_descriptions` (`base.py:170`) does the following:
   - It gets `get_type_hints(fn, include_extras=True)`.
   - With `parse_docstring=True`, it calls `_parse_python_function_docstring` (`base.py:126`). That runs `inspect.getdoc(fn)` and passes the result to `_parse_google_docstring` (`utils/function_calling.py:735`).
   - It then calls `_validate_docstring_args_against_annotations` (`base.py:~194`). This raises `ValueError` if the docstring documents an argument that is not in the signature.
   - For any argument still lacking a description, it falls back to `_get_annotation_description` (`base.py:~120`). That reads a description from `Annotated[...]` metadata or from a `FieldInfo`.
   - Without `parse_docstring`, the description is the whole `inspect.getdoc(fn)` and the argument descriptions come only from annotations.

5. **How `_parse_google_docstring` works** (`function_calling.py:735-817`):
   - It splits the docstring on `"\n\n"`.
   - The description is the leading blocks up to the first `Args:` block, joined with spaces. Blocks starting with `Returns:` or `Example:` are skipped, though it keeps scanning in case `Args:` comes later.
   - The `Args:` block is parsed line by line. A line containing `:` starts a new argument, and the name may carry a `(type)` suffix that gets stripped. More-indented lines are appended as continuations of the previous argument.
   - With `error_on_invalid_docstring=True`, it raises `ValueError("Found invalid Google-Style docstring.")`. This happens when there is no docstring at all, or when the docstring has fewer than the minimum number of blocks or no `Args:` block. The check ignores `run_manager`, `callbacks`, `runtime` and `return`.

6. **Final model.** `_create_subset_model` (`utils/pydantic.py:284`) delegates to `_create_subset_model_v2` (`pydantic.py:234`).
   - For each kept field it builds a `FieldInfo` whose description is `descriptions.get(name, field.description)`, so the docstring description takes priority.
   - It sets the model's `__doc__` to `textwrap.dedent(fn_description or model.__doc__ or "")`. So the parsed summary becomes the schema-level description.

7. **Tool description.** In `StructuredTool.from_function` (`structured.py:~268`), the function's raw `__doc__` is used only when `description is None and not parse_docstring`.
   - With `parse_docstring=True`, an explicit `description` is still used if given.
   - Otherwise it falls through to `args_schema.__doc__`, which is the parsed summary from step 6. This applies when `args_schema` is a `BaseModel` subclass. For a dict schema it uses `args_schema.get("description")`.
   - The result is stripped and dedented.
   - Net effect: the tool description is the docstring summary only, without the `Args:` section.

**Uncertainty:** the line numbers marked `~` come from the printed ranges rather than exact line lookups. I did not read the minimum-block constant `_MIN_DOCSTRING_BLOCKS` or the `tools/base.py` lines around 100–120 in full.