<div align="center">
  <img src="https://github.com/bgauryy/octocode/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">

  <h1>Octocode Design</h1>

  <p><strong>Dynamic design-system and UI architecture skill for client apps</strong></p>
  <p>Project discovery • Design specification • Accessibility/performance guardrails • Implementation map</p>

  [![Skill](https://img.shields.io/badge/skill-agentskills.io-purple)](https://agentskills.io/what-are-skills)
  [![License](https://img.shields.io/badge/license-MIT-blue)](https://github.com/bgauryy/octocode/blob/main/LICENSE)
</div>

---

## What It Does

`octocode-design` analyzes a project and generates a dynamic `DESIGN.md` that is used as a single source of truth for UI decisions.

It supports two modes:

| Mode | Intent | Outcome |
|------|--------|---------|
| **Existing project** | Document and improve an existing UI system | `DESIGN.md` + prioritized improvement plan |
| **New project** | Define a design system from scratch | `DESIGN.md` with constraints for first implementation |

The skill is system-level, not only visual. It covers:
- visual language and styling strategy
- component architecture and interaction states
- framework constraints and implementation mapping
- accessibility, performance, responsive behavior, and SEO baseline

---

## Requirements

- Octocode MCP available in your client.
- Local tools enabled (`ENABLE_LOCAL=true`) for project-level analysis.

See:
- [Authentication Setup](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)
- [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md)

---

## Tools Surface

### Local tools
- `localViewStructure`
- `localFindFiles`
- `localSearchCode`
- `localGetFileContent`

### LSP tools
- `lspGotoDefinition`
- `lspFindReferences`
- `lspCallHierarchy`

### Octocode repository/package tools
- `npmSearch`
- `ghSearchRepos`
- `ghViewRepoStructure`
- `ghSearchCode`
- `ghGetFileContent`
- `ghSearchPRs`

---

## What It Checks (Octocode-Driven)

- skill/reference alignment and path integrity
- project type detection (existing UI vs new UI)
- styling strategy and theming model consistency
- component architecture completeness (states, variants, overlays, feedback)
- framework/rendering constraint alignment
- accessibility baseline coverage (focus, keyboard, contrast, reduced motion)
- performance envelope coherence (motion, budgets, loading strategy)
- SEO/discovery baseline presence
- implementation map realism (real file locations and conventions)
- output quality: dynamic, project-aware, and non-generic

---

## References

| Document | Description |
|----------|-------------|
| [SKILL.md](./SKILL.md) | Full skill protocol and generation rules |
| [references/paths/existing-project.md](./references/paths/existing-project.md) | Existing-project workflow |
| [references/paths/new-project.md](./references/paths/new-project.md) | New-project workflow |
| [references/rules/styling.md](./references/rules/styling.md) | Styling conventions and theming rules |
| [references/rules/accessibility.md](./references/rules/accessibility.md) | Accessibility rules and checklist |
| [references/rules/performance.md](./references/rules/performance.md) | Performance targets and optimization rules |
| [references/rules/seo.md](./references/rules/seo.md) | SEO baseline and metadata guidance |
| [references/components.md](./references/components.md) | Component checklist |
| [references/tokens.md](./references/tokens.md) | Token templates and naming |
| [references/resources.md](./references/resources.md) | Design libraries and palette resources |

---

## License

MIT License © 2026 Octocode

See [LICENSE](https://github.com/bgauryy/octocode/blob/main/LICENSE) for details.
