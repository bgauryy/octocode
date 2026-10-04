**Helped:**
- The `localSearch` for `merge_content` across `libs/core` found every call site, the export and the test in one call. That is what produced the affected-versus-unaffected split.
- The batched `localFetch` with line ranges confirmed the signature (`base.py:366-369`) and showed the two variadic callers (`base.py:453`, `ai.py:665`) and the test (`test_messages.py:1109`).

**Did not help:**
- My test range (1060-1112) started about 20 lines too early and pulled in unrelated tests.
- The `base.py` fetch returned a "lines 373-435 omitted" gap, so I never read the function body. I stated that in the answer instead of re-fetching.
- I did not use `lspSearch` references. Text search can miss aliased imports or dynamic uses, and I did not rule them out.
- I did not check other packages in the repo for callers. Scope was `libs/core`, which matches the question, but external callers are only inferred.

**Next time:**
- Run `lspSearch` references on `merge_content` alongside the text search to confirm completeness.
- Fetch the function body directly (`373-435`) to see how `contents` is iterated.
- Use tighter ranges.

**Confidence:** medium-high. The call-site inventory and line numbers come straight from fetched output. The weaker parts are the body logic I never read and the unchecked callers outside `libs/core`.