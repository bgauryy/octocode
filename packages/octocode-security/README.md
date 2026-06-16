# octocode-security

Rust-backed security primitives for the octocode toolchain.

Secret detection and masking run in a native Node.js addon powered by Rust's
linear-time `regex` engine — ReDoS is structurally impossible.  Path
validation, command allowlisting, and input sanitization run in TypeScript.

Every code example below is verified by
[`tests/readme-examples.test.ts`](./tests/readme-examples.test.ts).

---

## Performance

Measured on Apple M-series (200 runs, 20 warmup). TS baseline = pure-JS V8
regex engine running the same 309 patterns.

### `sanitizeContent` — clean content (no secrets)

| Payload | TS (JS) p50 | Rust p50 | Speedup |
|---------|------------|----------|---------|
| 1 KB    | 0.056 ms   | 0.003 ms | **20×** |
| 10 KB   | 0.50 ms    | 0.019 ms | **27×** |
| 100 KB  | 5.1 ms     | 0.18 ms  | **27×** |
| 500 KB  | 25 ms      | 1.0 ms   | **25×** |
| 1 MB †  | 50 ms      | 2.1 ms   | **24×** |
| 5 MB †  | 248 ms     | 10 ms    | **25×** |

† Chunked path (`detect_chunked`). The REGEX_SET pre-filter runs once on the
full content; clean payloads early-return in near-zero time regardless of pattern count.

### `sanitizeContent` — 5 embedded secrets

| Payload | TS p50 | Rust p50 | Speedup |
|---------|--------|----------|---------|
| 10 KB   | 0.53 ms | 0.032 ms | **16×** |
| 100 KB  | 5.1 ms  | 0.24 ms  | **21×** |
| 500 KB  | 25 ms   | 1.5 ms   | **17×** |
| 1 MB †  | 51 ms   | 3.0 ms   | **17×** |

### `maskSensitiveData`

| Payload | TS p50 | Rust p50 | Speedup |
|---------|--------|----------|---------|
| 10 KB   | 0.56 ms | 0.18 ms | **3×** |
| 100 KB  | 5.1 ms  | 1.5 ms  | **3×** |
| 500 KB  | 25 ms   | 7.7 ms  | **3×** |

Run `yarn bench` to reproduce.

---

## Install

```bash
npm install octocode-security
```

The package ships prebuilt native binaries for all supported platforms as
`optionalDependencies` — no Rust toolchain needed at install time.
The root package does not publish local `.node` files; those artifacts live in
the per-platform optional packages.
Every native build script runs `scripts/sync-platform-binaries.cjs` after
`napi build` so each built `octocode-security.<triple>.node` file is copied
into its matching `npm/<platform>/` optional package directory.

Supported: `darwin-arm64`, `darwin-x64`, `linux-arm64-gnu`,
`linux-x64-gnu`, `linux-x64-musl`, `win32-x64-msvc`.

If no prebuilt binary is available, the package falls back to a pure-JS
implementation running the same 309 patterns through V8.

---

## Quick start

```ts
import { ContentSanitizer }          from 'octocode-security/contentSanitizer';
import { maskSensitiveData }         from 'octocode-security/mask';
import { PathValidator }             from 'octocode-security/pathValidator';
import { validateCommand }           from 'octocode-security/commandValidator';
import { withSecurityValidation }    from 'octocode-security/withSecurityValidation';
import { withBasicSecurityValidation } from 'octocode-security/withSecurityValidation';
```

### Secret detection and redaction

```ts
const result = ContentSanitizer.sanitizeContent(
  'export AWS_KEY=AKIAIOSFODNN7EXAMPLE\nexport DB_URL=postgresql://admin:s3cr3t@db:5432/prod'
);
// result.hasSecrets        → true
// result.secretsDetected   → ['awsAccessKey', 'postgresUrl']
// result.content           → 'export AWS_KEY=[REDACTED-AWSACCESSKEY]\nexport DB_URL=[REDACTED-POSTGRESURL]'
// result.warnings          → ['2 secret(s) redacted']
```

