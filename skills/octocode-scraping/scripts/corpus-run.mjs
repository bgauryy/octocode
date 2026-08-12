#!/usr/bin/env node
/**
 * Local-iterate bridge: regex or trusted script over scrape corpus and/or CDP artifact dirs.
 * Best of scrape (corpus roots) + chrome-devtools (search disk before re-browser).
 *
 * Usage:
 *   corpus-run.mjs --session-dir <scrapeSession> --regex 'offerId|productId' [--roots raw,text,extracts,cdp]
 *   corpus-run.mjs --artifact-dir <.octocode/tmp/chrome-devtools/...> --regex 'items'
 *   corpus-run.mjs --session-dir <dir> --concat-parts --write-full-clean --regex 'headline'
 *   corpus-run.mjs --session-dir <dir> --script ./my-check.mjs [--script-arg k=v]
 *
 * Script contract: export async function run(ctx) where
 *   ctx = { root, roots, files, read, write, sessionDir, artifactDir, args, matches }
 *   return { ok, findings?, matches? } (optional)
 */
import { readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  takeArg,
  hasFlag,
  listFilesRecursive,
  defaultCorpusInclude,
  defaultCdpArtifactInclude,
  concatCleanParts,
  safeScriptPath,
  readJsonFile,
} from './lib/bridge.mjs';

function usage(code = 2) {
  console.error(`Usage:
  corpus-run.mjs --session-dir <scrapeSession> --regex <pattern> [--flags <gimsu>] [--roots raw,text,extracts,cdp,snippets] [--limit 50] [--concat-parts] [--write-full-clean]
  corpus-run.mjs --artifact-dir <chrome-devtools-run> --regex <pattern> [...]
  corpus-run.mjs --session-dir <dir> --script <file.mjs> [--script-arg key=value]

Stdout is compact JSON (paths + match samples), never full file dumps.`);
  process.exit(code);
}

const args = process.argv.slice(2);
if (hasFlag(args, '--help') || hasFlag(args, '-h')) usage(0);

const sessionDirArg = takeArg(args, '--session-dir');
const artifactDirArg = takeArg(args, '--artifact-dir');
const regexStr = takeArg(args, '--regex');
const flags = takeArg(args, '--flags', 'gi');
const rootsCsv = takeArg(args, '--roots', 'raw,text,extracts,cdp,snippets');
const limit = Math.max(1, Math.min(500, Number(takeArg(args, '--limit', '50')) || 50));
const concatParts = hasFlag(args, '--concat-parts');
const writeFullClean = hasFlag(args, '--write-full-clean');
const scriptArg = takeArg(args, '--script');
const maxFileBytes = Math.max(1000, Number(takeArg(args, '--max-file-bytes', '2000000')) || 2_000_000);

if (!sessionDirArg && !artifactDirArg) usage();
if (!regexStr && !scriptArg) usage();

const sessionDir = sessionDirArg ? resolve(sessionDirArg) : null;
const artifactDir = artifactDirArg ? resolve(artifactDirArg) : null;
const roots = rootsCsv.split(',').map((s) => s.trim()).filter(Boolean);

if (sessionDir && !existsSync(sessionDir)) {
  console.log(JSON.stringify({ ok: false, error: `session-dir not found: ${sessionDir}` }));
  process.exit(1);
}
if (artifactDir && !existsSync(artifactDir)) {
  console.log(JSON.stringify({ ok: false, error: `artifact-dir not found: ${artifactDir}` }));
  process.exit(1);
}

const fullCleanWritten = [];
if (sessionDir && (concatParts || writeFullClean)) {
  const agent = await readJsonFile(join(sessionDir, 'AGENT_INDEX.json'), { pages: [] });
  const pageIds = (agent.pages || []).map((p) => p.pageId).filter(Boolean);
  // Also discover from text/ dir
  const textFiles = existsSync(join(sessionDir, 'text'))
    ? (await listFilesRecursive(join(sessionDir, 'text'), { include: (rel) => /\.clean\.part-/.test(rel) }))
    : [];
  for (const f of textFiles) {
    const m = f.rel.match(/^(page-\d+)\.clean\.part-/);
    if (m && !pageIds.includes(m[1])) pageIds.push(m[1]);
  }
  for (const pageId of [...new Set(pageIds)]) {
    const result = await concatCleanParts(sessionDir, pageId, { writeFull: writeFullClean || concatParts });
    if (result.fullRel) fullCleanWritten.push(result.fullRel);
  }
}

