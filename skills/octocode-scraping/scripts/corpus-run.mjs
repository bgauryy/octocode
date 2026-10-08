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
import { readFile, writeFile, stat, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { paginate } from './lib/pagination.mjs';
import { pathToFileURL, fileURLToPath } from 'node:url';
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
const limit = Number(takeArg(args, '--limit', '50'));
if (!Number.isSafeInteger(limit) || limit < 1 || limit > 300) { console.error('Invalid limit'); process.exit(2); }
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
    let text, sourceSnapshot;
    try {
      const size = (await stat(file.abs)).size;
      if (size > maxFileBytes) {
        scanned.push({ rel: file.rel, skipped: 'max-file-bytes', bytes: size });
        continue;
      }
      const raw = await readFile(file.abs); text = raw.toString('utf8'); sourceSnapshot = createHash('sha256').update(raw).digest('hex');
    } catch (error) {
      scanned.push({rel:file.rel,error:error.message}); continue;
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
        next: { continue: { command: process.execPath, args: [fileURLToPath(new URL('./source-query.mjs', import.meta.url)), '--file', file.abs, '--snapshot', sourceSnapshot] } },
        snippet: text.slice(start, end).replace(/\s+/g, ' ').trim().slice(0, 240),
      });
      perFile += 1;
      if (m[0].length === 0) re.lastIndex += re.unicode && text.codePointAt(re.lastIndex) > 0xffff ? 2 : 1;
      if (!re.global) break;
    }

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
let scriptEvidence = null;
let inlineScriptResult = null;
if (scriptArg) {
  const absScript = safeScriptPath(scriptArg, process.cwd());
  const mod = await import(pathToFileURL(absScript).href);
  if (typeof mod.run !== 'function') {
    console.log(JSON.stringify({ ok: false, error: 'script must export async function run(ctx)' }));
    process.exit(1);
  }
  scriptResult = await mod.run(ctxBase);
  const value = JSON.stringify(scriptResult ?? null);
  inlineScriptResult = Buffer.byteLength(value) <= 4000 ? scriptResult : null;
  const digest = createHash('sha256').update(value).digest('hex');
  const directory = resolve(sessionDir || artifactDir, 'indexes', 'script-results');
  await mkdir(directory, { recursive: true });
  const file = resolve(directory, digest + '.json');
  await writeFile(file, value);
  scriptEvidence = { file, sha256: digest, next: { continue: { command: process.execPath, args: [fileURLToPath(new URL('./source-query.mjs', import.meta.url)), '--file', file, '--snapshot', digest] } } };
}

let regexResult = { matches: [], scanned: [] };
if (regexStr) regexResult = await runRegex();

const pagingArgs = args.filter((_,i)=>!['--script','--script-arg','--write-full-clean','--concat-parts'].includes(args[i])&&!['--script','--script-arg'].includes(args[i-1]));
const skippedFiles = regexResult.scanned.filter(row => row.skipped || row.error).map(row=>({...row,next:{continue:{command:process.execPath,args:[fileURLToPath(new URL('./source-query.mjs',import.meta.url)),'--file',files.find(file=>file.rel===row.rel).abs]}}}));
const paging = await paginate({lists:{matches:regexResult.matches,scanned:regexResult.scanned,skippedFiles},files:files.map(file=>file.abs),dir:sessionDir||artifactDir,args:pagingArgs,script:fileURLToPath(import.meta.url),defaultLimit:limit});
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
        result: { ...(inlineScriptResult && typeof inlineScriptResult === 'object' ? inlineScriptResult : { detail: inlineScriptResult }), ok: scriptResult?.ok !== false, ...scriptEvidence },
      }
    : null,
  ...paging,
  coverage: { complete: regexResult.scanned.every(row => !row.skipped && !row.error), maxFileBytes },
}, null, 2));
process.exit(ok ? 0 : 1);
