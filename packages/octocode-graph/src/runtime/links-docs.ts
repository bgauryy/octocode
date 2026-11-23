// Documentation → code links (F4 of the graph end-to-end RFC): every Markdown file under the graph root is read for
// the forms that point at files and symbols, each resolved to exactly one target or recorded as unresolved with its
// reason and candidates. Nothing is guessed and nothing is dropped:
//   md-link    [text](path#anchor), images and reference definitions: a file or directory relative to the doc
//   md-path    a backticked path (`src/a.ts`, `a.ts`): the doc's folder, its package root, the graph root, then a
//              unique suffix among the graph's files
//   md-cite    a backticked or linked `path:line[-end]` / `path#L1-L2`: the file, with the lines in range
//   md-symbol  a backticked code-shaped name (camelCase, snake_case, `Type::member`, `f()`) that names one graph
//              symbol the doc places: its section names the declaring file, or, for a qualified name, the doc sits
//              in the declaring file's package. Any other code-shaped mention is listed `unconfirmed` with its
//              candidates (for an agent's link: links-proposed.ts); plain words are no mentions. Measured on this
//              repository: spelling-only matches were wrong 9 times in 10, package-only single names 2 in 3.
// Markdown is scanned token-aware, not by regex over lines: fenced code, code spans and HTML comments hold text
// that looks like links. A citation keeps a hash of the cited lines and of the citing line, so a refresh marks it
// `stale` when the code changed under an unchanged doc line.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join, posix } from 'node:path';
import { entitiesNamed, type ContractEntity } from './links-contracts.ts';
import type { GraphData, Link, UnresolvedLink } from './types.ts';

/** A Markdown reference as the scanner saw it. */
interface Mention {
  kind: 'link' | 'span';
  text: string;
  line: number;
  section: number;
}

export interface ScannedMarkdown {
  mentions: Mention[];
  /** GitHub heading slugs, for `#anchor` checks. */
  slugs: Set<string>;
}

const FENCE = /^ {0,3}(`{3,}|~{3,})(.*)$/;
const HEADING = /^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$/;
const DEFINITION = /^ {0,3}\[([^\]]+)\]:\s*(<[^>]*>|\S+)/;

// GitHub's heading anchor: lower case, punctuation other than `-` and `_` removed, spaces as hyphens.
export function slug(text: string) {
  return text
    .toLowerCase()
    .replace(/`/g, '')
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .trim()
    .replace(/\s/g, '-');
}

