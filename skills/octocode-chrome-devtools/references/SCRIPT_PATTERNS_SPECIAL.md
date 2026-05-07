# CDP Special Pattern Pointers

## Storage Audit

> **Detailed recipe in `INTENTS_STORAGE_CONSENT.md` -> `## storage`** — grep: `rg -n "^## storage" references/INTENTS_STORAGE_CONSENT.md`

Inventories cookies, localStorage, sessionStorage, IndexedDB, Cache Storage, Service Workers, and quota.
Also detects **cookie resurrection** (tracking IDs mirrored across storage to survive clearing).

## Consent Audit

> **Detailed recipe in `INTENTS_STORAGE_CONSENT.md` -> `## consent`** — grep: `rg -n "^## consent" references/INTENTS_STORAGE_CONSENT.md`

Detects CMP presence, pre-granted consent state, and tracker firing before user consent.

## Full Audit (combine all)

Combine the Network Console, Performance Audit, DOM Accessibility, and Security Audit `run()` functions above into a single script — enable all required domains at the top, attach all event listeners before navigating, run all sync checks after the page settles.
