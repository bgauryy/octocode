1. **Helped:** My first working `localSearch` for `GetResolveJsonModule` (restricted to `*.go`) found the definition and its direct callers in one call. The second `localSearch`, for `ResolveJsonModule` under `tsc/internal`, found the indirect users (`GetSupportedExtensionsWithJsonIfResolveJsonModule` callers), the tests and `project.go`. The `localFetch` of `compileroptions.go:260-290` showed the default logic. `structureSearch` with the `go` extension filter confirmed the Go sources are in this repo.

2. **Did not help:**
   - My first parallel batch failed validation because I left out the `queries` wrapper. That cost one round trip.
   - Returned paths were prefixed `typescript/tsc/...`, while the real paths in the checkout are `tsc/...`. This was confusing, though `localFetch` still worked.
   - I did not use `lspSearch` references for semantic confirmation, so the caller list is text-based.
   - I did not inspect testdata or baselines, so I can't name affected baseline files.

3. **Next time:** I would wrap `queries` correctly from the start. I would run `lspSearch` callers/references on `GetResolveJsonModule`. I would add a `localSearch` of the baselines and `testdata` for `resolveJsonModule`. I would read `showconfig.go` around line 47 to check what the `dependencies` list does.

4. **Confidence:** Medium-high. The code paths and line numbers come directly from source I read. The test and baseline impact is unverified, and the `showconfig` dependency remark is inferred from one line.