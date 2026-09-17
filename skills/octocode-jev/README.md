# Octocode Jev

A standalone meta-skill for research reasoning checks and logical crossroads, backed by TypeSafe's Jev decision API. The host agent supplies what it knows, asks a bounded question and independently verifies the advice. Includes a Rust HTTPS client, Node configuration launcher, local research consistency guard and offline tests. Choice, Score, Noul and model listing remain available; the skill never executes selected actions.

## Research second opinion

Use it when relevant evidence is already available but a contested inference or competing explanation could change the next step. Skip missing-fact discovery, deterministic checks and open-ended deep reasoning. Read [research](references/research.md) for packet construction, logical disputes and stopping rules.

Adapt `assets/research-request.json`, then run the atomic research command from the skill directory (put actual task files under the workspace's `.octocode/octocode-jev/`, not inside the skill):

```sh
node scripts/jev.mjs evaluate --input <request-file> --dry-run
node scripts/research.mjs --input <request-file> --retries 0 --timeout-ms 10000 > <result-envelope-file>
```

The command binds the response to the exact request hash and rejects contradictory claim/basis answers, invalid multi-source bases and pinned-model drift. Success means coherent advice, not verified truth; inspect the original sources. `scripts/research.mjs --help` explains its contract. `scripts/check-research.mjs` can recheck a saved bound envelope without an API call. Other API workflows use the normal client.

For provider limits and practical compaction, read [context budgeting](references/context.md).

## Run

Use Node 20+ for the launcher. The included `bin/octocode-jev-darwin-arm64` is built for Apple Silicon macOS. On another host, build from source with Rust 1.85+ and a C linker:

```sh
npm run build
node scripts/jev.mjs --help
node scripts/jev.mjs evaluate --input assets/request.json --dry-run
```

No npm dependencies or automatic install hooks. `package.json` supplies build, test and CLI entrypoints. `scripts/build.mjs` compiles `src/main.rs` with `Cargo.toml` and `Cargo.lock`, then copies the stripped executable into `bin/` with the host platform/architecture in its name. It uses a temporary build directory, or an explicit `CARGO_TARGET_DIR` for caching. Cargo downloads locked Rust dependencies on a fresh build. Release settings use size optimization, LTO and a single codegen unit.

Add this line to `<HOME>/.octocode/.env` (use your real key locally):

```dotenv
OCTOCODE_JEV_KEY=replace-with-your-key
```

The launcher reads that file automatically; no `export` or shell reload is needed. If `OCTOCODE_HOME` is set, it reads `.env` from that directory instead. Process variables, `.octocoderc`, and trusted project env files also work; the shared Octocode network timeout and retry configuration applies. Then run:

```sh
node scripts/jev.mjs evaluate --input assets/request.json --pretty
node scripts/jev.mjs models
```

The native executable runs without Node when the key is already in its process environment. The Node launcher adds Octocode configuration files; see [configuration](references/configuration.md) for precedence and examples. Both commands emit API JSON to stdout and JSON errors to stderr. Neither saves request or response files automatically.

## Install as a skill

Copy this entire `octocode-jev` folder to your agent's skills directory, or use the local skill installer with this folder as the source. Load `SKILL.md` to activate the workflow. Keep the references, scripts, assets and matching binary together. It runs without other Octocode skills installed; the vendored `scripts/octocode-config.mjs` supplies configuration lookup. Do not copy only `SKILL.md`.

## Verify or modify

In the Octocode repository, run `yarn workspace @octocodeai/config build` from the repository root after a fresh checkout to inject the shared configuration helper. The helper is generated and ignored by Git; standalone distributions include it.

```sh
npm run build
npm test
```

`scripts/test.mjs` exercises real local HTTP requests with fake credentials: wire format, all primitives, missing/inconsistent answers, key precedence, model overrides, bounded retries/timeouts, redirects, response limits, and an isolated single-folder copy. `scripts/research.test.mjs` checks the research guard and its standalone command. Neither calls the paid API. Rebuild after changing `src/main.rs`; the tests use the packaged binary. Only the built host target is verified; other platforms need their own build and test run.

The `assets/request.json` payload is a runnable example, not a live-response fixture. Dry-run needs no key but prints the supplied state, so use it only where that data may appear. A successful dry-run proves local structure, not provider acceptance or model accuracy.

Read [protocol](references/protocol.md) when writing payloads or checking current provider contracts, [patterns](references/patterns.md) when designing and composing decisions, and [sources](references/references.md) when auditing provenance. The source is original; no browser-use code is bundled. The configuration helper is the existing Octocode build artifact vendored unchanged.
