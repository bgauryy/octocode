---
name: octocode-get-started
description: Use when setting up the Octocode Codex plugin, checking local prerequisites and GitHub sign-in, or diagnosing a missing MCP connection. Not for ordinary code research.
---

# Get started with Octocode

tools: Octocode MCP, Node.js, npm, GitHub CLI (`gh`), and `npx -y octocode@{{CLI_VERSION}}`.
related-skill: `octocode-research`
output: Report checks in chat. GitHub CLI owns its local login; this skill creates no token store.
routes: Read [authentication and packaging sources](references/references.md) when explaining credential behavior or installation requirements.

## Workflow

1. Check `node --version`, `npm --version`, and whether the Octocode MCP tools are available. This release requires Node.js {{NODE_RANGE}}. If a prerequisite is missing, explain what to install and resume after it is available. Do not treat a missing optional service as a failure of local code research.
2. Establish the user's active project directory. Use an absolute project path for the first local tool call. Never substitute the plugin installation or cache directory for the project. If access is denied, explain the project-root mismatch and help configure `WORKSPACE_ROOT` through the user's MCP settings for that project. Do not broaden `ALLOWED_PATHS` to the entire filesystem.
3. If GitHub access is needed, check `gh --version` and `gh auth status --active --hostname github.com`. If sign-in is missing, guide the user to run `gh auth login --hostname github.com --web` in their own terminal. The user completes browser authorization. For GitHub Enterprise, use the hostname already configured by the user.
4. Check `npx -y octocode@{{CLI_VERSION}} auth --json` from the project directory. Report the authenticated state and credential source, never credentials. Environment variables and existing Octocode logins take precedence over `gh-cli`; explain that when the account differs from the expected GitHub CLI account. Do not remove or replace an existing login without the user's instruction.
5. Call an available Octocode local read tool against a known project file. Inspect the live tool schema and supply its required goal and reasoning. If the user needs GitHub, make one read against a repository they selected. Distinguish missing login, missing repository permission, and rate limiting from successful authentication.
6. Report what worked and any remaining setup. Use the bundled public skills for subsequent tasks. Dependencies such as browser tooling, local model services, language servers, or classification-provider credentials are needed only for their corresponding workflows; follow each skill's setup guidance when requested.

Never ask for a token in chat, run `gh auth token` as an agent-visible command, use `gh auth status --show-token`, or copy a token into a manifest or tool argument. The local MCP runtime obtains credentials internally and sends authenticated requests directly to GitHub. Recommend GitHub CLI login rather than creating an Octocode-managed login during onboarding.

Plugin activation loads skills and MCP configuration. It does not authorize account changes, install optional dependencies, or start workflow-specific hooks. If the user already installed the same skills separately, explain the duplication and let them choose which installation to keep.
