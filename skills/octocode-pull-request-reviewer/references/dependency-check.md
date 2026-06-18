# Octocode MCP Dependency Check

<dependency_gate priority="maximum">
**STOP. Verify Octocode MCP tools are available before proceeding.**

### Pre-Conditions
- [ ] Review target determined (PR Mode or Local Mode — see Review Target Detection)

### Actions — PR Mode (REQUIRED when reviewing a remote PR)
1. **Test MCP availability**: Call `ghHistoryResearch(type:"prs", keywordsToSearch:["test"], limit:1)` with a minimal query
   - **IF** tool responds successfully → **THEN** proceed
   - **IF** tool fails or is not found → **THEN** STOP and inform user:
     ```
     Octocode MCP is required for PR reviews but is not available.
     Please ensure the Octocode MCP server is running.
     Install: https://octocode.ai
     ```

### Actions — Local Mode (REQUIRED when reviewing local changes)
1. **Test local tools availability**: Call `localViewStructure` on the workspace root
   - **IF** tool responds successfully → **THEN** local tools are enabled, proceed
   - **IF** tool fails → **THEN** STOP and inform user to set `ENABLE_LOCAL=true` (see Review Target Detection)
2. **Test git availability**: Run `git status` to verify the workspace is a git repository
   - **IF** succeeds → **THEN** proceed
   - **IF** fails → **THEN** STOP and inform user: "This directory is not a git repository."

### Required Tools — PR Mode

| Tool | Fallback |
|------|----------|
| `ghHistoryResearch` | NONE — review cannot proceed |
| `ghGetFileContent` | NONE — review cannot proceed |
| `ghSearchCode` | NONE — review cannot proceed |
| `ghViewRepoStructure` | NONE — review cannot proceed |
| `npmSearch` | Skip external package analysis |

### Required Tools — Local Mode

| Tool | Fallback |
|------|----------|
| `localSearchCode` | NONE — review cannot proceed |
| `localGetFileContent` | NONE — review cannot proceed |
| `localViewStructure` | NONE — review cannot proceed |
| `localFindFiles` | NONE — review cannot proceed |
| `lspGetSemantics(type="definition")` | Fall back to `localSearchCode` |
| `lspGetSemantics(type="references")` | Fall back to `localSearchCode` |
| `lspGetSemantics(type="callers"/"callees"/"callHierarchy")` | Fall back to `localSearchCode` |
| Shell: `git status`, `git diff` | NONE — review cannot proceed |

### Gate Check — PR Mode
- [ ] `ghHistoryResearch` responded successfully
- [ ] PR number/URL is valid and accessible

### Gate Check — Local Mode
- [ ] `ENABLE_LOCAL=true` is configured (local tools respond)
- [ ] Workspace is a git repository (`git status` succeeds)
- [ ] At least one of: staged changes, unstaged changes, or untracked files exist

### FORBIDDEN
- **PR Mode**: Proceeding if `ghHistoryResearch` is unavailable
- **Local Mode**: Proceeding if local tools are disabled (`ENABLE_LOCAL=false`)
- Using shell commands for code reading/search when Octocode MCP tools are available

### ALLOWED
- **PR Mode**: Octocode MCP `github*` tool calls
- **Local Mode**: Octocode MCP `local*` + `lsp*` tool calls + shell `git` commands (status, diff, log only)

### On Failure
- **IF** Octocode MCP unavailable → **THEN** STOP, inform user, EXIT
- **IF** partial tools available → **THEN** STOP, list missing tools, EXIT
- **IF** PR not found → **THEN** STOP, ask user for correct PR number/URL
- **IF** local tools disabled → **THEN** STOP, instruct user to set `ENABLE_LOCAL=true`, EXIT
- **IF** no local changes found → **THEN** STOP, inform user: "No changes detected. Stage or modify files first."
</dependency_gate>
