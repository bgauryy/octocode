**Short answer:** `@tool(parse_docstring=True)` (in `tools/convert.py`) calls `StructuredTool.from_function`. That calls `create_schema_from_function`, which builds a Pydantic model from the function signature. The docstring is parsed as Google style, and the parsed text becomes the model's per-field descriptions and the model's `__doc__`. I did not read the top of the decorator body (before about line 290), so the `tool_name` and `tool_description` setup is unseen.

**1. Decorator to `from_function`**
- In `tools/convert.py`, the decorator forwards `parse_docstring` and `error_on_invalid_docstring` to `StructuredTool.from_function(...)` (around lines 315–328). This applies when `infer_schema or args_schema is not None`.
- `from_function` (`tools/structured.py`, about lines 255–265) does this when `args_schema is None and infer_schema`:
  - It calls `create_schema_from_function(name, source_function, parse_docstring=..., error_on_invalid_docstring=..., filter_args=_filter_schema_args(source_function))`.

**2. Building the argument schema** (`create_schema_from_function`, `tools/base.py`, about lines 255–360)
- It gets `inspect.signature(func)`.
- It wraps the function in `validate_arguments_v1` if the annotations are Pydantic v1, otherwise `validate_arguments`. Both use `_SchemaConfig`, which sets `extra="forbid"` and `arbitrary_types_allowed=True`. The resulting `validated.model` is the inferred model.
- It builds the list of filtered args:
  - The default is `FILTERED_ARGS = ("run_manager", "callbacks")`.
  - `self` or `cls` is added when the function is a method (its qualified name contains a dot).
  - Injected args are added when `include_injected` is false.
- It calls `_infer_arg_descriptions(func, parse_docstring=..., error_on_invalid_docstring=...)`.
- It drops Pydantic's placeholder fields (`args`/`kwargs` when the function has no `*args`/`**kwargs`, and `v__duplicate_kwargs`) and any filtered args.
- It returns `_create_subset_model(model_name, inferred_model, valid_properties, descriptions=arg_descriptions, fn_description=description)`.

**3. Description and argument descriptions** (`_infer_arg_descriptions`, `tools/base.py`, about lines 168–205)
- It calls `get_type_hints(fn, include_extras=True)`.
- With `parse_docstring` on, it calls `_parse_python_function_docstring`, which runs `inspect.getdoc(fn)` through `_parse_google_docstring` in `utils/function_calling.py`.
  - The parser splits the docstring on `"\n\n"`. The text blocks before `Args:` are joined with spaces to form the description. Blocks starting with `Returns:` or `Example:` are skipped, and an `Args:` block after them is still found.
  - It parses the `Args:` block line by line, splitting on the first `:`. A `name (type)` prefix is reduced to `name`. Lines indented deeper than the first argument line are continuations and are appended to the previous argument's description.
  - With `error_on_invalid_docstring`, it raises `ValueError("Found invalid Google-Style docstring.")`. That happens when there is no docstring, or when a non-filtered annotated arg exists and there is no second block starting with `Args:`. The filter excludes `run_manager`, `callbacks`, `runtime` and `return`.
- `_validate_docstring_args_against_annotations` then raises `ValueError` if the docstring documents an arg that is not in the function's annotations.
- For any annotated arg with no docstring description, it falls back to `_get_annotation_description`. That takes the first string metadata or `FieldInfo.description` from an `Annotated[...]` type.
- Without `parse_docstring`, the description is the whole `inspect.getdoc(fn)` and the argument descriptions come only from `Annotated` metadata.

**4. Applying it to the model** (`_create_subset_model_v2`, `utils/pydantic.py`, about lines 274–310)
- For each kept field it uses `descriptions_.get(field_name, field.description)`, so the docstring description wins. It rebuilds a `FieldInfo` with that description, keeping the default or `default_factory` and any metadata.
- It sets `rtn.__doc__ = textwrap.dedent(fn_description or model.__doc__ or "")`, so the parsed description becomes the model docstring.
- The v1 variant (`_create_subset_model_v1`) does the same by setting `field.field_info.description`.

**5. The tool's own description** (`from_function`, `structured.py`, about lines 268–298)
- If `description` is `None` and `parse_docstring` is on, it does not use `source_function.__doc__`, which it only does when `not parse_docstring`.
- It instead takes the description from the `args_schema`'s `__doc__`, which is the parsed summary. If that doc contains "A base class for creating Pydantic models", it is treated as empty. For a dict schema it uses `args_schema["description"]`.
- If the description is still `None`, it raises `ValueError("Function must have a docstring if description not provided.")`.
- If the user passed `description` explicitly, that is used as given. Otherwise it is dedented and stripped.

**Uncertainty:** line numbers are approximate. I read the code in ranges and the tool output did not number each line, so the ranges above are my estimates. The parser's exact location is `utils/function_calling.py` lines 759–820.