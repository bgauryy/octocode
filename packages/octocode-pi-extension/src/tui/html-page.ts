/**
 * html-page — the shared shell for Octocode's LOCAL HTML surfaces.
 *
 * Some data reads better in a browser than a terminal (plans as diagrams,
 * diffs, worker timelines). Writers generate these files under the Octocode
 * home and serve them over the shared loopback server (see local-server.ts):
 * every change rewrites the file and the page meta-refreshes, so the browser
 * tab live-updates while the user keeps talking in the terminal.
 *
 * Mermaid loads from the jsdelivr CDN when requested; offline the diagram
 * block degrades to its readable source text while the rest of the page still
 * renders. All dynamic text MUST pass through escapeHtml.
 */

/** Escape text for safe interpolation into HTML content or attributes. */
export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export interface OctocodePageOptions {
  title: string;
  /** Pre-escaped body HTML (callers own escaping of their dynamic text). */
  bodyHtml: string;
  /** Auto-reload period. The page shows a "live" hint when set. */
  refreshSeconds?: number;
  /** Stable state token used to reload only when generated content actually changes. */
  refreshToken?: string;
  /** Load + initialize mermaid for `<pre class="mermaid">` blocks. */
  mermaid?: boolean;
  /** Small brand label displayed above the page title. */
  eyebrow?: string;
  /** Use the wider application shell for settings/control-center pages. */
  wide?: boolean;
  /** Left-aligned document flow without panel containers, for plan review. */
  layout?: 'document';
  /** Optional pre-escaped footer content. */
  footerHtml?: string;
}

/**
 * Render a complete standalone HTML document in the Octocode brand look
 * (octocode-dark palette: teal accent, lavender links, gold highlights).
 */
