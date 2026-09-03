# CDP special patterns

Load for compound audits only. Why: broad checks must still be small scripts.

## Storage Audit
Combine `storage` intent with security redaction. Report key names, stores, counts, quota, and risky categories only.

## Consent Audit
Combine DOM/a11y inspection with storage/cookie metadata. Do not click consent choices unless asked.

## Full Audit
Run debug/network first, then security, storage, accessibility, performance, and screenshot scripts on the same port. Merge findings in the final answer; do not build one giant script.

Next: check-script equivalents in `references/cdp-checks.md`; when a compound audit fails mid-way, `references/recovery.md`.