File-context patterns (e.g. Kubernetes `Secret` blocks in YAML) only fire
when the file path matches:

```ts
const yaml = 'kind: Secret\ndata:\n  password: c2VjcmV0\n';
ContentSanitizer.sanitizeContent(yaml);                       // no match (no path)
ContentSanitizer.sanitizeContent(yaml, 'k8s/secret.yaml');    // match — redacted
ContentSanitizer.sanitizeContent(yaml, 'src/index.ts');       // no match
```

### Partial masking (for logs and display)

```ts
maskSensitiveData('token: ghp_16C7e42F292c6912E7710c838347Ae178B4a');
// → 'token: *h*_*6*7*4*F*9*c*9*2*7*0*8*8*4*A*1*8*4*'
// Every even-indexed character of the matched region is replaced with *
```

Explicit patterns bypass the Rust engine entirely (no registry, no builtins):

```ts
maskSensitiveData(text, [{ name: 'MY_TOKEN', description: '…', regex: /MY_TOKEN=[^\s]+/g }]);
```

### Input parameter validation

Recursively sanitizes all string values (redacts secrets), enforces depth
limit (20), array cap (100 items), string length cap (10 000 chars), and
blocks prototype-pollution keys (`__proto__`, `constructor`, `prototype`):

```ts
const r = ContentSanitizer.validateInputParameters({
  query: 'find auth code',
  token: 'ghp_16C7e42F292c6912E7710c838347Ae178B4a',  // will be redacted
  nested: { deep: 'value' },
});
// r.isValid         → true  (structural checks passed)
// r.hasSecrets      → true  (secret was found and redacted)
// r.sanitizedParams → { query: 'find auth code', token: '[REDACTED-…]', nested: { deep: 'value' } }
// r.warnings        → ['Secrets detected in token: githubPat']
```

### Path validation

```ts
const v = new PathValidator({
  workspaceRoot: '/repo',
  includeHomeDir: false,   // default: true
  additionalRoots: ['/data'],
});

v.validate('/repo/src/index.ts');          // { isValid: true, sanitizedPath: '/repo/src/index.ts' }
v.validate('/repo/../etc/passwd');         // { isValid: false, error: '…outside allowed directories' }
v.validate('/repo/node_modules/evil');     // { isValid: false, error: '…ignored directory…' }
v.validate('/repo/symlink-to-root');       // symlink target is resolved and re-validated
```

The module singleton (`pathValidator`) is initialized from `WORKSPACE_ROOT`
env and `ALLOWED_PATHS` env (comma-separated). Re-initialize it without
restart:

```ts
import { resetPathValidator } from 'octocode-security/pathValidator';
resetPathValidator({ workspaceRoot: '/new-root', includeHomeDir: false });
```

### Command validation

Allowlisted commands: `rg`, `ls`, `find`, `grep`, `git`. Each has its own
flag validator — arguments are never passed to a shell.

```ts
validateCommand('rg',   ['-n', '--type', 'ts', 'pattern', '.']);  // { isValid: true }
validateCommand('find', ['.', '-name', '*.ts', '-type', 'f']);     // { isValid: true }
validateCommand('git',  ['clone', '--depth', '1', 'https://github.com/org/repo', 'dir']); // { isValid: true }

validateCommand('curl', ['https://evil.com']);                     // { isValid: false }
validateCommand('rg',   ['--pre', 'evil-script', 'pattern']);      // { isValid: false }
validateCommand('git',  ['clone', 'file:///etc/shadow', 'dir']);   // { isValid: false, error: 'protocol not allowed' }
validateCommand('find', ['.', '-exec', 'rm', '-rf', '{}', ';']);   // { isValid: false }
```

Search-pattern positions get a stricter check (no shell metacharacters in the
regex itself). Non-pattern arguments are checked against the broader dangerous
pattern set.

### Tool-handler middleware

