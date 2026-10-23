import type { Snapshot } from './snapshot.js';
import { PAGE_JS } from './html-script.js';
import { PAGE_CSS } from './html-style.js';

/** Offline and inert: only this page's own inline style and script may run; nothing loads, submits or connects. */
const CSP = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'";

/**
 * JSON that is safe inside a `<script type="application/json">` block: every `<` is escaped (so no text can write `</script>`
 * or `<!--`), and so are U+2028 and U+2029, which older script engines treat as line breaks. `JSON.parse` reads it back unchanged.
 */
export function embeddedJson(value: unknown): string {
  return JSON.stringify(value).replace(/</g, '\\u003c').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
}

/**
 * The backlog as one static page: the data in a JSON block, the style and script inline. Nothing from the data is placed
 * in the markup; the script builds the page from the parsed data with `createElement` and text nodes.
 */
export function renderBacklogHtml(snapshot: Snapshot): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${CSP}">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Backlog</title>
<style>${PAGE_CSS}</style>
</head>
<body>
<header class="top" id="top"></header>
<noscript>This page needs JavaScript. The backlog data is in the page source, in the JSON block with id "data".</noscript>
<main id="main"></main>
<div class="backdrop" id="backdrop" hidden></div>
<aside class="drawer" id="drawer" role="dialog" aria-label="Backlog item" hidden></aside>
<script type="application/json" id="data">${embeddedJson(snapshot)}</script>
<script>${PAGE_JS}</script>
</body>
</html>
`;
}