export function renderOctocodePage(opts: OctocodePageOptions): string {
  const refreshSeconds = opts.refreshSeconds ? Math.max(1, Math.floor(opts.refreshSeconds)) : undefined;
  const refresh = refreshSeconds && opts.refreshToken
    ? `<meta name="octocode-refresh-token" content="${escapeHtml(opts.refreshToken)}">
<script type="module">
  (() => {
    const token = document.querySelector('meta[name="octocode-refresh-token"]')?.content;
    const scrollKey = 'octocode-page-scroll:' + location.pathname;
    try { const saved = sessionStorage.getItem(scrollKey); if (saved) requestAnimationFrame(() => scrollTo(0, Number(saved))); } catch {}
    setInterval(async () => {
      if (document.activeElement?.matches('textarea, input, [contenteditable="true"]')) return;
      try {
        const response = await fetch(location.href, { cache: 'no-store' });
        if (!response.ok) return;
        const next = new DOMParser().parseFromString(await response.text(), 'text/html')
          .querySelector('meta[name="octocode-refresh-token"]')?.content;
        if (next && token && next !== token) {
          try { sessionStorage.setItem(scrollKey, String(scrollY)); } catch {}
          location.reload();
        }
      } catch { /* keep the current readable page when polling fails */ }
    }, ${refreshSeconds * 1000});
  })();
</script>`
    : '';
  const mermaid = opts.mermaid
    ? `<script type="module">
      import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs';
      mermaid.initialize({ startOnLoad: true, theme: 'base', themeVariables: { primaryColor: '#fffaf5', primaryTextColor: '#15263a', lineColor: '#718096', primaryBorderColor: '#e8d8ca', tertiaryColor: '#edf7ff' } });
    </script>`
    : '';
  const live = refreshSeconds ? ` · live (checks for updates every ${refreshSeconds}s)` : '';
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
${refresh}
<title>${escapeHtml(opts.title)}</title>
<style>
  :root { --bg:#F2F7FC; --panel:#FFFFFF; --panel-soft:#F8FBFE; --line:#DCE6F0; --line-strong:#C7D4E1; --ink:#14283D; --muted:#66788A; --orange:#FF8A3D; --orange-deep:#EC692C; --violet:#7957D5; --cyan:#16B8C9; --teal:var(--cyan); --lav:var(--violet); --gold:var(--orange); --red:#D94B55; --shadow:0 18px 50px rgba(31,65,96,.10); }
  * { box-sizing: border-box; }
  html { scroll-behavior:smooth; }
  body { margin:0; padding:clamp(1rem,3vw,2.4rem); background:var(--bg); color:var(--ink); font:15px/1.6 Inter,ui-sans-serif,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif; }
  body::before { content:""; position:fixed; inset:0; pointer-events:none; z-index:-1; background:radial-gradient(circle at 8% 4%,rgba(22,184,201,.13),transparent 25rem),radial-gradient(circle at 92% 8%,rgba(121,87,213,.11),transparent 28rem),linear-gradient(150deg,rgba(255,138,61,.06),transparent 38%); }
  main { max-width: 920px; margin: 0 auto; }
  main.wide { max-width:1280px; }
  .brand-head { display:flex; align-items:center; gap:.9rem; margin:0 0 1.6rem; }
  .brand-mark { width:46px; height:46px; display:grid; place-items:center; flex:0 0 auto; border-radius:14px 14px 18px 18px; color:white; background:linear-gradient(145deg,var(--orange),var(--orange-deep)); box-shadow:0 9px 24px rgba(236,105,44,.25); font:900 1.2rem/1 ui-monospace,SFMono-Regular,Menlo,monospace; }
  .eyebrow { margin:0 0 .12rem; color:var(--violet); font:700 .7rem/1.3 ui-monospace,SFMono-Regular,Menlo,monospace; letter-spacing:.14em; text-transform:uppercase; }
  h1 { font-size:clamp(1.45rem,3vw,2.35rem); line-height:1.06; letter-spacing:-.035em; color:var(--ink); margin:0; font-weight:850; text-transform:uppercase; }
  h1 .mark { color:var(--orange); }
  .sub { color:var(--muted); font:500 .78rem/1.5 ui-monospace,SFMono-Regular,Menlo,monospace; margin:.3rem 0 0; }
  section { background:rgba(255,255,255,.94); border:1px solid var(--line); border-radius:18px; padding:clamp(1rem,2.4vw,1.45rem); margin-bottom:1rem; box-shadow:0 5px 22px rgba(31,65,96,.045); }
  h2 { font-size:.76rem; letter-spacing:.12em; text-transform:uppercase; color:var(--violet); margin:0 0 .8rem; font-weight:800; }
  ul.steps { list-style:none; margin:0; padding:0; }
  ul.steps li { padding:.3rem 0; border-bottom:1px solid var(--line); }
  ul.steps li:last-child { border-bottom:none; }
  .done { color:var(--muted); text-decoration:line-through; }
  .doing { color:var(--gold); font-weight:600; }
  .todo { color:var(--ink); }
  .blocked { color:var(--muted); font-style:italic; }
  .glyph { display:inline-block; width:1.4em; }
  .deps { color:var(--muted); font-size:.8rem; }
  pre { overflow-x:auto; }
  pre.mermaid { background:transparent; display:flex; justify-content:center; }
  /* Embedded RFC document: render as a real doc, not a status panel. */
  section.rfc h2 .rfc-status { color:var(--gold); font-size:.75rem; letter-spacing:.04em; margin-left:.5rem; }
  .rfc-body { color:var(--ink); }
  .rfc-body h1, .rfc-body h2, .rfc-body h3, .rfc-body h4 { color:var(--teal); text-transform:none; letter-spacing:normal; margin:1.2rem 0 .5rem; }
  .rfc-body h1 { font-size:1.15rem; } .rfc-body h2 { font-size:1rem; } .rfc-body h3 { font-size:.9rem; }
  .rfc-body a { color:var(--lav); }
  .rfc-body code { background:var(--bg); border:1px solid var(--line); border-radius:4px; padding:.05rem .3rem; font-size:.85em; }
  .rfc-body pre { background:var(--bg); border:1px solid var(--line); border-radius:8px; padding:.75rem; }
  .rfc-body pre code { border:none; padding:0; }
  .rfc-body table { border-collapse:collapse; width:100%; margin:.5rem 0; font-size:.9rem; }
  .rfc-body th, .rfc-body td { border:1px solid var(--line); padding:.35rem .6rem; text-align:left; }
  .rfc-body blockquote { border-left:3px solid var(--line); margin:.5rem 0; padding:.1rem 0 .1rem .8rem; color:var(--muted); }
  /* Phase timeline */
  ol.phase-timeline { list-style:none; margin:0; padding:0; display:flex; flex-wrap:wrap; gap:.4rem; }
  ol.phase-timeline .ph { display:inline-flex; align-items:center; gap:.4rem; font-size:.82rem;
    padding:.4rem .7rem; border-radius:8px; border:1px solid var(--line); color:var(--muted); background:var(--bg); }
  ol.phase-timeline .ph .ph-g { font-family:ui-monospace,monospace; }
  ol.phase-timeline .ph.done { color:var(--muted); }
  ol.phase-timeline .ph.now { color:var(--gold); border-color:var(--gold); font-weight:600;
    box-shadow:0 0 0 3px color-mix(in srgb, var(--gold) 18%, transparent); }
  ol.phase-timeline .ph.todo { opacity:.65; }
  .phase-note { margin:.7rem 0 0; color:var(--muted); font-size:.8rem; }
  .phase-note strong { color:var(--gold); font-weight:600; }
  .phase-note.abandoned { color:var(--red); }
  /* Decisions */
  ul.decisions { list-style:none; margin:0; padding:0; display:flex; flex-direction:column; gap:.5rem; }
  ul.decisions li { display:flex; flex-direction:column; gap:.15rem; padding:.5rem .7rem;
    border:1px solid var(--line); border-radius:8px; background:var(--bg); }
  ul.decisions .dq { color:var(--muted); font-size:.82rem; }
  ul.decisions .da { color:var(--lav); }
  .browser-reply label { display:block; color:var(--muted); font-size:.82rem; margin-bottom:.35rem; }
  .reply-help { color:var(--muted); margin:-.25rem 0 .8rem; font-size:.86rem; }
  .browser-reply textarea { width:100%; resize:vertical; min-height:6rem; padding:.75rem; color:var(--ink);
    background:var(--bg); border:1px solid var(--line); border-radius:8px; font:inherit; }
  .browser-reply textarea:focus { outline:2px solid var(--lav); outline-offset:2px; }
  .reply-actions { display:flex; flex-wrap:wrap; gap:.55rem; margin-top:.7rem; }
  .reply-actions button { cursor:pointer; border:1px solid var(--line); border-radius:7px; padding:.5rem .75rem;
    color:var(--ink); background:var(--bg); font:inherit; font-size:.82rem; }
  .reply-actions button:hover { border-color:var(--lav); }
  .reply-actions button.primary { color:white; background:linear-gradient(135deg,var(--orange),var(--orange-deep)); border-color:var(--orange-deep); font-weight:800; }
  .reply-actions button:disabled { cursor:wait; opacity:.55; }
  .reply-status { min-height:1.4em; margin:.55rem 0 0; color:var(--teal); font-size:.82rem; }
  details pre { background:var(--bg); border:1px solid var(--line); border-radius:8px; padding:1rem; color:var(--muted); }
  summary { cursor:pointer; color:var(--lav); font-size:.85rem; }
  code, pre { font-family:ui-monospace,SFMono-Regular,Menlo,monospace; }
  code { background:#F4F8FC; border:1px solid var(--line); border-radius:6px; padding:.08rem .34rem; }
  table { width:100%; border-collapse:collapse; }
  th, td { padding:.55rem; border-bottom:1px solid var(--line); text-align:left; vertical-align:top; }
  th { color:var(--muted); font-size:.78rem; text-transform:uppercase; letter-spacing:.06em; }
  button { cursor:pointer; border:1px solid var(--line-strong); border-radius:10px; padding:.5rem .76rem; color:var(--ink); background:var(--panel); font:inherit; font-size:.8rem; font-weight:700; transition:transform .16s ease,border-color .16s ease,box-shadow .16s ease; }
  button:hover { border-color:var(--violet); transform:translateY(-1px); box-shadow:0 6px 16px rgba(31,65,96,.10); }
  button.primary { color:white; background:linear-gradient(135deg,var(--orange),var(--orange-deep)); border-color:var(--orange-deep); font-weight:800; }
  .badge { display:inline-block; padding:.08rem .42rem; border:1px solid var(--line); border-radius:999px; color:var(--muted); font-size:.75rem; }
  .badge.on { color:#087F8C; border-color:rgba(22,184,201,.55); background:rgba(22,184,201,.08); }
  .stack { display:flex; flex-direction:column; gap:.55rem; }
  .row { display:flex; align-items:center; justify-content:space-between; gap:.8rem; }
  .muted { color:var(--muted); }
  input,select,textarea { width:100%; border:1px solid var(--line-strong); border-radius:10px; padding:.62rem .72rem; background:white; color:var(--ink); font:inherit; }
  input:focus,select:focus,textarea:focus { outline:3px solid rgba(121,87,213,.16); border-color:var(--violet); }
  label { display:grid; gap:.3rem; color:var(--muted); font-size:.8rem; font-weight:650; }
  footer { color:var(--muted); font-size:.78rem; margin:1.6rem 0 .4rem; padding:1rem 1.15rem; border:1px solid var(--line); border-radius:14px; background:rgba(255,255,255,.72); }
  footer code { color:var(--violet); }
  @media (max-width:700px) { body{padding:.85rem}.brand-head{align-items:flex-start}.row{align-items:flex-start;flex-direction:column}section{border-radius:14px}table{display:block;overflow-x:auto} }
  main.document { max-width:1100px; margin:0; text-align:left; }
  .document section { background:none; border:0; border-radius:0; padding:0; box-shadow:none; margin:0 0 1.8rem; }
  .document pre.mermaid { justify-content:flex-start; border:0; background:none; padding:0; }
  .document footer { border:0; border-top:1px solid var(--line); border-radius:0; background:none; padding:1rem 0; }
</style>
${mermaid}
</head>
<body>
<main class="${opts.layout === 'document' ? 'document' : opts.wide ? 'wide' : ''}">
<header class="brand-head"><div class="brand-mark" aria-hidden="true">O</div><div><p class="eyebrow">${escapeHtml(opts.eyebrow ?? 'Octocode · local control')}</p><h1>${escapeHtml(opts.title)}</h1><div class="sub">private, loopback-only workspace surface${live}</div></div></header>
${opts.bodyHtml}
<footer>${opts.footerHtml ?? 'Configuration lives in <code>/settings</code> — your single place for MCP servers, tool enablement, skills, and prompt visibility.'}</footer>
</main>
</body>
</html>
`;
}
