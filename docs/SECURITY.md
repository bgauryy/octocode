# Security

## Why it matters

An AI agent browsing your codebase runs into `.env` files, `~/.aws/credentials`, private keys, and CI tokens. Without active protection, those secrets flow straight into the LLM context window, where logs capture them, tool call results expose them, or prompt injection exfiltrates them.

Octocode enforces a hard boundary between untrusted content and the model:

> **Every returned byte is scanned and redacted before it reaches the LLM.** Secrets are stripped on the way *in* (inputs) and on the way *out* (results). If output sanitization itself fails, Octocode discards the raw result and returns a generic structured error.

You get this by default, for every tool call, over both MCP and CLI.

---

## The pipeline

Three independent redaction stages guard every tool call.

```mermaid
flowchart TD
    REQ(["🔵 Tool call"])

    subgraph S1["① Input guard"]
        I1["Schema & bounds\nstr ≤10k · arr ≤100 · depth ≤20"]
        I2["Injection scan\nshell metacharacters · prototype keys · circular refs"]
        I3["Secret redaction on args"]
        I1 --> I2 --> I3
    end

    subgraph S2["Tool execution"]
        T1["GitHub API · local FS · LSP · npm"]
    end

    subgraph S3["② Content guard (per file / response)"]
        C1["Path validation + sensitive-file blocklist"]
        C2["Rust scanner  300+ patterns → [REDACTED-TYPE]"]
        C3["File-context patterns + Registry extras"]
        C1 --> C2 --> C3
    end

    subgraph S4["③ Output guard"]
        O1["maskSensitiveData on every text item"]
        O2["Warn agent when redaction occurred"]
        O1 --> O2
    end

    REQ --> S1
    S1 -->|"invalid / secrets in args"| E1(["❌ Structured error"])
    S1 -->|clean| S2
    S2 --> S3
    S3 -->|"blocked path"| E2(["❌ Redacted error"])
    S3 -->|clean| S4
    S4 -->|"sanitizer failure"| E3(["❌ Raw output discarded"])
    S4 --> MODEL(["✅ Model"])

    style E1 fill:#dc3545,color:#fff,stroke:none
    style E2 fill:#dc3545,color:#fff,stroke:none
    style E3 fill:#dc3545,color:#fff,stroke:none
    style MODEL fill:#28a745,color:#fff,stroke:none
    style REQ fill:#0d6efd,color:#fff,stroke:none
```

The following table shows what the agent sees after redaction:

| Situation | Output |
|-----------|--------|
| Secret in file content | `[REDACTED-AWSACCESSKEYID]`, `[REDACTED-GITHUBPAT]`, … |
| Secret reaching output boundary | `A*I*S*A*4*1*X*…` (every-other-char masked) |
| Content too large | `[CONTENT-REDACTED-SIZE-LIMIT]` |
| Path blocked | Structured error — path never echoed |
| Output sanitizer failure | Generic structured error — raw output and exception details are discarded |

---

## Secret detection

The Rust-native `RegexSet` scanner (with a TypeScript fallback from the same pattern list) covers **300+ patterns** across every major cloud, SaaS, and dev-tool credential format.

```mermaid
flowchart LR
    IN(["Content string"])

    subgraph RUST["Rust RegexSet (primary)"]
        R1["300+ provider patterns\nAWS · Azure · GCP · GitHub\nOpenAI · Stripe · Slack …"]
    end

    subgraph TS["TypeScript layer (secondary)"]
        T1["File-context-aware patterns\ne.g. password= only fires\ninside .env / config files"]
        T2["Registry extras\ncustom org patterns"]
        T1 --> T2
    end

    MATCH{"Match?"}
    REDACT["[REDACTED-TYPENAME]"]
    OUT(["Sanitized content\n+ warnings[]"])

    IN --> RUST --> MATCH
    MATCH -->|"yes"| REDACT --> OUT
    MATCH -->|"no"| TS --> OUT
```

The scanner covers these categories:

