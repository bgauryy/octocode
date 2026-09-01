# Octocode Research Delegation

Load when skill discovery or comparison needs local workspace, GitHub, package, or code research. Why: this skill judges/installs skills — it does not own Octocode research rules.

Use `octocode-research` for router, tool choice, evidence grades, citations, and Octocode MCP/CLI fallback.

1. IF `octocode-research` is installed THEN load it for local and external research; it owns evidence routing.
2. ELSE IF the `octocode` CLI or Octocode MCP tools are available THEN use them directly:
   - local: `octocode tools localSearch --queries '{"operation":"text","path":"<path>","searchText":"<query>"}' --compact`
   - GitHub: `octocode tools ghSearch --queries '{"operation":"code","keywords":["<query>"],"owner":"<owner>","repo":"<repo>"}' --compact`
   - packages: `octocode tools npmSearch --queries '{"packageName":"<package>"}' --compact`
   Read `octocode tools <name> --scheme` before any raw tool call.
3. ELSE point to https://github.com/bgauryy/octocode/tree/main/skills/octocode-research. Install only with user approval: `npx octocode skill --name octocode-research` (add `--platform <host>` for a specific host).

`octocode skill --list` discovers official installable skills; `octocode-research` covers local, GitHub, npm, PR, and history research.

Return found skill folders here for review, quality scoring, adaptation, install gating, and recommendations.

Next: when fanning out load `references/search-playbook.md`; after inspection load `references/quality-rubric.md`.
