# CDP Browser Surface Patterns

Load for websockets, resource search, file upload, artifacts, shadow DOM, source maps, or event-listener graphs. Why: these need browser-specific helpers.

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
Use `sourcemap-resolver.mjs` staged by the sandbox. Emit `[SOURCEMAP]` with resolved original source/line when available. It fetches `.map` files over real outbound HTTP(S) via Node's `http`/`https` core modules — deliberately not localhost-only like everything else in the sandbox, since source maps live at the site's own real domain.

## Event Listener Graph
`DOMDebugger.getEventListeners({objectId, depth: 0})` on a batch of candidate elements (from `Runtime.getProperties` over a `querySelectorAll` result) builds an element→event-type→handler-location graph without clicking anything. Useful only on vanilla-JS sites with direct `onclick`/`addEventListener` bindings. On React/Vue/etc., unrelated elements resolve to the same delegated root-level listener — it just tells you "the framework dispatches this," not the real per-component handler; trace into the framework instead of adding more CDP calls.

Next: CDP domain ordering in `references/cdp-agent.md`.
