# Cookie Bridge

Load before transferring cookies into an isolated CDP session. Why: authenticated state is powerful and secret-bearing.

## Prefer Order
1. Manual `user-auth` on the same visible CDP port (no transfer).
2. `--from-storage-state` Playwright jar the user already exported.
3. `--from-port` pull from an existing CDP Chrome the user controls.
4. `--from-profile` only if Chrome is fully quit and user explicitly approved.

## Required Gate
Warn that CDP can read cookies/tokens. Require explicit yes, then pass `--i-understand-secrets`. Never print values — names, domains, counts, flags only.

## Commands
```bash
# from existing CDP source → isolated target
node <skill-dir>/scripts/cookie-bridge.mjs --i-understand-secrets \
  --from-port 9333 --to-port 9222 --urls "https://app.example.com"

# export jar for reuse (mode 0600)
node <skill-dir>/scripts/cookie-bridge.mjs --i-understand-secrets \
  --from-port 9333 --export-storage-state .octocode/tmp/auth.json --dry-run

# profile launch (Chrome must not lock the profile)
node <skill-dir>/scripts/cookie-bridge.mjs --i-understand-secrets \
  --from-profile Default --to-port 9222 --urls "https://app.example.com"
```

## After Inject
List targets on `--to-port`, navigate or attach with `--keep-tab`, continue debug/scrape. Do not re-export secrets into chat or HAR shares.

Next: auth gates in `references/intents-auth.md`; launch notes in `references/chrome-flags.md`.