Wrap any MCP tool handler to get automatic input sanitization, configurable
timeout, and structured error containment. Both wrappers share the same core
(`runSecure`) — validation, timeout, logging, and error handling are identical.

**`withSecurityValidation`** — for tools that receive auth context:

```ts
import {
  withSecurityValidation,
  configureSecurity,
  type SecurityDepsConfig,
} from 'octocode-security/withSecurityValidation';

// Wire up once at startup
configureSecurity({
  logToolCall:     async (toolName, repos, goal) => { /* telemetry */ },
  logSessionError: async (toolName, errorCode) => { /* error tracking */ },
  isLoggingEnabled: () => true,
});

const handleSearch = withSecurityValidation(
  'ghSearchCode',
  async (sanitizedArgs, authInfo, sessionId) => {
    // sanitizedArgs is already validated and secrets-redacted
    const results = await searchCode(sanitizedArgs, authInfo);
    return { content: [{ type: 'text', text: JSON.stringify(results) }] };
  },
  { timeoutMs: 30_000 }
);

// MCP server calls it as:
await handleSearch(rawArgs, { authInfo, sessionId, signal });
```

**`withBasicSecurityValidation`** — for local tools (no auth):

```ts
import { withBasicSecurityValidation } from 'octocode-security/withSecurityValidation';

const handleRipgrep = withBasicSecurityValidation(
  async (sanitizedArgs) => {
    const output = await runRg(sanitizedArgs);
    return { content: [{ type: 'text', text: output }] };
  },
  'localSearchCode',        // tool name for telemetry (optional)
  { timeoutMs: 60_000 }
);

await handleRipgrep(rawArgs, { signal });
```

`logToolCall` fires only on success (`isError !== true`). `logSessionError`
fires when the sanitizer throws synchronously. Handler rejections are caught
by the internal timeout wrapper and returned as error results.

### Path and file filters

```ts
import { shouldIgnore, shouldIgnorePath, shouldIgnoreFile } from 'octocode-security/ignoredPathFilter';

shouldIgnore('/home/user/.ssh/id_rsa');        // true — blocked path + file
shouldIgnorePath('/repo/.git/config');         // true
shouldIgnoreFile('.env.production');           // true
shouldIgnoreFile('src/index.ts');              // false
```

Blocked path segments: `.git`, `.ssh`, `.aws`, `.docker`, `.kube`,
`.terraform`, `secrets`, `private`, `.password-store`, crypto wallets,
browser profile directories, and more.

Blocked file names: `.env*`, `.npmrc`, `.netrc`, `credentials`, SSH keys,
known-hosts, PEM/key files, kubeconfigs, and more.

---

## Extending at runtime

`securityRegistry` is a `globalThis`-keyed singleton that survives ESM
multi-instance scenarios. All changes take effect immediately across every
module that reads the registry.

```ts
import { securityRegistry } from 'octocode-security/registry';

// Extra secret patterns — validated for ReDoS safety before insertion
securityRegistry.addSecretPatterns([{
  name: 'myInternalToken',
  description: 'Internal service token',
  regex: /MY_SVC_TOKEN=[A-Za-z0-9]{32}/g,
  matchAccuracy: 'high',
}]);

// Extra allowed FS roots (picked up by PathValidator on next construction)
securityRegistry.addAllowedRoots(['/mnt/data']);

// Extra allowed commands (normalized automatically — 'rg.exe' stores as 'rg')
securityRegistry.addAllowedCommands(['myCustomBinary']);

// Extra path/file ignore patterns (ReDoS-checked)
securityRegistry.addIgnoredPathPatterns([/(?:^|\/)\.internal(?:\/|$)/]);
securityRegistry.addIgnoredFilePatterns([/^secrets\.ya?ml$/]);

// Freeze to prevent further mutation (e.g. after server startup)
securityRegistry.freeze();
// Later: securityRegistry.reset() to unfreeze and clear
```

`addSecretPatterns` rejects any pattern that takes > 50 ms against a 100-char
adversarial input, preventing ReDoS from being introduced at runtime.

