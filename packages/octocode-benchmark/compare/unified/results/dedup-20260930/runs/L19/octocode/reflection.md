1. **Helped:** The `localSearch` for `ModuleDetection`, restricted to `*.go` with test files excluded, found the three files that matter in one call: `compileroptions.go`, `parseoptions.go` and `enummaps.go`. A batched `localFetch` then read `parseoptions.go:1-80` and `compileroptions.go:240-253` in parallel. That covered the default-kind logic and the per-file decision. A final `localFetch` of lines 78-149 finished the indicator helpers.

2. **Did not help:**
- My first `localSearch` failed on validation because I passed `include` and `exclude` as strings rather than arrays. That cost one call.
- The second `localSearch` for `ExternalModuleIndicator` in `parseoptions.go` was mostly redundant, since I read the file anyway.
- I never traced the callers of `GetExternalModuleIndicatorOptions`, or read the body of `GetImpliedNodeFormatForEmitWorker`. The final answer says both were unverified.
- Paths came back as `typescript/tsc/internal/…`, which is not the repo layout I expected. I cited them relative to the checkout's `typescript/` directory, and I'm not sure that matches the real repo paths.

3. **Next time:** I'd get the array parameter types right the first time. I'd also run `lspSearch` callers on `GetExternalModuleIndicatorOptions` and `lspSearch` definition on `GetImpliedNodeFormatForEmitWorker`. That would close the gaps I listed.

4. **Confidence:** High for the core logic, because I read the source directly. Medium for the claim that "implied node format ESNext" covers all the Node16/NodeNext cases, and for the exact path prefix.