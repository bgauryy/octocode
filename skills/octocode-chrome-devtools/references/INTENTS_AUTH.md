# CDP Auth Intents

Load for login, MFA, CAPTCHA, or real-profile work. Why: authenticated CDP can expose sensitive state.

## login
Open visible Chrome and let the user authenticate. Do not automate password/MFA entry unless explicitly approved and safe. After user confirmation, list targets and continue on the same port.

## user-auth
Use when the user must complete manual auth, consent, or CAPTCHA. Emit `[AUTH_COMPLETE]` only after a deterministic page signal says the authenticated state is present; emit `[AUTH_TIMEOUT]` with the observed URL/state otherwise.

## Real Profile Gate
Before `--profile`, warn that CDP scripts can read cookies, tokens, storage, and page data. Require explicit yes. Review generated scripts for secret reads and exfiltration.

## Follow-up
After auth, run the smallest debug/scrape/security script on the same port with `--keep-tab`; do not navigate unless asked.

Next: storage safety in `references/INTENTS_STORAGE_CONSENT.md`; launch flags in `references/CHROME_FLAGS.md`.