---

## Modules

| Subpath | Exports |
|---------|---------|
| `octocode-security` (main) | Everything below, re-exported |
| `octocode-security/contentSanitizer` | `ContentSanitizer` |
| `octocode-security/mask` | `maskSensitiveData` |
| `octocode-security/pathValidator` | `PathValidator`, `pathValidator`, `resetPathValidator` |
| `octocode-security/commandValidator` | `validateCommand`, `normalizeCommandName` |
| `octocode-security/withSecurityValidation` | `withSecurityValidation`, `withBasicSecurityValidation`, `configureSecurity`, `SecurityDepsConfig` |
| `octocode-security/ignoredPathFilter` | `shouldIgnore`, `shouldIgnorePath`, `shouldIgnoreFile` |
| `octocode-security/registry` | `SecurityRegistry`, `securityRegistry`, `ISecurityRegistry` |
| `octocode-security/pathUtils` | `redactPath` |
| `octocode-security/paramExtractors` | `extractResearchFields`, `extractRepoOwnerFromParams` |
| `octocode-security/regexes` | `allRegexPatterns` (TS source — inspection only; runtime uses Rust) |
| `octocode-security/types` | `SensitiveDataPattern`, `SanitizationResult`, `ValidationResult`, `PathValidationResult`, `ToolResult`, `ISanitizer` |

---

## Environment variables

| Variable | Effect |
|----------|--------|
| `OCTOCODE_SECURITY_FORCE_JS=1` | Skip native binary; always use JS fallback |
| `OCTOCODE_SECURITY_REQUIRE_NATIVE=1` | Throw if native binary cannot be loaded (no JS fallback) |
| `OCTOCODE_SECURITY_NATIVE_PATH=<path>` | Load `.node` binary from this exact path |
| `ALLOWED_PATHS=<comma-separated>` | Extra roots added to `PathValidator` at construction time |

---

## Architecture

### Pattern pipeline

Patterns are authored in `src/regexes/*.ts` as `SensitiveDataPattern` objects.
`scripts/gen-patterns.mjs` converts the compiled `allRegexPatterns` array to
`src/patterns.rs`. Every `build:ts` run regenerates `patterns.rs` first — the
two files cannot drift during a normal build.

`yarn verify:patterns` provides an additional explicit sync check: it
regenerates and fails if `git diff` shows any change. Use this in CI after
editing patterns.

### Detection strategy

**Single path** (`content.length ≤ 500 000` bytes): `REGEX_SET.matches(content)`
scans all 309 patterns in one pass (O(n) in content length). Only matched
pattern indices proceed to `replace_all`.

**Chunked path** (`content.length > 500 000` bytes): the same `REGEX_SET`
pre-filter runs once on the full content to build a candidate set. Only
candidate patterns enter the overlapping-chunk loop (500 KB chunks,
1 KB overlap). Clean large payloads skip the chunk loop entirely.

**JS fallback**: the same pattern order, same chunk/overlap constants, same
`lastIndex` reset discipline — results are identical to the Rust path.

**Extra patterns** (added via `securityRegistry`): always run in JS after the
Rust pass, using the same chunked strategy for large content.

### Native binary loading

`src/native.ts` is the single FFI bridge. Load order:

1. `OCTOCODE_SECURITY_NATIVE_PATH` env (explicit override)
2. Per-platform npm `optionalDependency` (e.g. `octocode-security-darwin-arm64`)
3. File candidates in `runtime/security/` (bundled into consuming packages)
4. Package-local `*.node` files (local development / explicit bundled layouts)
5. JS fallback (unless `OCTOCODE_SECURITY_REQUIRE_NATIVE=1`)

### Wrapper core

`withSecurityValidation` and `withBasicSecurityValidation` are thin closures
over a shared `runSecure` function. Every shared concern — input validation,
timeout, AbortSignal, success logging, error result shape, `logSessionError` —
is implemented exactly once. The wrappers differ only in whether `authInfo` and
`sessionId` are forwarded to the handler.

