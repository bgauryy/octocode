**Answer:** With `parse_docstring=True`, `@tool` builds a Pydantic args schema from the function signature. It takes per-argument descriptions and the tool's summary description from the Google-style docstring. Two details are not verified: `_create_subset_model_v2` was not read, and the lines that finish the `Args:` parsing loop (after `function_calling.py:800`) were not read.

**Flow (all paths under `libs/core/langchain_core/`):**

1. **Decorator to tool.** `tools/convert.py:315-327` calls `StructuredTool.from_function(...)`. It passes `parse_docstring`, `error_on_invalid_docstring` and `args_schema=schema`. This branch runs when `infer_schema or args_schema is not None`. Otherwise it builds a plain `Tool` (`convert.py:~341`).

2. **Schema creation.** `tools/structured.py:~259-265`: if no `args_schema` was given and `infer_schema` is true, `from_function` calls `create_schema_from_function(name, source_function, parse_docstring=..., error_on_invalid_docstring=..., filter_args=_filter_schema_args(...))`.

3. **Base model from the signature.** `create_schema_from_function` (`tools/base.py:~262-`) runs `inspect.signature`. It wraps the function in pydantic's `validate_arguments`, using the v1 variant if the annotations are v1 models (`_function_annotations_are_pydantic_v1`). It takes `validated.model` as the inferred model.
   - It filters `self`/`cls` for methods, the default `FILTERED_ARGS`, and the placeholder `args`/`kwargs`/`v__duplicate_kwargs` fields.
   - When `include_injected` is false, injected args are also filtered out.

4. **Descriptions.** `_infer_arg_descriptions` (`base.py:~171-200`) does this:
   - It gets `get_type_hints(fn, include_extras=True)`.
   - If `parse_docstring` is true, it calls `_parse_python_function_docstring`, which runs `inspect.getdoc(fn)` and passes the result to `_parse_google_docstring` (`utils/function_calling.py:735`).
   - Otherwise the description is the full docstring and there are no per-argument descriptions.
   - It then validates that every documented arg exists in the annotations, and raises `ValueError` if not.
   - For any arg still without a description, it falls back to `Annotated[...]` metadata: a string annotation or a `FieldInfo.description` (`_get_annotation_description`).

5. **Docstring parser** (`utils/function_calling.py:735-800+`):
   - It splits the docstring on blank lines (`"\n\n"`).
   - The description is all leading blocks before `Args:`, skipping blocks after `Returns:`/`Example:`. They are joined with a space and stripped.
   - The `Args:` block is parsed line by line into `{arg: description}`. Deeper-indented lines count as continuations of the previous argument.
   - With `error_on_invalid_docstring`, it raises `ValueError("Found invalid Google-Style docstring.")` in two cases: the docstring is missing, or there are no `Args:` block and fewer than the minimum number of blocks. `run_manager`, `callbacks`, `runtime` and `return` are ignored in that check.

6. **Final model.** `base.py:~355-361` returns `_create_subset_model(model_name, inferred_model, valid_properties, descriptions=arg_descriptions, fn_description=description)` (`utils/pydantic.py:284`). It dispatches to the v1 or v2 variant. The subset model carries the per-field descriptions and the model-level description.

7. **Tool description.** In `structured.py:~267-292`:
   - `description_` is the explicit `description` if given.
   - The raw `__doc__` fallback is skipped when `parse_docstring` is true (`if description is None and not parse_docstring`).
   - So `description_` stays `None`, and the tool description comes from `args_schema.__doc__`, which is the parsed `fn_description`.
   - If there is still no description, it raises `ValueError("Function must have a docstring if description not provided.")`.
   - When the description came from the docstring, it is `textwrap.dedent(...).strip()`-ed.

**Uncertainty:** the step 7 description flow is inferred from `from_function` plus `_create_subset_model(fn_description=...)`. I did not read the v2 subset-model body to confirm that it sets the model's `__doc__` from `fn_description`. Line numbers marked `~` were estimated from the ranges I read, not from exact anchors.