| Category | Examples |
|----------|---------|
| Cloud | AWS (key ID, secret, session token, ARN), Azure (AD, storage, Cosmos), GCP, Alibaba |
| AI providers | OpenAI, Anthropic, Azure OpenAI, Bedrock, AI21, AssemblyAI |
| SaaS / dev tools | GitHub (PAT, fine-grained, OAuth, app), Slack, Stripe, npm, Atlassian, Auth0, Adyen |
| Generic / structural | JWTs, PEM/SSH keys, bearer tokens, passwords in URLs, DB connection strings, high-entropy strings |

**File-context-aware patterns** avoid false positives in source code: `password =` only fires when the file path matches `.env`, `config`, or `secrets`; Spring Boot credential patterns only apply to `application.yml` / `application.properties`.

---

## Path validation

Local filesystem access passes through two independent layers, so a bypass of one layer still leaves the other guarding sensitive files.

```mermaid
flowchart TD
    REQ2(["File / path request"])

    subgraph L1["Layer 1 — Discovery pruning (tree walk)"]
        D1["Sensitive dirs pruned from results\n.ssh · .aws · .kube · .docker · secrets/ …"]
    end

    subgraph L2["Layer 2 — Read-time access control"]
        V1["Resolve + normalize\n WORKSPACE_ROOT → strip ../ → expand ~"]
        V2["Prefix-check allowed roots\nHOME (default) + ALLOWED_PATHS + registered roots"]
        V3["Sensitive-file blocklist\n*.pem · .env · id_rsa · *.tfstate …"]
        V4["Symlink re-validation\nrealpath → repeat prefix check"]
        V1 --> V2 --> V3 --> V4
    end

    REQ2 --> L1
    L1 -->|"sensitive dir"| HIDE(["❌ Not visible in results"])
    L1 -->|safe| L2
    V2 -->|"outside roots"| DENY1(["❌ Access denied"])
    V3 -->|"blocked file"| DENY2(["❌ Redacted error"])
    V4 -->|"symlink escape"| DENY3(["❌ Blocked"])
    V4 -->|safe| READ(["✅ Read proceeds"])

    style HIDE fill:#dc3545,color:#fff,stroke:none
    style DENY1 fill:#dc3545,color:#fff,stroke:none
    style DENY2 fill:#dc3545,color:#fff,stroke:none
    style DENY3 fill:#dc3545,color:#fff,stroke:none
    style READ fill:#28a745,color:#fff,stroke:none
```

The blocklist matches on file name and directory path, and covers these categories:

| Category | Examples |
|----------|---------|
| Keys and certs | `*.pem`, `*.key`, `*.p12`, `id_rsa`, `id_ed25519`, `.ssh/` |
| Credentials | `.env`, `.env.*`, `.netrc`, `.npmrc`, `.git-credentials`, `*_token`, `client_secret*.json` |
| Cloud and infra | `.aws/credentials`, `.kube/`, `*.tfstate`, `*.tfvars`, `.s3cfg` |
| Secret stores | `.password-store/`, `*.kdbx`, OS keychains, browser login DBs |
| Shell and history | `.bash_history`, `.zsh_history`, `.*_history` |
| Crypto wallets | `wallet.dat`, `.bitcoin/`, `.ethereum/` |
| App secrets | `wp-config.php`, `google-services.json`, `secrets.yml`, `master.key` |

Canonical sources: `packages/octocode-engine/src/security/filePatterns.ts` and `packages/octocode-engine/src/security/pathPatterns.ts`.

`ALLOWED_PATHS` (or `local.allowedPaths` in `.octocoderc`) adds roots on top of the HOME default. To turn local tools off entirely, set `ENABLE_LOCAL=false`.

---

## Command execution

External commands run through `child_process.spawn()` with an argument array — not `exec` — and Octocode rejects shell metacharacters before execution.

