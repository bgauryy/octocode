# Frontmatter

Use when checking metadata or host compatibility. The [Agent Skills specification](https://agentskills.io/specification) defines the portable format; the body has no required heading template.

| Field | Purpose |
|---|---|
| `name` | Required; matches the folder, 1–64 lowercase alphanumeric characters or hyphens, with no edge or consecutive hyphens. |
| `description` | Required; 1–1024 characters explaining the capability and relevant user intents. |
| `license` | Optional license name or reference to the bundled license. |
| `compatibility` | Optional real environment requirements, up to 500 characters. |
| `metadata` | Optional string-to-string map for a consumer that needs it. |
| `allowed-tools` | Experimental, optional space-separated tool permissions; verify host support. |

Prefer portable fields. Before adding a host extension, verify it in that host's current documentation or loader and explain the requirement. Do not copy a host's metadata, hook, or permission fields into every skill.

Keep `name` stable and tune discovery through `description`. The local reviewer checks common scalar frontmatter; use a YAML-aware spec validator for advanced YAML or publishing requirements. Trigger quality is reviewed in [SKILL.md](../SKILL.md).
