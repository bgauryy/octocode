# CDP Special Patterns

Load for compound audits only. Why: broad checks should still be small scripts.

## Storage Audit
Combine `storage` intent with security redaction. Report key names, stores, counts, quota, and risky categories only.

## Consent Audit
Combine DOM/a11y inspection with storage/cookie metadata. Do not click consent choices unless asked.

## Full Audit
Run debug/network first, then security, storage, accessibility, performance, and screenshot scripts on the same port. Merge findings in the final answer; do not build one giant script.