| Command | Hardening |
|---------|-----------|
| `rg` | Explicit flag allowlist; `--pre`/`--pre-glob` blocked (arbitrary binary exec). Combined short flags validated char-by-char. |
| `git` | Only `clone` + `sparse-checkout`. `file://`, `git://`, `http://` URLs blocked (HTTPS only). `-c` keys allowlisted to safe config (`advice.detachedHead`, `core.autocrlf`, `http.extraHeader`, …). |
| `find` | `-exec`, `-execdir`, `-ok`, `-delete`, `-printf` and all exec/write operators blocked. |
| `grep` | Shared dangerous-pattern scan (`;&|$()` etc.) applied to all arguments. |

---

## Credentials and tokens

**Resolution order.** `OCTOCODE_TOKEN` → `GH_TOKEN` → `GITHUB_TOKEN` → `GITHUB_PERSONAL_ACCESS_TOKEN` → encrypted on-disk OAuth → `gh` CLI token. The first non-empty value wins; the four environment variables are protected keys that Octocode never reads from `.env` or `.octocoderc`.

**On-disk storage.** AES-256-GCM encrypted under `OCTOCODE_HOME`.

**Output masking.** Output masking covers tokens, so results never echo them.

See [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) for token setup and credential architecture.

---

## Input limits and injection guards

| Check | Limit / action |
|-------|---------------|
| String length | ≤ 10,000 chars |
| Array length | ≤ 100 items |
| Object nesting | ≤ 20 levels |
| Prototype pollution | `__proto__`, `constructor`, `prototype` keys → rejected |
| Circular references | WeakSet ancestor tracking → rejected |
| Numeric ranges | depth / context-lines / limits / offsets clamped at schema layer |

---

## Tool timeout and cancellation

Every tool call runs under a 60-second timeout. MCP clients can cancel through `AbortSignal`. Both timeout and cancellation return a structured error, so no partial data leaks through. Override the default in either of these ways:

```ts
configureSecurity({ defaultTimeoutMs: 30_000 })   // process-wide override
withSecurityValidation(name, handler, { timeoutMs: 10_000 })  // per-tool override
```

---

## Extension API

The `securityRegistry` singleton lets you extend security policy before boot — useful for org-specific secrets or multi-tenant deployments.

```mermaid
flowchart LR
    subgraph API["securityRegistry"]
        A1["addSecretPatterns()\n+ ReDoS safety check"]
        A2["addAllowedCommands()"]
        A3["addAllowedRoots()"]
        A4["addIgnoredPathPatterns()\naddIgnoredFilePatterns()\n+ ReDoS safety check"]
    end

    A1 & A2 & A3 & A4 --> FREEZE{"freeze()"}
    FREEZE -->|"locked — mutations throw"| RUNTIME["Runtime\n(immutable policy)"]
    FREEZE -->|"reset() to unfreeze"| API

    style RUNTIME fill:#28a745,color:#fff,stroke:none
```

```ts
import { securityRegistry } from '@octocodeai/octocode-engine/security';

securityRegistry.addSecretPatterns([
  { name: 'my-service-token', regex: /mst_[A-Za-z0-9]{32}/ }
]);
securityRegistry.addAllowedCommands(['my-search-tool']);
securityRegistry.addAllowedRoots(['/mnt/shared-workspace']);
securityRegistry.addIgnoredPathPatterns([/\/internal-vault\//]);
securityRegistry.addIgnoredFilePatterns([/\.company-secret$/]);

securityRegistry.freeze(); // lock — throws on any further mutation
```

Custom patterns pass through these guards:
- A **ReDoS timing heuristic** checks every `regex` value against 50-, 500-, and 2,000-character inputs, and rejects any pattern that takes 200 ms or longer on one of them. Rejection happens before registration.
- Octocode deduplicates repeated names and sources.
- `securityRegistry.version` increments on every mutation — use it for cache invalidation.

---

## Scope and disclosure

Octocode protects the **agent context boundary** — what flows between untrusted content and the model. It does not replace repository secret-scanning, OS-level sandboxing, or network egress controls; run those alongside it.

To report a vulnerability, open a private advisory on the [octocode repository](https://github.com/bgauryy/octocode) rather than a public issue.
