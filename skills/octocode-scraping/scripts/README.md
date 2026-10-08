# Scraping commands

These scripts handle fetching, corpus queries, and complete source pagination. Run a command with `--help` for its inputs.

| Need | Command |
|---|---|
| Fetch or crawl public pages | `fetch.mjs` |
| Check optional hosted provider or credits | `provider-check.mjs`, `provider-usage.mjs` |
| Inspect or search a saved session | `corpus-inspect.mjs`, `corpus-find.mjs` |
| Inspect static DOM, assets, or links | `dom-find.mjs`, `resource-list.mjs`, `graph-navigate.mjs` |
| Prove a field or page an original source | `corpus-run.mjs`, `source-query.mjs` |
| Bring browser evidence into a session | `har-ingest.mjs` |

For hosted requests, `SCRAPING_ANT` must be in the process environment. It may be stored in `<HOME>/.octocode/.env`; pass it through the host or capture `npx octocode config get SCRAPING_ANT` into the script environment without displaying it. Direct public fetching needs no key. Keep source artifacts and follow each `next` continuation to avoid gaps.

Focused tests are in `tests/`. Run those covering the changed command or library; a browser integration change also needs a live browser check.