---

## Scripts

| Script | Purpose |
|--------|---------|
| `yarn build` | gen-patterns → napi release build → sync platform package → esbuild + `.d.ts` |
| `yarn build:ts` | gen-patterns → esbuild + `.d.ts` (no Rust recompile) |
| `yarn build:dev` | gen-patterns → napi debug build → sync platform package → esbuild + `.d.ts` |
| `yarn build:rust:darwin-arm64` | Cross-compile for macOS arm64 and sync `npm/darwin-arm64` |
| `yarn build:rust:all` | All platform binaries, each synced into its optional package directory |
| `yarn test` | Vitest with v8 coverage (≥ 90% required) |
| `yarn test:quiet` | Vitest, no coverage |
| `yarn typecheck` | `tsc --noEmit` |
| `yarn lint` / `yarn lint:fix` | ESLint |
| `yarn lint:rust` | `cargo clippy -D warnings` |
| `yarn pack:check` | `npm pack --dry-run` guard: no root `.node` files, bounded size |
| `yarn verify` | pattern sync + typecheck + clippy + tests + pack check + `cargo audit` |
| `yarn verify:patterns` | Regenerate `patterns.rs`, fail on drift |
| `yarn gen` | Regenerate `patterns.rs` only |
| `yarn bench` | JS-vs-Rust benchmark (`bench/compare.mjs`) |
| `yarn audit` | `cargo audit` (unmaintained / unsound / yanked) |

---

## Tests

| File | What it covers |
|------|---------------|
| `rust-specific.test.ts` | Native binary load, NAPI boundary types, Unicode/multibyte, large content (single + chunked path), ReDoS guarantees, known-secret spot-checks (26 patterns), masking, parallel calls, shape contract, idempotency |
| `withSecurityValidation.core.test.ts` | Parity contract for both wrappers: validation, result shape, logging gate (success=log / error=skip), `logSessionError` trigger, timeout, AbortSignal, auth passthrough, `configureSecurity` isolation |
| `withSecurityValidation.basic.test.ts` | Validation failure branches, timeout, signal, output pass-through |
| `withSecurityValidation.logging.test.ts` | Logging order, repo extraction, bulk-query splitting |
| `withSecurityValidation.extractRepoOwner.test.ts` | Repo/owner extraction from query shapes |
| `withSecurityValidation.extractResearchFields.test.ts` | Research field extraction |
| `patterns-sync.test.ts` | Count parity (TS ↔ patterns.rs ↔ binary), name order, file-context metadata, DO-NOT-EDIT integrity, `build:ts` gate |
| `contentSanitizer.test.ts` | Sanitizer API including null/undefined/circular/depth edge cases |
| `commandValidator.test.ts` | Full flag-by-flag coverage for `rg`, `git`, `find`; injection attempts |
| `pathValidator.test.ts` + `pathValidator.extended.test.ts` | Traversal attacks, symlinks, ENOENT ancestor walk, home-dir handling, `resetPathValidator` |
| `registry.test.ts` | Mutation, freeze/reset lifecycle, ReDoS guard, duplicate dedup, version counter |
| `ignoredPathFilter.test.ts` | Path and file ignore matching, macOS `/private/var` normalization |
| `mask.test.ts` + `mask.branches.test.ts` | Masking coverage and branch paths |
| `local-tools-sanitization.test.ts` | End-to-end input sanitization for local-tool arg shapes |
| `penetration-test.test.ts` | Adversarial inputs: path traversal, command injection, prototype pollution, ReDoS |
| `coverage-gaps.test.ts` | Targeted coverage for uncovered branches |
| `investigate-bypasses.test.ts` | Known bypass attempts |
| `readme-examples.test.ts` | Verifies every code example in this file |

Rust unit tests live in `src/detector.rs` (11 tests): single-path and chunked-path correctness,
char-boundary handling, file-context gating, `mask_text` edge cases.

---

## License

MIT
