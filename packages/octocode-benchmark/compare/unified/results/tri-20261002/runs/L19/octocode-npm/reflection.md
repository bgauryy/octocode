**1. Helped:** The repo-wide `localSearch` text query for `ModuleDetection` (excluding `node_modules`) was the key call. It surfaced `core/compileroptions.go`, `ast/parseoptions.go` and `tsoptions/enummaps.go` in one go. The follow-up `localGetFileContent` with `fullContent` on `parseoptions.go`, plus line windows on `compileroptions.go`, then settled the logic.

**2. Did not help:**
- My first search used a guessed `internal/` path and failed. The Go code is under `tsc/internal`, not the repo root, and I only learned that after a `tree` and a miss on `packages/`.
- The wide search returned lots of noise: baselines, the `.ts` fixtures and generated enums, which is about 30 files.
- The `fullContent` read of `parseoptions.go` returned no line numbers. I then cited `parseoptions.go:46-56` and `:62-86`, and those ranges were my estimates, not seen. The `:19-42` range and the `compileroptions.go` ranges are backed by search hits or numbered windows. My answer should have flagged the estimated ones.
- I also never traced callers of `SetExternalModuleIndicator`.

**3. Next time:** Run the `tree` first, then search `tsc/internal` with `include: *.go`. Use `startLine`/`endLine` windows so every cited line is verified.

**4. Confidence:** High on the logic, since I read it directly. Medium on the two estimated line ranges, and that is the part to correct.