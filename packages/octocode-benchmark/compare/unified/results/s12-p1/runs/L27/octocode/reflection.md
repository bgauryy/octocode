**1. Helped:**
- The first `localSearch` was the fastest step. One regex over `config/config.go` and `go.mod` gave the field declaration (`config.go:851`) and the root pin (`go.mod:69`, v0.71.0).
- `ghGetFileContent` on `model/metric.go` with `branch: v0.71.0` and a line range gave the exact enum, `Set`, and validators at the pinned version.

**2. Did not help:**
- The `ghSearchCode` call read the default branch (`ca4f6e15`), not v0.71.0. It was a wasted call, and its output could have been mistaken for the pinned version's code.
- My `localSearch` for `^replace` ran against `config.go` instead of `go.mod`, so it found nothing useful. I never checked `go.mod` for a `replace` directive and flagged that gap in the answer.
- I didn't read `isValidLegacyRune`, so the exact legacy metric-name character set is stated from memory.
- I didn't read `config.go:1002-1008` directly. I only saw its matched lines in the first search.

**3. Next time:**
- Skip `ghSearchCode`. Go straight to `ghGetFileContent` at the tag.
- Read the pinned file range in `go.mod`, including any `replace` block.
- Fetch `isValidLegacyRune` in the same call as the enum.

**4. Confidence:**
- **High** for the module, the version, and the three enum values with their label-name and UTF-8 rules, because I saw them at the pinned tag.
- **Medium** for the legacy metric-name character set, since I never read the helper and the pattern comes from memory.
- **Medium** that no `replace` directive overrides the pin, since I never searched `go.mod` for one.