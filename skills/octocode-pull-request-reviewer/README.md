<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">

  <h1>Octocode Pull Request & Code Reviewer</h1>

  <p><strong>Expert code review for PRs and local changes</strong></p>
  <p>Architectural analysis • Defect detection • Security scanning • LSP-powered flow tracing</p>

  [![Skill](https://img.shields.io/badge/skill-agentskills.io-purple)](https://agentskills.io/what-are-skills)
  [![License](https://img.shields.io/badge/license-MIT-blue)](https://github.com/bgauryy/octocode-mcp/blob/main/LICENSE)

</div>

---

## What It Does

This skill turns your AI agent into an expert code reviewer that handles both **remote Pull Requests** and **local changes** (staged/unstaged). It uses Octocode MCP tools for deep code forensics — tracing call hierarchies, finding all symbol usages, and mapping blast radius of changes.

| Mode | Input | Tools Used |
|------|-------|------------|
| **PR Mode** | PR number, URL, or branch | `github*` tools (+ `local*`/`lsp*` if workspace matches) |
| **Local Mode** | "review my changes", "review staged" | `local*` + `lsp*` tools + shell `git` commands |

---

## Installation

```bash
npx add-skill https://github.com/bgauryy/octocode-mcp/tree/main/skills/octocode-pull-request-reviewer
```

---

## Requirements

### For PR Mode (Remote Pull Requests)

- **Octocode MCP server** running with GitHub/GitLab authentication
- See [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md)

### For Local Mode (Local Changes) — `ENABLE_LOCAL=true`

Local Mode requires Octocode MCP **local tools** and **LSP tools** to be enabled. These are disabled by default.

**Enable them:**

```bash
# Option 1: Environment variable
export ENABLE_LOCAL=true

# Option 2: In your Octocode config file (~/.octocode/config.json)
{
  "local": {
    "enabled": true
  }
}
```

**What `ENABLE_LOCAL` unlocks:**

| Tool Category | Tools | Purpose |
|---------------|-------|---------|
| **Local Filesystem** | `localSearchCode`, `localViewStructure`, `localFindFiles`, `localGetFileContent` | Search, explore, and read code in your workspace |
| **LSP Semantic** | `lspGotoDefinition`, `lspFindReferences`, `lspCallHierarchy` | Jump to definitions, find all usages, trace call chains |

> **Full documentation:** [Local Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md) | [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md)

**Verify it's working:**

Ask your agent: *"Can you check if local tools are available?"* — the skill will call `localViewStructure` on your workspace root. If it responds, you're good.

---

## Usage

### Review a Pull Request

```
"Review PR #123"
"Review https://github.com/org/repo/pull/456"
"Is this PR safe to merge?"
```

### Review Local Changes

```
"Review my changes"
"Review my staged changes"
"Review local changes"
"Review my diff"
```

The agent will:
1. Ask for any review guidelines or context files
2. Collect your changes (`git status` + `git diff`)
3. Present a TL;DR summary and ask for focus areas
4. Perform deep analysis using local + LSP tools
5. Deliver prioritized findings with `file:line` citations and code fixes

---

## How Local Tools Check Your Repo

When reviewing local changes, the skill uses a **funnel approach** — progressively narrowing from broad discovery to precise semantic analysis:

```
DISCOVER          SEARCH           LSP SEMANTIC        READ
    │                 │                 │                  │
    ▼                 ▼                 ▼                  ▼
 Project           Find symbols     Trace callers,     Read
 structure         + get lineHint   usages, defs       implementation
```

### Step 1: Collect Changes (Shell git)

```bash
git status                    # What files changed?
git diff --staged             # What's staged?
git diff                      # What's unstaged?
git branch --show-current     # Which branch?
```

### Step 2: Understand Structure (Local Tools)

```
localViewStructure(path="/workspace/src", depth=2)
→ See the project layout, understand where changed files fit

localFindFiles(path="/workspace", modifiedWithin="1d")
→ Find recently modified files
```

### Step 3: Search & Discover (Local Tools)

```
localSearchCode(pattern="changedFunction", path="/workspace/src", filesOnly=true)
→ Find all files containing the changed symbol, get lineHint

localSearchCode(pattern="TODO|FIXME", path="/workspace/src/changed-file.ts")
→ Find TODOs in changed files
```

### Step 4: Semantic Analysis (LSP Tools)

LSP tools provide **language-aware** analysis — they understand types, scopes, and call relationships.

```
lspCallHierarchy(
  symbolName="changedFunction",
  lineHint=42,              ← from localSearchCode!
  direction="incoming"
)
→ Who calls this function? Will they break?

lspFindReferences(
  symbolName="ChangedType",
  lineHint=10               ← from localSearchCode!
)
→ Every usage of this type across the codebase

lspGotoDefinition(
  symbolName="importedHelper",
  lineHint=5                ← from localSearchCode!
)
→ Jump to where this imported symbol is defined
```

### Step 5: Read Implementation (Local Tools — LAST)

```
localGetFileContent(
  path="/workspace/src/auth/middleware.ts",
  matchString="authenticate",
  matchStringContextLines=20
)
→ Read the relevant code section with surrounding context
```

---

## Review Domains

The skill evaluates changes across 7 domains:

| Domain | What It Catches |
|--------|----------------|
| **Bug** | Runtime errors, logic flaws, null access, race conditions |
| **Security** | Injection, XSS, data exposure, auth bypass |
| **Architecture** | Pattern violations, coupling, circular deps |
| **Performance** | O(n²), blocking ops, memory leaks |
| **Code Quality** | Naming, conventions, magic numbers |
| **Error Handling** | Swallowed exceptions, unclear messages |
| **Flow Impact** | Breaking callers, altered return values, changed data flow |

---

## Review Flow

```
Phase 1           Phase 2          Phase 3              Phase 4          Phase 5        Phase 6
GUIDELINES    →   CONTEXT     →   USER CHECKPOINT  →   ANALYSIS    →   FINALIZE   →   REPORT
                                                                                        
Ask for docs      PR: github*     Present TL;DR        Deep dive        Dedupe         Summary +
& guidelines      Local: git      Ask focus areas       local* + lsp*   Verify vs      Document
                  diff + status                         tools            guidelines
```

The agent **stops at Phase 3** to ask you what to focus on before diving deep.

---

## Output

Findings are delivered as a prioritized list with:
- Exact `file:line` location
- Confidence level (HIGH/MED)
- Problem description
- Actionable code fix (diff format)

Optionally saved to:
- **PR Mode:** `.octocode/reviewPR/{session}/PR_{number}.md`
- **Local Mode:** `.octocode/reviewLocal/{session}/REVIEW_{branch}_{timestamp}.md`

---

## References

| Document | Description |
|----------|-------------|
| [SKILL.md](./SKILL.md) | Full agent protocol (phases, gates, rules) |
| [references/flow-analysis-protocol.md](./references/flow-analysis-protocol.md) | LSP tracing recipes (6 recipes for local + remote) |
| [references/domain-reviewers.md](./references/domain-reviewers.md) | Domain detection matrix and priority levels |
| [references/dependency-check.md](./references/dependency-check.md) | Pre-flight gates and failure handling |
| [references/execution-lifecycle.md](./references/execution-lifecycle.md) | Detailed Phase 1, 2, 3, 5, 6 playbooks |
| [references/review-guidelines.md](./references/review-guidelines.md) | Confidence model and changed-code mindset |
| [references/verification-checklist.md](./references/verification-checklist.md) | Full delivery checklist |
| [references/parallel-agent-protocol.md](./references/parallel-agent-protocol.md) | Multi-agent swarm strategy |
| [references/output-template.md](./references/output-template.md) | Report format template (PR + Local) |
| [Local Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md) | Full local + LSP tool documentation |
| [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) | `ENABLE_LOCAL` and other settings |

---

## License

MIT License © 2026 Octocode

See [LICENSE](https://github.com/bgauryy/octocode-mcp/blob/main/LICENSE) for details.