// Inline references of one line: code spans (a backtick run closed by a run of the same length) and link
// destinations (`[text](dest)`, `![alt](dest)`): a `](` closes only an unescaped `[` outside code spans.
function inline(line: string, n: number, section: number, out: Mention[]) {
  const open: number[] = [];
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '\\') {
      i++;
      continue;
    }
    if (c === '`') {
      let k = i;
      while (line[k] === '`') k++;
      const run = k - i;
      let close = line.indexOf('`'.repeat(run), k);
      while (
        close >= 0 &&
        (line[close + run] === '`' || line[close - 1] === '`')
      )
        close = line.indexOf('`'.repeat(run), close + 1);
      if (close < 0) {
        i = k - 1;
        continue;
      }
      out.push({
        kind: 'span',
        text: line.slice(k, close).trim(),
        line: n,
        section,
      });
      i = close + run - 1;
      continue;
    }
    if (c === '<') {
      const end = line.indexOf('>', i);
      if (end > i && /^<[a-z][a-z0-9+.-]*:/i.test(line.slice(i, end))) {
        i = end;
        continue;
      }
    }
    if (c === '[') {
      open.push(i);
      continue;
    }
    if (c !== ']' || open.pop() === undefined || line[i + 1] !== '(') continue;
    let j = i + 2;
    if (line[j] === '<') {
      const end = line.indexOf('>', j);
      if (end < 0) continue;
      out.push({
        kind: 'link',
        text: line.slice(j + 1, end),
        line: n,
        section,
      });
      i = end;
      continue;
    }
    let depth = 0;
    const start = j;
    for (; j < line.length; j++) {
      if (line[j] === '\\') j++;
      else if (line[j] === '(') depth++;
      else if (line[j] === ')' && depth-- === 0) break;
      else if (/\s/.test(line[j]) && depth === 0) break;
    }
    // A destination ends at `)`, or at a space before a quoted title; anything else is not a link.
    const rest = line.slice(j).trimStart();
    const dest = line.slice(start, j);
    if (dest && (line[j] === ')' || /^["'(]/.test(rest)))
      out.push({ kind: 'link', text: dest, line: n, section });
    i = j;
  }
}

/** The references of one Markdown text, outside fenced code and HTML comments. */
export function scanMarkdown(text: string): ScannedMarkdown {
  const mentions: Mention[] = [];
  const slugs = new Set<string>();
  const seen = new Map<string, number>();
  let fence: { char: string; length: number } | null = null;
  let comment = false;
  let section = 0;
  text.split('\n').forEach((raw, i) => {
    const n = i + 1;
    const text0 = raw.replace(/\r$/, '');
    const f = FENCE.exec(text0);
    if (fence) {
      if (
        f &&
        f[1][0] === fence.char &&
        f[1].length >= fence.length &&
        !f[2].trim()
      )
        fence = null;
      return;
    }
    if (f && !(f[1][0] === '`' && f[2].includes('`'))) {
      fence = { char: f[1][0], length: f[1].length };
      return;
    }
    // HTML comments: drop every commented part of the line, across lines.
    let line = text0;
    if (comment) {
      const end = line.indexOf('-->');
      if (end < 0) return;
      line = line.slice(end + 3);
      comment = false;
    }
    line = line.replace(/<!--[\s\S]*?-->/g, ' ');
    const opened = line.indexOf('<!--');
    if (opened >= 0) {
      line = line.slice(0, opened);
      comment = true;
    }
    const h = HEADING.exec(line);
    if (h) {
      section = n;
      const base = slug(h[2]);
      const k = seen.get(base) ?? 0;
      seen.set(base, k + 1);
      slugs.add(k ? `${base}-${k}` : base);
    }
    const d = DEFINITION.exec(line);
    if (d) {
      mentions.push({
        kind: 'link',
        text: d[2].replace(/^<|>$/g, ''),
        line: n,
        section,
      });
      return;
    }
    inline(line, n, section, mentions);
  });
  return { mentions, slugs };
}

const SCHEME = /^[a-z][a-z0-9+.-]*:/i;
const IDENT =
  /^[A-Za-z_][A-Za-z0-9_]*(?:(?:::|\.)[A-Za-z_][A-Za-z0-9_]*)*(?:\(\))?$/;
const CITE = /^(.+?)(?::(\d+)(?:-(\d+))?(?::\d+)?)$/;
// camelCase, snake_case, a qualified name or a call: spelled as code, not as a word.
const CODE_SHAPED = /[a-z][A-Z]|_|::|\.|\(\)$/;
const hash = (text: string) =>
  createHash('sha256')
    .update(text.replace(/\s+/g, ' ').trim())
    .digest('hex')
    .slice(0, 16);

interface DocContext {
  root: string;
  files: Set<string>;
  dirs: Set<string>;
  extensions: Set<string>;
  /** Unique-suffix lookup: a path's tail segments → the files ending with them. */
  byTail: Map<string, string[]>;
  /** Symbol ids by local name (`run`, `Type.member`). */
  symbols: Map<string, string[]>;
  packageRoots: string[];
  /** Directory names anywhere in the tree: a path whose first segment is one is a claim about this tree. */
  dirNames: Set<string>;
  lines: (file: string) => string[] | null;
}

function context(
  D: GraphData,
  root: string,
  listing: readonly string[]
): DocContext {
  const files = new Set(listing);
  const dirs = new Set<string>();
  const byTail = new Map<string, string[]>();
  const extensions = new Set<string>();
  for (const f of files) {
    const parts = f.split('/');
    for (let i = 1; i < parts.length; i++)
      dirs.add(parts.slice(0, i).join('/'));
    for (let i = 0; i < parts.length; i++) {
      const tail = parts.slice(i).join('/');
      byTail.set(tail, [...(byTail.get(tail) ?? []), f]);
    }
    const ext = /\.([A-Za-z0-9]+)$/.exec(parts.at(-1)!)?.[1];
    if (ext) extensions.add(ext.toLowerCase());
  }
  const symbols = new Map<string, string[]>();
  for (const s of D.symbols ?? []) {
    const local = s.id.split('#')[1]?.replace(/@\d+$/, '') ?? '';
    for (const key of new Set([local, local.split('.').pop()!]))
      symbols.set(key, [...(symbols.get(key) ?? []), s.id]);
  }
  const packageRoots = [...files]
    .filter(f =>
      /(^|\/)(package\.json|Cargo\.toml|pyproject\.toml|go\.mod)$/.test(f)
    )
    .map(f => posix.dirname(f))
    .toSorted((a, b) => b.length - a.length);
  const cache = new Map<string, string[] | null>();
  const lines = (file: string) => {
    if (!cache.has(file)) {
      try {
        cache.set(file, readFileSync(join(root, file), 'utf8').split('\n'));
      } catch {
        cache.set(file, null);
      }
    }
    return cache.get(file)!;
  };
  const dirNames = new Set([...dirs].map(d => d.split('/').pop()!));
  return {
    root,
    files,
    dirs,
    extensions,
    byTail,
    symbols,
    packageRoots,
    dirNames,
    lines,
  };
}

const clean = (p: string) => {
  const n = posix.normalize(p).replace(/^\.\//, '');
  return n === '.' ? '' : n.replace(/\/$/, '');
};

// A path relative to the doc, its package root, or the graph root; then a unique tail among the graph's files.
// The package a file belongs to: the nearest directory above it with a manifest, or null.
const packageOf = (ctx: DocContext, file: string) =>
  ctx.packageRoots.find(r => r !== '.' && file.startsWith(`${r}/`)) ?? null;

// An unresolved path is recorded only when it claims this tree: relative to the doc, or starting with a directory
// name the tree has. Bare file names and user or tool paths (`.mcp.json`, `~/.octocode/x`, `owner/repo`) are not.
const anchored = (ctx: DocContext, path: string) =>
  path.startsWith('./') ||
  path.startsWith('../') ||
  (path.includes('/') && ctx.dirNames.has(path.split('/')[0]));

function resolvePath(
  ctx: DocContext,
  doc: string,
  path: string,
  linked: boolean
) {
  const docDir = posix.dirname(doc) === '.' ? '' : posix.dirname(doc);
  const pkg = packageOf(ctx, doc) ?? '.';
  const bases = linked
    ? [path.startsWith('/') ? '' : docDir]
    : [docDir, pkg && pkg !== '.' ? pkg : '', ''];
  for (const base of bases) {
    const p = clean(posix.join(base, path.replace(/^\//, '')));
    if (p.startsWith('..')) continue;
    if (ctx.files.has(p)) return { file: p };
    if (ctx.dirs.has(p) || p === '') return { file: p ? `${p}/` : './' };
  }
  if (linked) return { candidates: [] };
  const tail = clean(path);
  const matches = ctx.byTail.get(tail) ?? [];
  return matches.length === 1 ? { file: matches[0] } : { candidates: matches };
}

const looksLikePath = (ctx: DocContext, text: string) => {
  if (
    /\s|[*?{}[\]<>|$]/.test(text) ||
    SCHEME.test(text) ||
    text.startsWith('-')
  )
    return false;
  const last = text.split('/').pop()!;
  const ext = /\.([A-Za-z0-9]+)$/.exec(last)?.[1]?.toLowerCase();
  return (
    Boolean(ext && ctx.extensions.has(ext)) ||
    text.startsWith('./') ||
    text.startsWith('../')
  );
};

/** Links and unresolved references of every Markdown file in the listing. */
export function docLinks(
  D: GraphData,
  root: string,
  listing: readonly string[],
  previous: readonly Link[] = [],
  contracts: readonly ContractEntity[] = []
): { links: Link[]; unresolved: UnresolvedLink[] } {
  const ctx = context(D, root, listing);
  // A name no code declares may be one a contract declares (links-contracts.ts): `lspSearch` names a tool.
  const declaredBy = entitiesNamed(contracts);
  const before = new Map(
    previous.map(l => [
      `${l.rule}\0${l.source}\0${l.target}\0${l.lines?.join('-')}`,
      l,
    ])
  );
  const links: Link[] = [];
  const unresolved: UnresolvedLink[] = [];
  const docs = listing.filter(f => /\.(md|markdown|mdx)$/i.test(f));
  const scanned = new Map<string, ScannedMarkdown>();
  const scanOf = (doc: string) => {
    if (!scanned.has(doc)) {
      const text = ctx.lines(doc);
      scanned.set(doc, scanMarkdown(text ? text.join('\n') : ''));
    }
    return scanned.get(doc)!;
  };
  for (const doc of docs) {
    const docLines = ctx.lines(doc) ?? [];
    const { mentions } = scanOf(doc);
    const sectionFiles = new Map<number, Set<string>>();
    const push = (link: Omit<Link, 'check' | 'sourceHash'>) => {
      const sourceHash = hash(docLines[link.line - 1] ?? '');
      const key = `${link.rule}\0${link.source}\0${link.target}\0${link.lines?.join('-')}`;
      const prev = before.get(key);
      // A cited span that changed under an unchanged citing line is stale until the doc line changes.
      const stale =
        link.hash !== undefined &&
        prev?.sourceHash === sourceHash &&
        (prev.check === 'stale' || prev.hash !== link.hash);
      links.push({
        ...link,
        sourceHash,
        check: stale ? 'stale' : 'ok',
        ...(stale && { hash: prev.hash }),
      });
      const file = link.target.split('#')[0];
      const set =
        sectionFiles.get(mentionSection.get(link.line) ?? 0) ?? new Set();
      set.add(file);
      sectionFiles.set(mentionSection.get(link.line) ?? 0, set);
    };
    const mentionSection = new Map(mentions.map(m => [m.line, m.section]));
    const cite = (m: Mention, file: string, from: number, to: number) => {
      const lines = ctx.lines(file);
      if (!lines || from < 1 || to > lines.length || to < from) {
        unresolved.push({
          source: doc,
          line: m.line,
          text: m.text,
          rule: 'md-cite',
          reason: 'line-out-of-range',
        });
        return;
      }
      push({
        source: doc,
        line: m.line,
        target: file,
        lines: [from, to],
        relation: 'cites',
        rule: 'md-cite',
        hash: hash(lines.slice(from - 1, to).join('\n')),
      });
    };
    const symbolSpans: Mention[] = [];
    for (const m of mentions) {
      if (m.kind === 'link') {
        if (SCHEME.test(m.text) || m.text.startsWith('//')) continue;
        const [rawPath, fragment = ''] = m.text.split('#');
        let path: string;
        try {
          path = decodeURIComponent(rawPath.split('?')[0]);
        } catch {
          path = rawPath;
        }
        if (!path) {
          // A same-doc anchor.
          if (fragment && !scanOf(doc).slugs.has(fragment.toLowerCase()))
            unresolved.push({
              source: doc,
              line: m.line,
              text: m.text,
              rule: 'md-link',
              reason: 'no-such-anchor',
            });
          continue;
        }
        const found = resolvePath(ctx, doc, path, true);
        if (!found.file) {
          unresolved.push({
            source: doc,
            line: m.line,
            text: m.text,
            rule: 'md-link',
            reason: 'no-such-file',
          });
          continue;
        }
        const range = /^L(\d+)(?:-L?(\d+))?$/.exec(fragment);
        if (range) {
          cite(m, found.file, Number(range[1]), Number(range[2] ?? range[1]));
          continue;
        }
        if (
          fragment &&
          /\.(md|markdown|mdx)$/i.test(found.file) &&
          !scanOf(found.file).slugs.has(fragment.toLowerCase())
        ) {
          unresolved.push({
            source: doc,
            line: m.line,
            text: m.text,
            rule: 'md-link',
            reason: 'no-such-anchor',
          });
          continue;
        }
        push({
          source: doc,
          line: m.line,
          target: found.file,
          relation: 'references',
          rule: 'md-link',
        });
        continue;
      }
      const c = CITE.exec(m.text);
      if (c && c[2] && looksLikePath(ctx, c[1])) {
        const found = resolvePath(ctx, doc, c[1], false);
        if (found.file && !found.file.endsWith('/'))
          cite(m, found.file, Number(c[2]), Number(c[3] ?? c[2]));
        else if (found.candidates?.length || anchored(ctx, c[1]))
          unresolved.push({
            source: doc,
            line: m.line,
            text: m.text,
            rule: 'md-cite',
            reason: found.candidates?.length ? 'ambiguous' : 'no-such-file',
            ...(found.candidates?.length && { candidates: found.candidates }),
          });
        continue;
      }
      if (looksLikePath(ctx, m.text)) {
        const found = resolvePath(ctx, doc, m.text, false);
        if (found.file)
          push({
            source: doc,
            line: m.line,
            target: found.file,
            relation: 'references',
            rule: 'md-path',
          });
        else if (found.candidates?.length || anchored(ctx, m.text))
          unresolved.push({
            source: doc,
            line: m.line,
            text: m.text,
            rule: 'md-path',
            reason: found.candidates?.length ? 'ambiguous' : 'no-such-file',
            ...(found.candidates?.length && { candidates: found.candidates }),
          });
        continue;
      }
      if (IDENT.test(m.text)) symbolSpans.push(m);
    }
    // Symbols last, so the files a section names can place a name.
    const docPackage = packageOf(ctx, doc);
    for (const m of symbolSpans) {
      if (!CODE_SHAPED.test(m.text)) continue;
      const name = m.text.replace(/\(\)$/, '').replace(/::/g, '.');
      const parts = name.split('.');
      const key = parts.length >= 2 ? parts.slice(-2).join('.') : name;
      const candidates = ctx.symbols.get(key) ?? [];
      if (!candidates.length) {
        const entities = declaredBy.get(name) ?? [];
        if (entities.length === 1)
          push({
            source: doc,
            line: m.line,
            target: entities[0].id,
            relation: 'references',
            rule: 'md-contract',
          });
        else if (entities.length > 1)
          unresolved.push({
            source: doc,
            line: m.line,
            text: m.text,
            rule: 'md-contract',
            reason: 'ambiguous',
            candidates: entities.map(e => e.id),
          });
        // A name neither code nor a contract declares is no reference here: `.env`, `sequenceDiagram`.
        continue;
      }
      const named = sectionFiles.get(m.section) ?? new Set();
      const cited = candidates.filter(id => named.has(id.split('#')[0]));
      const local =
        parts.length >= 2
          ? candidates.filter(
              id =>
                docPackage !== null &&
                packageOf(ctx, id.split('#')[0]) === docPackage
            )
          : [];
      const chosen = cited.length ? cited : local;
      if (chosen.length === 1)
        push({
          source: doc,
          line: m.line,
          target: chosen[0],
          relation: 'references',
          rule: 'md-symbol',
        });
      else
        // Not placed by the doc, or placed in several files: for review with its candidates, never a guess.
        unresolved.push({
          source: doc,
          line: m.line,
          text: m.text,
          rule: 'md-symbol',
          reason: chosen.length > 1 ? 'ambiguous' : 'unconfirmed',
          candidates: [...(chosen.length > 1 ? chosen : candidates)],
        });
    }
  }
  return { links, unresolved };
}
