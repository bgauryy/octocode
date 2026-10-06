# Octocode Chrome DevTools

Collect live browser evidence: DOM actions, HAR, console, performance, storage, and authenticated pages. Agent guidance and command examples live in [SKILL.md](SKILL.md).

## Install

Requires Chrome and Node.js 24+. The sandbox selects the network permission flag for the installed Node version.

```bash
npx -y octocode skill install octocode-chrome-devtools
```

## Maintainer verification

Run from the skill folder; the suites use isolated profiles and local fixtures.

```bash
node scripts/hermetic-suite.mjs
node scripts/cdp-checks/webmcp-tools.check.mjs
node scripts/live-suite.mjs
```

`hermetic-suite` checks standalone portability, optional scraping integration, environment forwarding, artifact pagination, and evidence retention. `webmcp-tools.check` tests discovery and invocation in Chrome. `live-suite` tests snapshots, input, iframes, upload, drag, waits, screenshots, and performance. Use `live-suite --only <name>` for a focused case; `--keep` retains its browser and artifacts for debugging.

Run `octocode-skills/scripts/skill-review.mjs` against this folder for skill structure and reference checks.

Standalone helpers accept `--help`: `open-browser`, `cdp-sandbox`, `cdp-runner`, `cookie-bridge`, `prune-artifacts`, `protocol-corpus`, `har-ingest-to-scrape`, `corpus-run-local`, and `cdp-checks/{measure-query,har-pager,har-redact,api-replay}`. The scraping bridges and protocol corpus need `octocode-scraping` beside this folder or `--scraping-skill-dir <dir>`.

Read helpers and checks through the sandbox as described in [script patterns](references/script-patterns.md); they export `run(cdp)` or library functions and are not standalone commands.

`robustness-self-test.mjs` checks request-once response paging, deadlines, HAR body redaction, pruning, and unavailable measurements. Native browser settings are the default; `--stealth` is an opt-in emulation experiment.

Live coverage includes delayed selectors, post-action text timeouts, disabled controls, exact role/name lookup, ambiguity errors, closed shadow-root input, trusted versus synthetic event traces, streamed response timing, redirects, unfinished requests, and isolated iframe actions/network bodies. `frame-events.mjs` shares iframe event setup across network checks.

Live fixtures cover [delayed content and input events](scripts/tests/fixtures/async-flow.html) and [cross-origin frame input](scripts/tests/fixtures/cross-frame.html).

Custom flows use the installed Chrome schema through `cdp.protocol()` and arbitrary `cdp.send` methods. `--browser` supports browser-level targets. The optional recipes cover common tasks; they do not define the supported protocol. `scripts/artifact-query.mjs` pages any saved JSON, JSONL/text, or binary capture with digest-pinned continuations. See [protocol routing](references/cdp-protocol.md) and [custom patterns](references/script-patterns.md).
