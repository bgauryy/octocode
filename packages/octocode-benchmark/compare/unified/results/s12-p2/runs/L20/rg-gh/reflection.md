1. **Helped:** The `grep -rIn "esolveJsonModule" . --include=*.go ... | grep -v testdata` call in `tsc/` found the definition and every caller at once. The follow-up `sed -n` and `grep "GetResolveJsonModule"` call confirmed the default logic and the full caller list in one pass.

2. **Did not help:**
   - The first call used `rg`, which isn't installed, so it failed.
   - Every call printed `/dev/null: Operation not permitted`. This was harmless but noisy.
   - The first `git log -1` failed for the same reason, so I never confirmed the checkout was at the pinned commit 4f5ddae224.
   - The localized `*.generated.json` files flooded the grep output, and I should have excluded them.
   - I never ran the test baselines or opened `GetEmitModuleKind` or `GetModuleResolutionKind`.

3. **Next time:** Use `grep -rIn --include=*.go --exclude-dir=testdata --exclude-dir=loc` from the start. Verify the commit by reading `.git/HEAD` directly. Also open the two getter dependencies, and grep for tests that cover the default.

4. **Confidence:** Medium-high. The caller list and line numbers come straight from the output I saw. The pinned commit is unverified, and the claims about test impact and the getter dependencies are inferred.