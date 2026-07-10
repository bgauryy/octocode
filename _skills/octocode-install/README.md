# Octocode Install

Interactive setup for Octocode on macOS and Windows: MCP config, CLI, GitHub auth, IDE wiring, and skills installation.

Use when getting started with Octocode or repairing a broken install path.

## Capabilities

- Guided Octocode MCP + CLI install
- GitHub auth / token wiring
- IDE MCP config for supported hosts
- Skills installation into agent skill directories

## How It Works

```text
DETECT PLATFORM → AUTH → MCP/CLI → IDE → SKILLS → VERIFY
```

Follow the prompts for the current OS. Prefer verifying with a real `attend` / MCP tool call after install rather than assuming config file writes succeeded.

## Installation

```bash
npx octocode skill --name octocode-install
```

## Maintainer Notes

Keep this README user-facing. Platform-specific steps live in the skill body. Do not store secrets in skill files; use the host auth flow.
