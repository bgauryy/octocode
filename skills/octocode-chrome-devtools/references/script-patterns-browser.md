# CDP Browser Surface Patterns

Load for websockets, resource search, file upload, artifacts, shadow DOM, or source maps. Why: these need browser-specific helpers.

## WebSocket Surveillance
Enable Network before navigation; collect created/closed/frame events. Report counts and safe samples; redact secrets.

## Resource Search
Use Performance/resource entries, Network URLs, and DOM script/link tags. Search URLs/text with bounded snippets.

## File Upload
Use absolute host paths with `DOM.setFileInputFiles`, then dispatch visible `input`/`change` events. Ask before uploading real sensitive files.

## Artifacts
Screenshots, PDFs, traces, and metadata must be written under `cdp.outputDir`. Emit `[SCREENSHOT]`, `[METRIC]`, or `[FINDING]` with the path.

## Shadow DOM
`DOM.querySelector` does not pierce shadows. Use `Runtime.evaluate` recursive helpers and return selectors/paths, not raw giant DOM.

## Source Maps
Use `sourcemap-resolver.mjs` staged by the sandbox. Emit `[SOURCEMAP]` with resolved original source/line when available.

Next: CDP domain ordering in `references/cdp-agent.md`.
