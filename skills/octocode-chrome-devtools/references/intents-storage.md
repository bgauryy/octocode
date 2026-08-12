# CDP Storage And Consent Intents

Load for storage inspection or consent analysis. Why: storage can hold secrets; consent often blocks evidence.

## storage
Prefer `storage-measure-check.mjs` → `measure-query --view cookies|keys|findings`. Legacy counts-only: `storage-cookies-audit.mjs`. Keys/counts/flags only unless user approves values. Omit token/cookie/password/session values from stdout.

## consent
Detect banners/overlays/CMP; emit selectors + suggested manual action. Click accept/reject only if asked.

Next: `script-patterns-async.md`; security overlap → `intents-inspect.md`.
