# Changelog

## [Unreleased]

### Fixed
- **`local*` tools: clearer path-denial errors ([#450]).** A path outside the
  allowed roots that contains no symlinks no longer reports a misleading
  `Symlink target '…' is outside allowed directories`. It now says
  `Path '…' is outside allowed directories (allowed: …)`, listing the configured
  roots so agents can self-correct in one step. The `Symlink target …` wording is
  reserved for genuine symlink escapes (a path inside a root whose target resolves
  outside). Applies uniformly across `localSearchCode` (text + AST/structural),
  `localFindFiles`, `localViewStructure`, `localGetFileContent`, and
  `lspGetSemantics`.
- **`.octocoderc` `local.allowedPaths` now takes effect.** The file-config
  allowlist was validated but never reached the path validator, so only the
  `ALLOWED_PATHS` env var worked. `local.allowedPaths` now adds roots on top of
  the always-allowed home directory, matching the env var (CLI and MCP).
- **Docs: corrected `ALLOWED_PATHS` / `allowedPaths` semantics.** Removed the
  contradictory "empty = unrestricted / all paths allowed" wording; empty means
  **home directory only** (paths outside home are denied). Clarified that
  `workspaceRoot` is the base for resolving relative paths, not itself an allowed
  root.

### Notes
- The path allow-list remains **on by default** and cannot be disabled — only
  widened (`ALLOWED_PATHS` env or `local.allowedPaths`) or removed entirely with
  the whole local surface via `ENABLE_LOCAL=false`.

[#450]: https://github.com/bgauryy/octocode/issues/450
