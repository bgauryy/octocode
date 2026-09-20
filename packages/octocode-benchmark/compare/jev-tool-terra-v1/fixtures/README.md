# Offline harness fixtures

`fake-mcp.mjs` supplies synthetic tool responses solely to `proxy-selftest.mjs`
and `appserver-selftest.mjs`. They test admission, one-call approvals, result
forwarding, and usage accounting without paid models or external data.

Its hardcoded judgments and token counts are test inputs, **not benchmark
observations**. It is never loaded by `campaign.mjs` or `recheck.mjs`.
Those runners freeze the real built Octocode MCP server and native runtime;
each run records its downstream entrypoint in `proxy-config.json` and its
actual server catalog in `calls.jsonl`. Report offline self-tests separately
from live research results and provider usage.
