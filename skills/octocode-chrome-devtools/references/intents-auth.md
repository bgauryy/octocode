# CDP Auth Intents

Load for login, MFA, CAPTCHA, real-profile, or cookie transfer. Why: authenticated CDP exposes sensitive state.

## login
Open visible Chrome; let the user authenticate. Do not automate password/MFA unless explicitly approved.

## user-auth
Emit `[AUTH_COMPLETE]` only on a deterministic post-auth signal; else `[AUTH_TIMEOUT]` with observed URL/state.

## Real Profile Gate
Before `--profile`, warn CDP can read cookies, tokens, storage, and page data. Require explicit yes.

## Cookie Bridge
When the user wants default-browser cookies in an isolated headless session, load `references/cookie-bridge.md` and run `scripts/cookie-bridge.mjs` with `--i-understand-secrets`. Prefer storageState or `--from-port` over `--from-profile` if Chrome is already open.

`--from-profile` requires Chrome to be fully quit because a running instance locks the profile; otherwise launch falls back to an isolated, cookie-less profile. This skill cannot attach to an ordinary already-open Chrome without relaunching it. Keeping that live session requires an available browser-extension transport such as `chrome.debugger`; do not build one as part of this flow.

## Follow-up
After auth/inject, run the smallest debug/scrape script on the same `--to-port` with `--keep-tab`; do not navigate unless asked. Never print secret values.

Next: storage safety in `references/intents-storage.md`; launch flags in `references/chrome-flags.md`.
