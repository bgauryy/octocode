1. **Helped:** The second call, a `grep -rnE` for `def static`, `static(`, `import.*static` and `static as` over `django/*.py`. It listed all three `static` definitions and every importer and caller in one pass. The third call, `sed -n 170,185p` plus a grep on `templatetags/static.py`, confirmed which definition does the path-to-URL job.

2. **Did not help:**
   - The first call failed because `rg` isn't installed, so I had to repeat the search with grep.
   - Every call printed `/dev/null: Operation not permitted`. It was noise, though the output was still usable.
   - I never opened the callers in `widgets.py` or `admin_list.py`, and I never confirmed the checkout is at the pinned commit.
   - I named `Media.absolute_path` for line 228 without seeing its `def` line. I inferred it from the docstring, which was an overclaim.

3. **Next time:** Start with grep, since `rg` is absent. Read about 5 lines around each hit (`sed -n`) to get the enclosing function names and confirm the callee. Also grep for `handle_simple` and `templatetags.static` to rule out indirect callers. Run `git rev-parse HEAD` if allowed.

4. **Confidence:** Medium-high that the three call sites are the right answer, because the import lines and the call lines all matched. Medium on the method name at line 228 and on the exact role of the line 187 call, since I never read those lines.