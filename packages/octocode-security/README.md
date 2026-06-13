# octocode-security

Rust-backed security primitives for the octocode toolchain: secret detection and
masking run in a native addon built on Rust's linear-time `regex` engine (no
ReDoS possible); path, command, and input validation run in TypeScript.

Every example below is executable and verified by
[`tests/readme-examples.test.ts`](./tests/readme-examples.test.ts).

## Quick start

```ts
import { PathValidator } from 'octocode-security/pathValidator';
import { ContentSanitizer } from 'octocode-security/contentSanitizer';
import { maskSensitiveData } from 'octocode-security/mask';
import { validateCommand } from 'octocode-security/commandValidator';

// Path traversal is blocked
const validator = new PathValidator({ workspaceRoot: '/repo', includeHomeDir: false });
validator.validate('/repo/../etc/passwd'); // { isValid: false, error: '…outside allowed directories' }

// Secrets are detected and redacted (304 patterns, Rust RegexSet)
ContentSanitizer.sanitizeContent('token: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx');
// { content: 'token: [REDACTED-…]', hasSecrets: true, secretsDetected: […] }

// Or partially masked for display
maskSensitiveData('ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx');

// Commands are allowlisted (rg, ls, find, grep, git) with per-command flag validation
validateCommand('rg', ['-n', 'pattern', '.']); // { isValid: true }
```

## Modules

| Subpath | What it does |
| --- | --- |
| `octocode-security/pathValidator` | Multi-layer path validation: normalize → allowed-root prefix check → ignored-path filter → symlink resolve (incl. deepest existing ancestor for not-yet-created paths) → re-validate |
| `octocode-security/contentSanitizer` | Secret detection/redaction over the Rust pattern engine + recursive input-parameter validation (size, depth, array caps, dangerous keys) |
| `octocode-security/mask` | Partial masking of detected secrets for logs and display |
| `octocode-security/commandValidator` | Command allowlist + per-command flag/argument validators (`spawn`-style, never a shell) |
| `octocode-security/withSecurityValidation` | Tool-handler middleware: input sanitization, timeout, catch-all error containment |
| `octocode-security/ignoredPathFilter` | `shouldIgnore()` — system/VCS/secret-bearing path patterns |
| `octocode-security/registry` | `securityRegistry` — runtime extension point for extra roots, commands, patterns; `freeze()` lifecycle |
| `octocode-security/regexes` | The TS pattern source of truth (inspection only — the runtime detector is Rust) |

## Architecture

- **TS → Rust pattern pipeline.** Patterns live in `src/regexes/*.ts`;
  `scripts/gen-patterns.mjs` generates `src/patterns.rs` from the compiled
  array. The generator **fails** if any pattern cannot be converted to Rust
  regex syntax, and `yarn verify:patterns` fails CI when `patterns.rs` drifts
  from regeneration.
- **Panic-safe FFI.** Native entry points never `unwrap` on input data; panics
  are confined to static init over compile-time-known patterns.
- **Native binary.** `octocode-security.<platform>.node`, built with
  `yarn build:rust` (napi-rs). In the monorepo the binary is bundled into the
  consuming package's `runtime/security/` directory at build time.

## Scripts

| Script | Purpose |
| --- | --- |
| `yarn build` | Full build: gen-patterns → napi release build → esbuild + d.ts |
| `yarn test` | Vitest suite with coverage |
| `yarn verify` | typecheck + cargo clippy (`-D warnings`) |
| `yarn verify:patterns` | Regenerate `patterns.rs` and fail on drift |
| `yarn bench` | JS-vs-Rust detection benchmark |

## License

MIT
