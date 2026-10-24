// Markdown engine. Runs in Node (self-test) and in the page (inlined via Function#toString), so
// mdKit must stay self-contained: no imports or outer references inside it, so its helpers
// cannot move to module scope and the function holds the whole engine.
/* oxlint-disable unicorn/consistent-function-scoping -- helpers must stay inside the serialized mdKit */
// oxlint-disable-next-line max-lines-per-function -- serialized whole via Function#toString
export function mdKit() {
  const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
  const slugify = (t) => String(t).toLowerCase().trim().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s+/g, '-') || 'section';
  // oxlint-disable-next-line no-control-regex -- strips control characters a browser ignores inside a URL scheme.
  const safeUrl = (u) => (/^(javascript|vbscript|data):/i.test(String(u).replace(/[\s\u0000-\u001f]/g, '')) ? '#' : String(u).trim());
  const decode = (value) => { try { return decodeURIComponent(value); } catch { return value; } };
  const encodePath = (path) => path.split('/').map(encodeURIComponent).join('/');
  const routeHref = (path, slug = '') => '#/' + (path ? encodePath(path) + (slug ? '@' + encodeURIComponent(slug) : '') : '');
  const routeParse = (hash) => {
    const raw = hash.replace(/^#\/?/, '');
    const at = raw.lastIndexOf('@');
    return at > 0 ? { path: decode(raw.slice(0, at)), slug: decode(raw.slice(at + 1)) } : { path: decode(raw) || null, slug: '' };
  };
  const ALERTS = { note: 'Note', tip: 'Tip', important: 'Important', warning: 'Warning', caution: 'Caution' };
  const INLINE_TAG = /^<(\/?)(br|kbd|sub|sup|mark|ins|del|s|u|b|i|small)\s*\/?>$/i;

  function frontmatter(src) {
    const m = String(src).match(/^\uFEFF?---\r?\n([\s\S]*?)\r?\n(?:---|\.\.\.)\r?\n?/);
    if (!m) return { meta: {}, body: String(src) };
    const meta = {};
    for (const line of m[1].split(/\r?\n/)) {
      const kv = line.match(/^([A-Za-z][\w -]*):\s*(.*)$/);
      if (kv && kv[2] !== '') meta[kv[1].trim().toLowerCase()] = kv[2].trim().replace(/^["']|["']$/g, '');
    }
    return { meta, body: String(src).slice(m[0].length) };
  }

  const HASH_LANG = /^(py|python|sh|bash|zsh|shell|console|yaml|yml|toml|rb|ruby|r|perl|ini|conf|dockerfile|make|makefile|nix|ex|elixir|ps1|powershell)$/;
  const DASH_LANG = /^(sql|lua|haskell|hs|elm)$/;
  const KW = new Set(('abstract and as assert async await break case catch class const continue crate def default defer del delete do elif else enum '
    + 'except export extends extern false final finally fn for from func function go if impl implements import in instanceof interface is lambda let loop '
    + 'match mod module move mut namespace new nil none not null of or package pass private protected pub public raise readonly ref return select self '
    + 'static struct super switch then this throw throws trait true try type typeof undefined unsafe use using var void when where while with yield').split(' '));

  const tokenClass = (m) => {
    if (m[1]) return 'tk-c';
    if (m[2]) return 'tk-s';
    if (m[3]) return 'tk-n';
    if (KW.has(m[4])) return 'tk-k';
    return /^[A-Z][a-z]\w*$/.test(m[4]) ? 'tk-t' : '';
  };
  const commentPattern = (lang) => {
    if (lang === 'json') return '(?!)';
    if (HASH_LANG.test(lang)) return '#[^\\n]*';
    return DASH_LANG.test(lang) ? '--[^\\n]*' : '\\/\\/[^\\n]*|\\/\\*[\\s\\S]*?\\*\\/';
  };
  const diffLine = (l) => {
    const cls = /^(\+\+\+|---|@@)/.test(l) ? 'tk-m' : l[0] === '+' ? 'tk-add' : l[0] === '-' ? 'tk-del' : '';
    return cls ? `<span class="${cls}">${esc(l)}</span>` : esc(l);
  };

  function highlight(code, lang) {
    lang = (lang || '').toLowerCase();
    if (lang === 'diff' || lang === 'patch') return code.split('\n').map(diffLine).join('\n');
    if (!lang || /^(text|txt|plain|plaintext|md|markdown|output|log|csv|tsv)$/.test(lang)) return esc(code);
    const str = '"(?:[^"\\\\\\n]|\\\\.)*"|\'(?:[^\'\\\\\\n]|\\\\.)*\'|`(?:[^`\\\\]|\\\\.)*`';
    const re = new RegExp(`(${commentPattern(lang)})|(${str})|\\b(\\d[\\d_]*(?:\\.\\d+)?)\\b|\\b([A-Za-z_]\\w*)\\b`, 'g');
    let out = '', last = 0, m;
    while ((m = re.exec(code))) {
      if (!m[0]) { re.lastIndex++; continue; }
      out += esc(code.slice(last, m.index));
      const cls = tokenClass(m);
      out += cls ? `<span class="${cls}">${esc(m[0])}</span>` : esc(m[0]);
      last = re.lastIndex;
    }
    return out + esc(code.slice(last));
  }

  // Link and footnote definitions leave the body; fenced code keeps them as text.
  const fenceToggle = (fence, l) => {
    const f = l.match(/^ {0,3}(`{3,}|~{3,})/);
    if (!f) return undefined;
    if (!fence) return f[1];
    return f[1][0] === fence[0] && f[1].length >= fence.length && !l.trim().slice(f[1].length).trim() ? null : fence;
  };
  function definitions(lines) {
    const refs = {}, notes = {}, body = [];
    let fence = null;
    for (let i = 0; i < lines.length; i++) {
      const l = lines[i];
      const toggled = fenceToggle(fence, l);
      if (toggled !== undefined || fence) { if (toggled !== undefined) fence = toggled; body.push(l); continue; }
      let m;
      if ((m = l.match(/^ {0,3}\[\^([^\]]+)\]:\s*(.*)$/))) {
        let t = m[2];
        while (i + 1 < lines.length && /^ {2,}\S/.test(lines[i + 1])) t += `\n${lines[++i].trim()}`;
        notes[m[1]] = t; continue;
      }
      if ((m = l.match(/^ {0,3}\[([^\]^][^\]]*)\]:\s*<?([^\s>]+)>?(?:\s+["'(](.*)["')])?\s*$/))) { refs[m[1].toLowerCase()] = { url: m[2], title: m[3] }; continue; }
      body.push(l);
    }
    return { refs, notes, body };
  }

  const link = (u, html, title) => `<a href="${esc(safeUrl(u))}"${title ? ` title="${esc(title)}"` : ''}>${html}</a>`;
  const emphasis = (s) => esc(s)
    .replace(/\*\*([^*\s](?:[^*]*?[^*\s])?)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^\w])__([^_\s](?:[^_]*?[^_\s])?)__(?!\w)/g, '$1<strong>$2</strong>')
    .replace(/(^|[^*\w])\*([^*\s](?:[^*]*?[^*\s])?)\*(?![*\w])/g, '$1<em>$2</em>')
    .replace(/(^|[^_\w])_([^_\s](?:[^_]*?[^_\s])?)_(?!\w)/g, '$1<em>$2</em>')
    .replace(/~~([^~\s](?:[^~]*?[^~\s])?)~~/g, '<del>$1</del>')
    .replace(/(?: {2,}|\\)\n/g, '<br>')
    .replace(/\n/g, ' ');

  // The inline renderer for one document: its link references and footnotes, numbered on first use.
  function inlineRenderer(refs, notes, noteOrder) {
    const footref = (id) => {
      if (notes[id] == null) return esc(`[^${id}]`);
      let n = noteOrder.indexOf(id) + 1;
      if (!n) n = noteOrder.push(id);
      return `<sup class="fnref"><a href="#fn-${esc(slugify(id))}" id="fnref-${esc(slugify(id))}">${n}</a></sup>`;
    };
    function inline(text) {
      const slots = [];
      const keep = (html) => `\u0000${slots.push(html) - 1}\u0000`;
      const reference = (m, t, id) => { const d = refs[id.toLowerCase()]; return d ? keep(link(d.url, inline(t), d.title)) : m; };
      let s = String(text);
      s = s.replace(/(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)/g, (_, t, c) => keep(`<code>${esc(/^ .* $/.test(c) ? c.slice(1, -1) : c)}</code>`));
      s = s.replace(/\\([\\`*_{}[\]()#+\-.!|>~<])/g, (_, c) => keep(esc(c)));
      s = s.replace(/<(https?:\/\/[^>\s]+)>/g, (_, u) => keep(link(u, esc(u))));
      s = s.replace(/<\/?[A-Za-z][^<>]*>/g, (tag) => { const t = tag.match(INLINE_TAG); return t ? keep(`<${t[1]}${t[2].toLowerCase()}>`) : tag; });
      s = s.replace(/!\[([^\]]*)\]\(\s*<?([^)\s>]+)>?(?:\s+"([^"]*)")?\s*\)/g, (_, a, u, t) => keep(`<img alt="${esc(a)}" src="${esc(safeUrl(u))}"${t ? ` title="${esc(t)}"` : ''} loading="lazy">`));
      s = s.replace(/\[((?:[^[\]]|\[[^\]]*\])*)\]\(\s*(?:<([^>\n]*)>|((?:[^()\s]|\([^()]*\))*))(?:\s+"([^"]*)")?\s*\)/g, (...g) => keep(link(g[2] ?? g[3], inline(g[1]), g[4])));
      s = s.replace(/\[\^([^\]\s]+)\]/g, (_, id) => keep(footref(id)));
      s = s.replace(/\[([^[\]]+)\]\[([^\]]*)\]/g, (m, t, id) => reference(m, t, id || t));
      s = s.replace(/\[([^[\]]+)\](?![(:[])/g, (m, t) => reference(m, t, t));
      // oxlint-disable-next-line no-control-regex -- \u0000 delimits placeholder slots; input cannot end a URL inside one.
      s = s.replace(/(^|[\s(])(https?:\/\/[^\s<>()\u0000]+[^\s<>().,;:!?'"\u0000])/g, (_, p, u) => p + keep(link(u, esc(u))));
      s = emphasis(s);
      // oxlint-disable-next-line no-control-regex -- restores the \u0000-delimited placeholder slots.
      for (let k = 0; k < 4 && s.includes('\u0000'); k++) s = s.replace(/\u0000(\d+)\u0000/g, (_, n) => slots[Number(n)]);
      return s;
    }
    return inline;
  }

  const LIST = /^( {0,3})([-*+]|\d{1,9}[.)])( +|$)/;
  const HR = /^ {0,3}([-*_])( *\1){2,} *$/;
  const FENCE = /^ {0,3}(`{3,}|~{3,})\s*([^\s`{]*)/;
  const DELIM = /^ {0,3}\|? *:?-+:? *(\| *:?-+:? *)*\|? *$/;
  const blank = (l) => !l.trim();
  const lead = (l) => l.match(/^ */)[0].length;
  const isTable = (ls, i) => ls[i].includes('|') && i + 1 < ls.length && ls[i + 1].includes('-') && DELIM.test(ls[i + 1]);
  const nextFilled = (ls, i) => { let j = i; while (j < ls.length && blank(ls[j])) j++; return j; };
  const splitRow = (r) => {
    let s = r.trim();
    if (s.startsWith('|')) s = s.slice(1);
    if (s.endsWith('|') && !s.endsWith('\\|')) s = s.slice(0, -1);
    const cells = [];
    let cur = '', tick = false;
    for (let k = 0; k < s.length; k++) {
      const c = s[k];
      if (c === '\\' && s[k + 1] === '|') { cur += '\\|'; k++; continue; }
      if (c === '`') tick = !tick;
      if (c === '|' && !tick) { cells.push(cur.trim()); cur = ''; continue; }
      cur += c;
    }
    cells.push(cur.trim());
    return cells;
  };
  const interrupts = (l) => FENCE.test(l) || /^ {0,3}(#{1,6}(\s|$)|>)/.test(l) || HR.test(l) || /^ {0,3}([-*+]|1[.)]) +\S/.test(l) || /^\s*<\/?(details|summary)\b/i.test(l);
  const cellAlign = (c) => (c.startsWith(':') && c.endsWith(':') ? 'center' : c.endsWith(':') ? 'right' : '');

  function codeBlock(code, lang) {
    if (lang === 'graph' || lang === 'chart') return `<pre class="widget-src" data-widget="${lang}"><code>${esc(code)}</code></pre>`;
    if (lang === 'mermaid') return `<pre class="mermaid-src"><code class="language-mermaid">${esc(code)}</code></pre>`;
    return `<pre${lang ? ` data-lang="${esc(lang)}"` : ''}><code${lang ? ` class="language-${esc(lang)}"` : ''}>${highlight(code, lang)}</code></pre>`;
  }

  // Block readers: each takes the parse state { ls, i, out, inline, tight }, and when the line at
  // `i` starts its block, appends the HTML, advances `i` past the block, and returns true.
  function fencedBlock(st) {
    const m = st.ls[st.i].match(FENCE);
    if (!m) return false;
    const { ls } = st;
    const mark = m[1], lang = (m[2] || '').toLowerCase(), indent = lead(ls[st.i]), buf = [];
    st.i++;
    while (st.i < ls.length) {
      const c = ls[st.i].match(/^ {0,3}(`{3,}|~{3,})\s*$/);
      if (c && c[1][0] === mark[0] && c[1].length >= mark.length) break;
      buf.push(ls[st.i].slice(Math.min(indent, lead(ls[st.i]))));
      st.i++;
    }
    st.i++;
    st.out.push(codeBlock(buf.join('\n'), lang));
    return true;
  }

  // One-line blocks: headings, rules, and <details>/<summary> lines.
  function lineBlock(st) {
    const l = st.ls[st.i];
    let m, html;
    if ((m = l.match(/^ {0,3}(#{1,6})(?:\s+(.*?))?(?:\s+#+)?\s*$/))) html = `<h${m[1].length}>${st.inline(m[2] || '')}</h${m[1].length}>`;
    else if (HR.test(l)) html = '<hr>';
    else if ((m = l.match(/^\s*<details(\s+open)?\s*>\s*$/i))) html = `<details${m[1] ? ' open' : ''}>`;
    else if ((m = l.match(/^\s*<summary>(.*)<\/summary>\s*$/i))) html = `<summary>${st.inline(m[1])}</summary>`;
    else if (/^\s*<\/details>\s*$/i.test(l)) html = '</details>';
    else return false;
    st.out.push(html);
    st.i++;
    return true;
  }

  function tableBlock(st) {
    const { ls, inline } = st;
    if (!isTable(ls, st.i)) return false;
    const head = splitRow(ls[st.i]);
    const align = splitRow(ls[st.i + 1]).map(cellAlign);
    const cell = (tag, c, k) => `<${tag}${align[k] ? ` style="text-align:${align[k]}"` : ''}>${inline(c)}</${tag}>`;
    let html = `<table><thead><tr>${head.map((c, k) => cell('th', c, k)).join('')}</tr></thead><tbody>`;
    st.i += 2;
    while (st.i < ls.length && !blank(ls[st.i]) && ls[st.i].includes('|') && !interrupts(ls[st.i])) {
      const row = splitRow(ls[st.i++]);
      html += `<tr>${head.map((_, k) => cell('td', row[k] ?? '', k)).join('')}</tr>`;
    }
    st.out.push(`${html}</tbody></table>`);
    return true;
  }

  function quoteBlock(st) {
    const { ls } = st;
    if (!/^ {0,3}>/.test(ls[st.i])) return false;
    const buf = [];
    while (st.i < ls.length && !blank(ls[st.i]) && (/^ {0,3}>/.test(ls[st.i]) || (buf.length && !interrupts(ls[st.i])))) buf.push(ls[st.i++].replace(/^ {0,3}> ?/, ''));
    const a = buf[0] && buf[0].match(/^\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*(.*)$/i);
    if (a) {
      const kind = a[1].toLowerCase(), rest = [a[2], ...buf.slice(1)].filter((x, k) => k || x.trim());
      st.out.push(`<div class="alert alert-${kind}" role="note"><p class="alert-title">${ALERTS[kind]}</p>${blocks(rest, undefined, st.inline)}</div>`);
    } else st.out.push(`<blockquote>${blocks(buf, undefined, st.inline)}</blockquote>`);
    return true;
  }

  // The lines after a list marker that belong to its item; sets `list.loose` on an inner blank gap.
  function listItemLines(st, list, width) {
    const { ls } = st;
    const content = [];
    while (st.i < ls.length) {
      const nx = ls[st.i];
      if (blank(nx)) {
        const j = nextFilled(ls, st.i);
        if (j >= ls.length || lead(ls[j]) < width) break;
        if (!LIST.test(ls[j].slice(width))) list.loose = true;
        while (st.i < j) { content.push(''); st.i++; }
        continue;
      }
      if (lead(nx) >= width) { content.push(nx.slice(width)); st.i++; continue; }
      if (LIST.test(nx) || interrupts(nx) || isTable(ls, st.i)) break;
      content.push(nx.trim()); st.i++;
    }
    return content;
  }

  // After an item: a blank gap followed by a sibling marker continues the (now loose) list.
  function continuesList(st, list, width) {
    const { ls } = st;
    if (st.i >= ls.length || !blank(ls[st.i])) return true;
    const j = nextFilled(ls, st.i);
    const nm = j < ls.length && ls[j].match(LIST);
    if (!nm || /\d/.test(nm[2]) !== list.ordered || nm[1].length >= width) return false;
    list.loose = true;
    st.i = j;
    return true;
  }

  function listItemHtml(c, loose, inline) {
    const task = c[0].match(/^\[([ xX])\]\s+/);
    if (task) c[0] = c[0].slice(task[0].length);
    const inner = blocks(c, !loose, inline);
    return task ? `<li class="task"><input type="checkbox" disabled${task[1] === ' ' ? '' : ' checked'}> ${inner}</li>` : `<li>${inner}</li>`;
  }

  function listBlock(st) {
    const { ls } = st;
    const m = ls[st.i].match(LIST);
    if (!m) return false;
    const ordered = /\d/.test(m[2]), start = ordered ? parseInt(m[2], 10) : 1, items = [];
    const list = { ordered, loose: false };
    while (st.i < ls.length) {
      const mm = ls[st.i].match(LIST);
      if (!mm || /\d/.test(mm[2]) !== ordered) break;
      const width = mm[1].length + mm[2].length + Math.min(Math.max(mm[3].length, 1), 4);
      const first = ls[st.i].slice(mm[0].length);
      st.i++;
      items.push([first, ...listItemLines(st, list, width)]);
      if (!continuesList(st, list, width)) break;
    }
    const lis = items.map((c) => listItemHtml(c, list.loose, st.inline));
    st.out.push(ordered ? `<ol${start !== 1 ? ` start="${start}"` : ''}>${lis.join('')}</ol>` : `<ul>${lis.join('')}</ul>`);
    return true;
  }

  // A paragraph, or a setext heading when underlined with = or -.
  function paragraph(st) {
    const { ls } = st;
    const buf = [ls[st.i].trim()];
    st.i++;
    let setext = 0;
    while (st.i < ls.length && !blank(ls[st.i])) {
      if (/^ {0,3}=+\s*$/.test(ls[st.i])) { setext = 1; st.i++; break; }
      if (/^ {0,3}-+\s*$/.test(ls[st.i])) { setext = 2; st.i++; break; }
      if (interrupts(ls[st.i]) || isTable(ls, st.i)) break;
      buf.push(ls[st.i].replace(/^ +/, ''));
      st.i++;
    }
    const text = st.inline(buf.join('\n'));
    st.out.push(setext ? `<h${setext}>${text}</h${setext}>` : st.tight ? text : `<p>${text}</p>`);
  }

  const BLOCKS = [fencedBlock, lineBlock, tableBlock, quoteBlock, listBlock];
  function blocks(ls, tight, inline) {
    const st = { ls, i: 0, out: [], inline, tight };
    while (st.i < ls.length) {
      if (blank(ls[st.i])) { st.i++; continue; }
      if (!BLOCKS.some((read) => read(st))) paragraph(st);
    }
    return st.out.join('\n');
  }

  function parse(src) {
    const lines = String(src).replace(/\r\n?/g, '\n').replace(/\t/g, '    ').split('\n');
    const { refs, notes, body } = definitions(lines);
    const noteOrder = [];
    const inline = inlineRenderer(refs, notes, noteOrder);
    let html = blocks(body, undefined, inline);
    if (noteOrder.length) {
      html += `<section class="footnotes"><ol>${noteOrder.map((id) => `<li id="fn-${esc(slugify(id))}">${inline(notes[id])} <a class="fnback" href="#fnref-${esc(slugify(id))}" aria-label="Back to reference">↩</a></li>`).join('')}</ol></section>`;
    }
    return html;
  }

  const TONES = [
    [/^(p0|sev0|sev1|critical|blocker|blocking|blocked|severe|fail|failed|failing|error|refuted|rejected|invalid|broken|high risk|must|no-go)$/i, 'red'],
    [/^(p1|sev2|high|major|warn|warning|at risk|partial|needs work|unverified|disputed|should|revise|revert|request changes)$/i, 'amber'],
    [/^(p2|sev3|medium|moderate|likely|in review|proposed|draft|open|in progress|ongoing|todo|could|investigate|waiting|comment)$/i, 'blue'],
    [/^(p3|low|minor|info|nit|deferred|optional|n\/a|unknown|skipped|superseded|inconclusive|no|none)$/i, 'gray'],
    [/^(pass|passed|ok|confirmed|verified|accepted|approved|done|resolved|fixed|ready|supported|keep|go|yes|complete|recommended|acceptable|accept|approve|✓|✔|✅)$/i, 'green'],
  ];
  // High/medium/low read as good or bad depending on the column or label they sit under:
  // "Fit: High" is good, "Risk: High" is bad. Without a known context they keep severity colors.
  const GOOD_SCALE = /fit|confidence|quality|score|readiness|coverage|value|benefit|strength|support|certainty|relevan|match|agreement|evidence/i;
  const SCALE = { high: 'green', strong: 'green', medium: 'blue', moderate: 'blue', mixed: 'blue', low: 'amber', weak: 'amber', none: 'red' };
  const toneOf = (text, context = '') => {
    const t = String(text).trim().replace(/[.:]$/, '');
    if (!t || t.length > 14) return '';
    if (context && GOOD_SCALE.test(context) && SCALE[t.toLowerCase()]) return SCALE[t.toLowerCase()];
    for (const [rx, tone] of TONES) if (rx.test(t)) return tone;
    return '';
  };

  // Data-only widgets: validate before loading a renderer; never execute report code.
  const fail = (message) => { throw new Error(message); };
  const str = (v) => typeof v === 'string' && v.trim().length > 0;
  function graphWidget(d) {
    if (!Array.isArray(d.nodes) || !d.nodes.length || !Array.isArray(d.edges)) fail('Provide nodes and edges arrays');
    const ids = new Set();
    const nodes = d.nodes.map(n => {
      if (!n || !str(n.id) || !str(n.label) || ids.has(n.id)) fail('Each node needs a unique id and a label');
      ids.add(n.id);
      if (n.url !== undefined && (typeof n.url !== 'string' || !/^https?:\/\//i.test(n.url) || !safeUrl(n.url))) fail('Node URLs must use HTTP or HTTPS');
      return { id: n.id, label: n.label, description: typeof n.description === 'string' ? n.description : '', url: n.url || '' };
    });
    const edges = d.edges.map(e => {
      if (!e || !ids.has(e.source) || !ids.has(e.target)) fail('Every edge must reference existing node IDs');
      return { source: e.source, target: e.target, label: typeof e.label === 'string' ? e.label : '' };
    });
    return { title: d.title, nodes, edges };
  }
  const finiteValues = (s, count) => Array.isArray(s.values) && s.values.length === count && s.values.every(v => typeof v === 'number' && Number.isFinite(v));
  function chartWidget(d) {
    if (!['bar', 'line', 'pie'].includes(d.type)) fail('Chart type must be bar, line, or pie');
    if (!Array.isArray(d.labels) || !d.labels.length || !d.labels.every(str)) fail('Provide nonempty labels');
    if (!Array.isArray(d.series) || !d.series.length) fail('Provide at least one series');
    const series = d.series.map(s => {
      if (!s || !str(s.name) || !finiteValues(s, d.labels.length)) fail('Every series needs a name and finite values matching labels');
      if (d.type === 'pie' && s.values.some(v => v < 0)) fail('Pie values cannot be negative');
      return { name: s.name, values: s.values };
    });
    if (d.type === 'pie' && series.length !== 1) fail('Pie charts require one series');
    return { title: d.title, type: d.type, labels: d.labels, series, unit: typeof d.unit === 'string' ? d.unit : '' };
  }
  function widget(kind, source) {
    const d = JSON.parse(source);
    if (!d || typeof d !== 'object' || Array.isArray(d)) fail('Expected a JSON object');
    if (!str(d.title)) fail('A title is required');
    return kind === 'graph' ? graphWidget(d) : chartWidget(d);
  }

  function widgetBlocks(body) {
    const entities = { '&amp;':'&', '&quot;':'"', '&#39;':"'", '&lt;':'<', '&gt;':'>' };
    return [...parse(body).matchAll(/<pre class="widget-src" data-widget="(graph|chart)"><code>([\s\S]*?)<\/code><\/pre>/g)].map(m => ({kind:m[1],source:m[2].replace(/&(?:amp|quot|lt|gt|#39);/g,e=>entities[e])}));
  }
  return { widgetBlocks, widget, esc, slugify, safeUrl, frontmatter, highlight, parse, toneOf, GOOD_SCALE, routeHref, routeParse, encodePath };
}
