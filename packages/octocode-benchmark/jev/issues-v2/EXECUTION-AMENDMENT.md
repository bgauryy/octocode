# Pre-comparison execution amendment

The app rejected the first treatment spawn with `agent thread limit reached`, twice. The partial first baseline was stopped and its artifacts preserved as `setup-pilot-R37637`; it is excluded from primary results. No treatment had started. The original frozen contract remains unchanged as an audit record.

For the primary comparison all twelve workers use identical fresh, ephemeral `codex exec` sessions, gpt-5.6-terra, high effort, at most two concurrent workers while the independent app Terra curator prepares gold. No parent/previous-session history is passed. The shared case/budget/rubric/packet policy remains as frozen. The launch script records actual process start/end, JSONL host usage when available, and final outputs. This replaces the original assumption of unavailable host tokens for these CLI sessions. Prefer actual host usage receipts over worker estimates.

Use `--ignore-user-config` so user-specific MCP/hooks do not vary or perform external actions, explicit matching model/effort and permission settings, and instruction-only write isolation as already disclosed. Credential loading remains the signed-in CLI's responsibility. Never print credentials. No CLI session may delegate, resume another session, or use network research outside the Octocode wrapper.

The curator receives no arm artifacts until its gold is sealed. This amendment and launch script are hashed before primary launches. The partial baseline is an infrastructure-excluded setup pilot, not a measured outcome. The substantive budget/rubric is unchanged.
