**Short answer:** `@tool(parse_docstring=True)` passes the flag through `StructuredTool.from_function` to `create_schema_from_function`. That function builds a Pydantic model from the function signature, then rebuilds it as a subset model. The subset model's field descriptions come from the Google-style `Args:` block, and the docstring's summary becomes the model's `__doc__`. The tool's own description is set separately in `from_function`. All paths below are under `libs/core/langchain_core/`.

**Call chain**
1. `tool()` in `tools/convert.py` forwards `parse_docstring` and `error_on_invalid_docstring` to `StructuredTool.from_function(...)` (`convert.py:326`, the call starts at about `convert.py:317`).
2. When `args_schema` is None and `infer_schema` is true, `from_function` calls `create_schema_from_function(name, source_function, parse_docstring=..., error_on_invalid_docstring=..., filter_args=_filter_schema_args(...))` (`tools/structured.py:259-265`).

**Argument schema** (`create_schema_from_function`, `tools/base.py:263-353`)
- It runs `inspect.signature(func)` and wraps the function with pydantic `validate_arguments` (the v1 variant if the annotations are pydantic v1). The resulting `validated.model` is the base model.
- It builds a list of filtered args:
  - `self` or `cls` for methods, plus `FILTERED_ARGS`.
  - Injected args, when `include_injected` is false.
- It calls `_infer_arg_descriptions(func, parse_docstring=..., ...)` (`base.py:340-345`).
- It drops the placeholder `args` and `kwargs` fields unless the function really has `*args` or `**kwargs`. It also drops `v__duplicate_kwargs` and the filtered args.
- It returns `_create_subset_model(model_name, inferred_model, valid_properties, descriptions=arg_descriptions, fn_description=description)`.
- In `_create_subset_model_v2` (`utils/pydantic.py`):
  - Each field gets `descriptions_.get(name, field.description)`, so the docstring description wins over any existing field description.
  - Defaults, default factories and metadata are preserved.
  - `rtn.__doc__ = textwrap.dedent(fn_description or model.__doc__ or "")`.

**Docstring parsing** (`_infer_arg_descriptions`, `tools/base.py:170-203`)
- It reads the type hints with `get_type_hints(fn, include_extras=True)`.
- With `parse_docstring` on, it calls `_parse_python_function_docstring`. That calls `inspect.getdoc(fn)` and then `_parse_google_docstring` (`base.py:126-148`, defined in `utils/function_calling.py:735`).
- It then runs `_validate_docstring_args_against_annotations`. This raises `ValueError` if the docstring documents an arg that isn't in the signature (`base.py:151-167`).
- For any arg the docstring did not describe, it falls back to `_get_annotation_description(arg_type)`. That picks up a description from `Annotated[...]` metadata, including a `FieldInfo.description` (`base.py:~120-123`).
- Without `parse_docstring`, the description is just `inspect.getdoc(fn) or ""` and there are no per-arg descriptions from the docstring.

**`_parse_google_docstring`** (`utils/function_calling.py:735+`)
- It splits the docstring on `"\n\n"` into blocks.
- Blocks before the first `Args:` block are joined with spaces to form the description. Blocks starting with `Returns:` or `Example:` are skipped, and parsing continues in case `Args:` comes later.
- The `Args:` block is parsed line by line, and deeper-indented lines are treated as continuations of the previous arg's description. I only read the start of this loop, so the exact handling of each `name: text` line is not verified.
- If `error_on_invalid_docstring` is true, it raises `ValueError("Found invalid Google-Style docstring.")` when:
  - there is no docstring, or
  - there are annotated args (excluding `run_manager`, `callbacks`, `runtime` and `return`) but the docstring has too few blocks or no `Args:` block.

**Tool description** (`StructuredTool.from_function`, `tools/structured.py:267-295`)
- An explicit `description=` argument always wins.
- When `parse_docstring` is on, the raw `__doc__` is not used (`if description is None and not parse_docstring`, line 268).
- `description_` therefore stays None and falls through to the `args_schema.__doc__` branch. That `__doc__` is the parsed summary set by `_create_subset_model_v2`.
- The result is dedented and stripped.
- If no description is found, it raises `ValueError("Function must have a docstring if description not provided.")`.

**Uncertainty:** I did not read the tail of the `Args:` parsing loop, the pydantic-v1 subset-model path, or `_filter_schema_args`.