async function collectFiles() {
  const files = [];
  if (sessionDir) {
    const allow = new Set(roots);
    const listed = await listFilesRecursive(sessionDir, {
      include: (rel) => {
        if (!defaultCorpusInclude(rel) && !rel.startsWith('cdp/')) return false;
        if (rel === 'sources.jsonl' || rel === 'AGENT_INDEX.json') return roots.includes('extracts') || roots.includes('raw') || true;
        const top = rel.split('/')[0];
        if (['raw', 'text', 'extracts', 'cdp', 'snippets', 'indexes', 'graph'].includes(top)) return allow.has(top);
        return allow.has(top);
      },
    });
    for (const f of listed) {
      // Prefer full clean over parts when both exist and concat was requested
      if ((concatParts || writeFullClean) && /\.clean\.part-/.test(f.rel) && fullCleanWritten.length) {
        const pageId = f.rel.match(/^(?:text\/)?(page-\d+)\./)?.[1];
        if (pageId && fullCleanWritten.includes(`text/${pageId}.clean.md`)) continue;
      }
      files.push({ root: 'session', abs: f.abs, rel: f.rel, base: sessionDir });
    }
  }
  if (artifactDir) {
    const listed = await listFilesRecursive(artifactDir, { include: (rel) => defaultCdpArtifactInclude(rel) });
    for (const f of listed) files.push({ root: 'artifact', abs: f.abs, rel: f.rel, base: artifactDir });
  }
  return files;
}

const files = await collectFiles();

function lineCol(text, index) {
  const before = text.slice(0, index);
  const line = before.split(/\n/).length;
  const col = index - before.lastIndexOf('\n');
  return { line, column: col };
}

async function runRegex() {
  let re;
  try {
    re = new RegExp(regexStr, flags.includes('g') ? flags : `${flags}g`);
  } catch (error) {
    console.log(JSON.stringify({ ok: false, error: `invalid regex: ${error.message}` }));
    process.exit(1);
  }
  const matches = [];
  const scanned = [];
  for (const file of files) {
    let text;
    try {
      const buf = await readFile(file.abs);
      if (buf.length > maxFileBytes) {
        scanned.push({ rel: file.rel, skipped: 'max-file-bytes', bytes: buf.length });
        continue;
      }
      text = buf.toString('utf8');
    } catch {
      continue;
    }
    scanned.push({ rel: file.rel, bytes: Buffer.byteLength(text) });
    re.lastIndex = 0;
    let m;
    let perFile = 0;
    while ((m = re.exec(text)) !== null) {
      const { line, column } = lineCol(text, m.index);
      const start = Math.max(0, m.index - 80);
      const end = Math.min(text.length, m.index + (m[0]?.length || 0) + 80);
      matches.push({
        file: file.rel,
        root: file.root,
        abs: file.abs,
        line,
        column,
        match: m[0],
        groups: m.slice(1),
        snippet: text.slice(start, end).replace(/\s+/g, ' ').trim().slice(0, 240),
      });
      perFile += 1;
      if (matches.length >= limit || perFile >= Math.max(5, Math.floor(limit / 2))) break;
      if (!re.global) break;
    }
    if (matches.length >= limit) break;
  }
  return { matches, scanned };
}

const ctxBase = {
  root: sessionDir || artifactDir,
  sessionDir,
  artifactDir,
  roots,
  files: files.map((f) => ({ rel: f.rel, abs: f.abs, root: f.root })),
  args: Object.fromEntries(
    args.flatMap((a, i) => {
      if (a === '--script-arg' && args[i + 1]) {
        const [k, ...rest] = args[i + 1].split('=');
        return [[k, rest.join('=')]];
      }
      return [];
    }),
  ),
  async read(relOrAbs) {
    const abs = relOrAbs.startsWith('/') ? relOrAbs : join(sessionDir || artifactDir, relOrAbs);
    return readFile(abs, 'utf8');
  },
  async write(rel, content) {
    const base = sessionDir || artifactDir;
    const abs = join(base, rel);
    await writeFile(abs, content);
    return abs;
  },
  matches: [],
};

let scriptResult = null;
if (scriptArg) {
  const absScript = safeScriptPath(scriptArg, process.cwd());
  const mod = await import(pathToFileURL(absScript).href);
  if (typeof mod.run !== 'function') {
    console.log(JSON.stringify({ ok: false, error: 'script must export async function run(ctx)' }));
    process.exit(1);
  }
  scriptResult = await mod.run(ctxBase);
}

let regexResult = { matches: [], scanned: [] };
if (regexStr) regexResult = await runRegex();

const ok = scriptResult?.ok !== false;
console.log(JSON.stringify({
  ok,
  flow: 'local-iterate',
  sessionDir,
  artifactDir,
  roots,
  filesScanned: regexResult.scanned.length || files.length,
  fullCleanWritten,
  matchCount: regexResult.matches.length,
  matches: regexResult.matches,
  script: scriptArg
    ? {
        path: resolve(scriptArg),
        result: scriptResult && typeof scriptResult === 'object'
          ? {
              ok: scriptResult.ok !== false,
              findings: scriptResult.findings || scriptResult.matches || undefined,
              detail: scriptResult.detail || undefined,
            }
          : { ok: true, detail: scriptResult },
      }
    : null,
  next: regexResult.matches.length
    ? regexResult.matches.slice(0, 5).map((m) => ({ file: m.file, line: m.line, match: m.match }))
    : ['No regex hits — broaden pattern or ingest HAR bodies first'],
}, null, 2));
process.exit(ok ? 0 : 1);
