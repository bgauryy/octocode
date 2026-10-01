# Authentication and packaging sources

Read this reference when explaining local credentials, package installation, or optional dependencies.

- [GitHub CLI login](https://cli.github.com/manual/gh_auth_login): browser authentication and local credential storage. GitHub CLI uses the system credential store when available and can fall back to a local plaintext file.
- [Octocode authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md): credential precedence and the internal GitHub CLI fallback. The installed runtime's `auth --json` output identifies its effective source without exposing the token.
- [Plugin packaging](https://developers.openai.com/plugins/build/plugins): manifests, marketplace installation, and installed plugin paths.
- [Octocode public skills](https://github.com/bgauryy/octocode/tree/main/skills): workflow-specific prerequisites and supporting resources.
