# Octocode for Codex

Install Octocode's local MCP server and all public Octocode skills as one Codex plugin. The plugin adds an onboarding skill for project setup and GitHub sign-in.

## Install

The marketplace entry requires a published `@octocodeai/codex-plugin` release and the runtime versions pinned by that release. Until those packages are published, use the local development procedure in the repository's `ARCHITECTURE.md` for this package.

1. Install Node.js compatible with the pinned Octocode CLI and MCP releases, npm, and a Codex client that supports plugins. The source release targets Node.js 24.15 or later within Node.js 24.
2. Add the marketplace:

   ```sh
   codex plugin marketplace add bgauryy/octocode
   ```

3. Install **Octocode** from that marketplace in the desktop Plugins Directory. Codex CLI versions with `plugin add` also support:

   ```sh
   codex plugin add octocode@octocode
   ```

4. Open your project and start a new chat with Octocode enabled. Ask: **Set up Octocode for this project.**

## Connect GitHub

Local code research works without GitHub sign-in. For authenticated GitHub access, install [GitHub CLI](https://cli.github.com/) and sign in on the computer running the MCP server:

```sh
gh auth login --hostname github.com --web
gh auth status --active --hostname github.com
```

GitHub CLI manages the local credential. Octocode's local process reads it internally to call GitHub; the plugin adds no hosted credential service or credential store. Never paste a token into chat or the plugin files. GitHub CLI can fall back to a local plaintext file when the OS credential store is unavailable; see [GitHub's login documentation](https://cli.github.com/manual/gh_auth_login).

Existing environment credentials and Octocode logins take precedence over GitHub CLI. Onboarding reports the active source without displaying the token. See [Octocode authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) for account selection and Enterprise configuration.

## Use the skills

The bundle includes every public skill from the release's `skills/` collection, with its references and helpers. Research, architecture, code review, documentation, evaluation, and other workflows activate according to their skill descriptions. Some workflows require separately configured language servers, browser tools, local model services, or provider credentials. Read the selected skill's prerequisites when using it.

If you already installed Octocode skills individually, choose one installation to avoid duplicate skill entries. Installing this plugin does not remove your existing skills or enable workflow-specific hooks.

## Troubleshoot

- **MCP does not start:** check Node.js/npm availability in the Codex process environment and the availability of the pinned runtime release. Review the MCP startup error before retrying.
- **Project access denied:** confirm Codex opened the intended project. Set `WORKSPACE_ROOT` in the MCP configuration for that project if needed. Do not point it at the plugin cache or widen access to the entire filesystem.
- **GitHub account differs:** inspect the credential source through onboarding; an environment token or existing Octocode login can override GitHub CLI.
- **Private repository denied:** confirm the selected account has access and satisfies the organization's authorization requirements.
- **Optional tool missing:** complete the prerequisites for that skill. Other workflows remain available.

## Update or remove

Refresh the marketplace and install the selected release through your Codex client. The catalog pins the plugin version; each plugin release pins its MCP version. Start a new chat after updating.

Use the Plugins Directory to uninstall Octocode. On clients supporting CLI removal, consult `codex plugin remove --help`. Removing the plugin leaves your GitHub CLI login and Octocode user configuration under your control.

The GitHub marketplace is separate from OpenAI's public directory. A public-directory listing for the local MCP package requires confirmation from OpenAI; this package does not claim that approval.
