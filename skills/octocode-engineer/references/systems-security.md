Load when threat modelling, or choosing authentication, authorization, secrets handling or cryptography.

# Systems: security

## Security

| Area | Practice |
|---|---|
| Threat model | Name assets, actors, entry points and trust boundaries; walk STRIDE per trust boundary (spoofing, tampering, repudiation, information disclosure, denial of service, elevation of privilege); build an attack tree for one goal; then assign each mitigation an owner |
| Input | Validate at every trust boundary; parse into typed values; parameterize queries; encode output for its context (HTML, shell, SQL) |
| Authentication vs authorization | Authentication proves who; authorization checks each object access on the server (BOLA, below) |
| Least privilege | Minimal scopes for tokens, services and database roles; separate read and write credentials |
| Secrets | Keep them out of code, logs and URLs; rotate; load from a secret store |
| Passwords | Hash with Argon2id, scrypt or bcrypt, never a bare fast hash such as SHA-256 |
| Cryptography | Use vetted libraries and TLS 1.2+ (prefer 1.3); AEAD such as AES-GCM or ChaCha20-Poly1305; never reuse a nonce with the same key; compare secrets in constant time; draw randomness from a CSPRNG |
| Integrity | Pick a CRC, hash, HMAC or signature by the threat ([integrity](data-systems-distributed.md#integrity-deduplication-and-synchronization)) |
| Server-side requests and files | Guard against SSRF, path traversal and deserialization of untrusted data |
| Supply chain | Lockfiles, pinned versions, vulnerability scanning, minimal dependencies |
| Audit | Log security events with actor and outcome, never with secrets |
| Zero trust | Authenticate and authorize every request; network location grants nothing |
| Service identity | mTLS between services; workload identity (short-lived, platform-issued credentials) instead of static keys |
| Encryption at rest | Envelope encryption: a data key encrypts the data, a key-encryption key in a KMS encrypts the data key; rotate keys on a schedule |
| Data | Classify by sensitivity; enforce data residency; privacy by design (collect less, keep it shorter) |
| Build provenance | SBOM per artifact, signed artifacts and provenance (SLSA levels), hermetic and reproducible builds |

| Access model | Decides by | Fits |
|---|---|---|
| RBAC | Roles granted to users | Stable job functions |
| ABAC | Attributes of user, resource and environment | Context rules: region, time, classification |
| ReBAC | Relationships in a graph (owner, member, shared with) | Documents, folders, social and multi-tenant sharing |
| PBAC | Central policies evaluated by a policy engine | Many services sharing one rule set |

A policy enforcement point (PEP) in each service asks a policy decision point (PDP) and enforces the answer. OAuth 2.0 delegates authorization; OpenID Connect adds identity on top. OWASP API Security Top 10 (2023) lists broken object-level authorization (BOLA) first and broken function-level authorization (BFLA) fifth: check each object and each operation on the server.
