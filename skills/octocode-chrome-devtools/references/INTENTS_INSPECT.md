# CDP Inspection Intents

Load when inspecting browser surfaces beyond ordinary DOM text. Why: choose domains and safety gates.

## security
Check TLS/certificate metadata, mixed content, CSP/security headers, cookie flags by name only, and unsafe storage patterns. Never output secret values.

## websocket
Track frames and failures. Record URL, opcode/type, counts, sampled safe payload metadata; redact tokens and user data.

## service-worker
Use `ServiceWorker.*` plus Target sessions to inspect registration, versions, activation, update failures, and controlled clients.

## workers
Discover worker targets with Target auto-attach. Route commands with `sessionId`; keep a target role map.

## intercept
Use Fetch only when mocking/blocking is needed. Every paused request must continue, fail, or fulfill; otherwise the page hangs.

## screenshot
Use Page capture or printToPDF. Write artifacts under `cdp.outputDir` and emit `[SCREENSHOT] <path>`.

## accessibility
Use Accessibility tree plus DOM labels/roles. Report missing names, keyboard traps, focus order risks, and evidence selectors.

## supply-chain
Inspect loaded scripts, source maps, third-party origins, integrity attributes, and unexpected remote code.

## full-audit
Run debug, security, storage, accessibility, performance, and screenshots as separate scripts on one session.

Next: special combined patterns live in `references/SCRIPT_PATTERNS_SPECIAL.md`.
