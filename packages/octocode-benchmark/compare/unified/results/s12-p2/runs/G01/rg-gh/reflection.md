1. **Helped:** The first `gh api pulls/16403` plus `/files` call gave the PR body and the file list in one shot. The `/files` call filtered with `jq` to the patches for `applications.py`, `background.py`, `routing.py` and `pyproject.toml` was the best step. It showed the runtime changes directly. Fetching `_runtime.py` raw at the head SHA through the contents API, with `cat -n`, gave me line numbers to cite.

2. **Did not help:**
- Every Bash call printed `/dev/null: Operation not permitted`. It was harmless but noisy.
- My second call, a `cd /tmp` loop over `_runtime` and `_api`, failed with exit 1 and no content.
- `rg` is not installed, despite the prompt saying it is. I had to fall back to `grep`.
- I never read `_api.py`, most of `_asgi.py`, or any tests. That left the data-exposure, span-attribute and `exclude` questions unanswered.
- The patch output for the docs page was truncated by `head`.

3. **Next time:** I would fetch `_api.py` and `_asgi.py` in full, in parallel with the diffs, and check `grep` availability first. I would also search the PR's review comments and linked issues, which I never did.

4. **Confidence:** Medium. The behavior claims about `__call__`, auto-configure and the new spans are well supported by diffs I read. The data-exposure and double-instrumentation points are partly inferred from code I only skimmed, and I said so in the answer.