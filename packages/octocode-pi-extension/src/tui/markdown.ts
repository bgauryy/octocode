/**
 * markdown — render Markdown to HTML for Octocode's local browser surfaces.
 *
 * Used by the plan page to embed an accepted RFC document inline. The content is
 * authored locally (by the agent/user) and served only on the loopback server,
 * but the page still opens in the user's real browser, so we neutralize raw HTML
 * passthrough as defense-in-depth: every `html` token (block and inline) is
 * escaped rather than emitted verbatim, so a `<script>` in an RFC cannot run.
 * GitHub-flavored Markdown (tables, task lists, fenced code) renders normally.
 *
 * `renderMarkdown` never throws — on any failure it falls back to the raw source
 * inside an escaped <pre>, matching the "surface writes never break the tool"
 * contract of plan-html.ts.
 */

import { Marked } from 'marked';
import { escapeHtml } from './html-page.js';

// A dedicated instance (not the shared singleton) so our renderer overrides never
// leak into any other marked consumer in the process (e.g. pi-tui).
const md = new Marked({ gfm: true, breaks: false });

// Neutralize raw HTML: block- and inline-level `html` tokens are escaped instead
// of passed through. marked has had no built-in sanitizer since v5, so this is the
// supported way to keep author-supplied HTML from reaching the DOM as markup.
md.use({
  renderer: {
    html(token: { text: string } | string): string {
      return escapeHtml(typeof token === 'string' ? token : token.text);
    },
  },
});

/** Render Markdown source to sanitized HTML. Never throws; falls back to <pre>. */
export function renderMarkdown(source: string): string {
  try {
    const html = md.parse(String(source ?? ''), { async: false });
    return typeof html === 'string' ? html : `<pre>${escapeHtml(String(source ?? ''))}</pre>`;
  } catch {
    return `<pre>${escapeHtml(String(source ?? ''))}</pre>`;
  }
}
