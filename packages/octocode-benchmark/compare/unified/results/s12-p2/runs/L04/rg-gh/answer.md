**Short answer:** with `parse_docstring=True`, `@tool` builds the schema by inferring a Pydantic model from the function signature. It then rebuilds that model as a subset, with per-argument descriptions taken from the Google-style `Args:` block. The tool's own description comes from the docstring text before `Args:`, but only through the schema, not through the function's `__doc__`. I read the code at the pinned checkout and did not run it. Paths below are under `libs/core/langchain_core/`.

**Call chain**
1. `tools/convert.py:~321-326` passes `parse_docstring` and `error_on_invalid_docstring` from `@tool` into `StructuredTool.from_function`.
2. `tools/structured.py:~257-264`: if `args_schema is None and infer_schema`, it calls `create_schema_from_function(name, source_function, parse_docstring=..., error_on_invalid_docstring=..., filter_args=_filter_schema_args(...))`.
3. `tools/base.py:263` (`create_schema_from_function`):
   - It runs `validate_arguments` (or the pydantic v1 variant) on the function to get an inferred model, `validated.model`.
   - It drops `self`/`cls` for methods, `FILTERED_ARGS`, the `args`/`kwargs` placeholders when the function has no `*args`/`**kwargs`, and `v__duplicate_kwargs`.
   - It calls `_infer_arg_descriptions(...)` at ~`base.py:343` to get `(description, arg_descriptions)`.
   - It returns `_create_subset_model(model_name, inferred_model, valid_properties, descriptions=arg_descriptions, fn_description=description)` at ~`base.py:362-368`.
4. `_infer_arg_descriptions` (`base.py:170-201`):
   - It gets `get_type_hints(fn, include_extras=True)`.
   - With `parse_docstring`, it calls `_parse_python_function_docstring`. That uses `inspect.getdoc(fn)` and passes it to `_parse_google_docstring` (`utils/function_calling.py:735`).
   - It then validates the result with `_validate_docstring_args_against_annotations` (`base.py:~153-167`). A documented arg that isn't in the signature raises `ValueError("Arg X in docstring not found in function signature.")`.
   - For any arg without a docstring description, it falls back to `Annotated[...]` metadata via `_get_annotation_description` (`base.py:105`). That function accepts a string annotation or a `FieldInfo.description`.
5. `_parse_google_docstring` (`utils/function_calling.py:735-817`):
   - It splits the docstring on blank lines (`"\n\n"`).
   - The function description is the blocks before the first `Args:` block, joined with spaces. It stops accumulating at `Returns:` or `Example:` blocks, though it keeps scanning for a later `Args:`.
   - In the `Args:` block, each `name: desc` line or `name (type): desc` line becomes an entry. The `(type)` part is stripped.
   - Deeper-indented lines are treated as continuations and appended to the previous arg. The base indent is latched from the first arg line.
   - With `error_on_invalid_docstring=True`, it raises `ValueError("Found invalid Google-Style docstring.")` if there is no docstring, or if the docstring has fewer than `_MIN_DOCSTRING_BLOCKS` blocks or no `Args:` block. This check ignores `run_manager`, `callbacks`, `runtime` and `return`.
6. `_create_subset_model_v2` (`utils/pydantic.py:234`):
   - It creates a new model with one `FieldInfo` per kept field. The description is `descriptions.get(name, field.description)`. Default or `default_factory` and metadata are preserved.
   - It sets `rtn.__doc__ = textwrap.dedent(fn_description or model.__doc__ or "")`. The parsed function description therefore becomes the schema's docstring. `_create_subset_model` at `utils/pydantic.py:284` picks the v1 or v2 variant to match the input model.

**Tool description (`tools/structured.py:267-303`)**
- `description_ = description`.
- If `description is None and not parse_docstring`, it uses `source_function.__doc__`.
- With `parse_docstring=True` and no explicit `description`, `description_` stays `None`, so it falls back to `args_schema.__doc__`. That is the parsed description from step 6, without the `Args:` section. A Pydantic placeholder docstring (containing "A base class for creating Pydantic models") becomes `""`. For a dict schema it uses `args_schema["description"]`.
- If it is still `None`, it raises `ValueError`.
- When no explicit `description` was passed, it applies `textwrap.dedent(...).strip()`.
- An explicit `description` wins over everything, per the precedence documented at `tools/convert.py:~126-131`.

**Uncertainty:** I didn't run any examples. The line numbers marked `~` are approximate because I read them from `sed` ranges or grep matches, not from a numbered listing of that exact line. I also didn't read `_filter_schema_args` or the v1 subset-model path (`_create_subset_model_v1`).