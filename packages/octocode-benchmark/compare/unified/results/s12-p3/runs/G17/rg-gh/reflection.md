1. **Helped:** My one Bash call did most of the work. It looped `gh api -X GET repos/psf/requests/contents/<file>?ref=v2.32.3` over `setup.py`, `adapters.py`, `__version__.py` and `pyproject.toml`, piped through `base64 -d | cat -n | grep`. Pinning the ref to the tag and numbering the lines gave citable `path:line` evidence. The same call also fetched `git/ref/tags/v2.32.3`.

2. **Did not help:**
   - The call printed `/bin/bash: /dev/null: Operation not permitted` three times. The output still looked complete, but I never found out which command caused it.
   - My broad grep with `head -80` was noisy and cut off the `adapters.py` output.
   - For `pyproject.toml` the grep showed only a `minversion` line, so I never read the file.
   - The ref lookup returned the tag object SHA, not the underlying commit SHA. I never dereferenced it.
   - I did not query PyPI metadata, which is what the question literally asked about.

3. **Next time:**
   - Fetch `pyproject.toml` in full.
   - Dereference the tag to the commit with `git/tags/<sha>`.
   - Use narrower greps, or `sed -n` for specific line ranges.
   - Compare against the PyPI JSON API if I can reach it.

4. **Confidence:**
   - **High:** the Python requirement, dependencies, extras, adapter defaults and line numbers, since I saw them directly in the output.
   - **Medium:** completeness. There could be metadata in `pyproject.toml` that I didn't read, and I haven't confirmed the PyPI wheel matches the tag.