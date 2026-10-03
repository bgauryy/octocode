**1. Helped:**
- The first parallel pair was `gh pr view 16403 --json ...` and `gh pr diff`. The view call gave the file list, the merged state, the head SHA and the PR body. The diff call gave `routing.py` and `applications.py`.
- Saving the diff to `/tmp/p.diff` let me slice it with `awk` and `sed`. That is how I read `_runtime.py` and `pyproject.toml` in full.

**2. Did not help:**
- The first `head -c 30000` truncated the diff in the middle of `_api.py`, so I had to re-fetch it.
- The diff-relative line numbers (from `sed -n` slices of `p.diff`) can't be cited as `path:line` at the pinned commit.
- I never read `_api.py` or most of `_asgi.py`. My `grep` over `_asgi.py` was a keyword filter, so I saw only matching lines.
- I never checked out the repo at the pinned commit. `gh` can't give file lines at a commit.

**3. Next time:**
- Fetch the diff to a file on the first call.
- Read `_api.py` fully, since `_unconfigured` and `_operation` decide the cost of the no-SDK path.
- Use `gh api` with the contents endpoint at the head SHA to get real file line numbers.
- Check the PR's review comments, which I never looked at.

**4. Confidence:** medium. I read the behavior summary, the `_runtime.py` code and the dependency changes directly. The no-SDK cost, the redaction list and how `exclude` works for mounts are unverified, and I said so in the answer.