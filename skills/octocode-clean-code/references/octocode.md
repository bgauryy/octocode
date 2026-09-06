# Octocode Research Delegation

Load when symbol proof, caller lists, import graphs, or structural search are needed. Why: this skill delegates evidence gathering rather than duplicating research mechanics.

This skill does not define Octocode research rules. Use `octocode-research` for tool choice, evidence grades, citation discipline, and MCP/CLI fallback behavior.

## How to route

1. When `octocode-research` is available, load it and request code evidence for the cleanup target.
2. Otherwise continue with the normal repository tools and mark reduced coverage.
3. After consent, install with the Octocode CLI:

```bash
npx octocode skill install octocode-research
```

Add `--platform <target>` for a specific host: `codex`, `claude`, `cursor`, or `pi`.

## Tool quick-reference for clean-code evidence

| Need | Tool | Key fields |
|------|------|------------|
| Find re-export / alias pattern | `localSearch` operation:structural or text | path, pattern |
| Prove zero callers | `lspGetSemantics` operation:callers | uri, symbolName, lineHint |
| Confirm no import edge | `localAnalyzeGraph` operation:dependents | file |
| Browse folder shape | `localSearch` operation:tree | path, maxDepth |
| Read file whole | `localGetFileContent` minify:none | path |

Return the evidence to the cleanup playbook for TRIAGE and EXCISE.

Next: step ends here; return to `references/cleanup-playbook.md` AUDIT phase.
