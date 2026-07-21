# CDP Storage And Consent Intents

Load for storage inspection or consent-banner analysis. Why: storage can contain secrets; consent flows often block evidence.

## storage
Inspect localStorage/sessionStorage keys, IndexedDB database names/object stores, cache names, service-worker registrations, and quota metadata. Print keys and counts only unless the user explicitly approves value inspection. Never print tokens, cookies, passwords, or session IDs.

## consent
Detect banners, blocking overlays, CMP globals, and cookie categories. Do not click accept/reject unless the user asked for that action. Emit selectors and suggested manual action.

## Safe Output
Allowed: key names, counts, domains, expiration metadata, cookie flag names. Forbidden: cookie values, Authorization headers, JWTs, passwords, API keys.

Next: for DOM actionability load `references/script-patterns-async.md`; for security overlap load `references/intents-inspect.md`.
