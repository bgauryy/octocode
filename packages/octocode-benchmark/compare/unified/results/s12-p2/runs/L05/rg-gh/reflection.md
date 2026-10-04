1. **Helped:** The `grep -rn "merge_content" . --include=*.py` call found the definition, every call site and the tests in one pass. The `sed -n` calls on `base.py`, `ai.py` and `test_messages.py` then confirmed the signature and which call sites are variadic.

2. **Did not help:** The first call used `rg`, which isn't installed ("command not found"), so I had to rerun the search with `grep`. Every Bash call printed `/dev/null: Operation not permitted`. That is harmless noise, but it makes real failures harder to spot. I never viewed the `.pyi`-style structure of `__init__.py`.

3. **Next time:** I would check `command -v rg` first, or just use `grep`. I would also `sed` the `__init__.py` lines around 19, 123 and 139, and the `def` line above `ai.py:655`.

4. **Confidence:** Medium-high on the core finding. The two variadic call sites (`base.py:453`, `ai.py:665`), the three two-argument sites and the test call are all directly seen. Several details are inferred rather than seen:
   - The `TYPE_CHECKING` and lazy-import characterization of `__init__.py`.
   - The name `add_ai_message_chunks`.
   - The `~381` loop line.

   I also did not search other packages in the monorepo.