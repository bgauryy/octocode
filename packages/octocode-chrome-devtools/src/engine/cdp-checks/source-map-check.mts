/**
 * source-map: resolves one script's source map and maps generated positions to original sources.
 * Position convention: generated `line:col` has a 1-based line and a 0-based column (Chrome's
 * column); the original line is 1-based and its column 0-based.
 * Options: SOURCEMAP_SCRIPT (url or scriptId, required), SOURCEMAP_POSITIONS, SOURCEMAP_LIST_SOURCES,
 * SOURCEMAP_SAVE_SOURCES.
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import type { CheckSession } from '../cdp-runner-session.mjs';
import { errorMessage } from '../error-message.mjs';
import {
  originalPositionFor,
  parseSourceMap,
  readScriptMapText,
} from '../sourcemap-resolver.mjs';
import { collectScripts, type ScriptRow } from './script-collect.mjs';
import {
  confinedPath,
  fileStem,
  sanitizeSourcePath,
} from './script-source-paths.mjs';

const MAX_STDOUT_ITEMS = Math.max(
  1,
  Number.parseInt(process.env.MAX_STDOUT_ITEMS ?? '10', 10) || 10
);

const out = (line: string) => {
  process.stdout.write(`${line}\n`);
};

/** A position list `l:c,l:c`: 1-based lines, 0-based columns. */
export function parsePositions(text: string) {
  const ok: { line: number; column: number }[] = [];
  const bad: string[] = [];
  for (const raw of text
    .split(',')
    .map(p => p.trim())
    .filter(Boolean)) {
    const m = /^(\d+):(\d+)$/.exec(raw);
    if (m && Number(m[1]) >= 1)
      ok.push({ line: Number(m[1]), column: Number(m[2]) });
    else bad.push(raw);
  }
  return { ok, bad };
}

function findScript(scripts: Map<string, ScriptRow>, query: string) {
  const all = [...scripts.values()];
  const byId = scripts.get(query);
  if (byId) return { script: byId };
  const exact = all.filter(s => s.url === query);
  const pool = exact.length
    ? exact
    : all.filter(s => s.url && s.url.includes(query));
  const urls = [...new Set(pool.map(s => s.url))];
  if (urls.length > 1) return { ambiguous: urls };
  // The latest script of a reloaded url wins.
  return { script: pool[pool.length - 1] };
}

export async function run(cdp: CheckSession) {
  const query = process.env.SOURCEMAP_SCRIPT || '';
  const listSources = process.env.SOURCEMAP_LIST_SOURCES === '1';
  const saveSources = process.env.SOURCEMAP_SAVE_SOURCES === '1';
  const { ok: positions, bad } = parsePositions(
    process.env.SOURCEMAP_POSITIONS || ''
  );
  if (!query) {
    out('[FINDING] SOURCEMAP_SCRIPT_REQUIRED pass a script url or scriptId');
    process.exitCode = 1;
    return;
  }

  const scripts = await collectScripts(cdp);
  const found = findScript(scripts, query);
  if (found.ambiguous) {
    out(
      `[FINDING] SOURCEMAP_SCRIPT_AMBIGUOUS ${JSON.stringify({ query, matches: found.ambiguous.slice(0, MAX_STDOUT_ITEMS), count: found.ambiguous.length })}`
    );
    process.exitCode = 1;
    return;
  }
  const script = found.script;
  if (!script) {
    out(
      `[FINDING] SOURCEMAP_SCRIPT_NOT_FOUND ${JSON.stringify({ query, scriptsSeen: scripts.size })}`
    );
    process.exitCode = 1;
    return;
  }
  for (const p of bad)
    out(
      `[FINDING] SOURCEMAP_POSITION_INVALID ${JSON.stringify({ position: p, expected: 'line:column, 1-based line, 0-based column' })}`
    );
  if (bad.length) process.exitCode = 1;

  if (!script.sourceMapURL) {
    out(
      `[FINDING] SOURCEMAP_MISSING ${JSON.stringify({ scriptId: script.scriptId, url: script.url })}`
    );
    return;
  }

  let mapText: string;
  let parsed;
  try {
    mapText = await readScriptMapText(script.sourceMapURL, script.url);
    parsed = parseSourceMap(mapText);
  } catch (error) {
    const message = errorMessage(error);
    const code = message.startsWith('Refused')
      ? 'SOURCEMAP_REFUSED'
      : message.startsWith('Malformed')
        ? 'SOURCEMAP_MALFORMED'
        : 'SOURCEMAP_UNAVAILABLE';
    out(
      `[FINDING] ${code} ${JSON.stringify({ scriptId: script.scriptId, url: script.url, error: message })}`
    );
    process.exitCode = 1;
    return;
  }

  const raw = JSON.parse(mapText) as { sourcesContent?: unknown };
  const contents = Array.isArray(raw.sourcesContent) ? raw.sourcesContent : [];
  const stem = fileStem(script.url, `script-${script.scriptId}`);
  const mapName = `${stem}.map.json`;
  const used = new Set<string>();
  const sources = parsed.sources.map((source, i) => {
    const hasContent = typeof contents[i] === 'string';
    const row: {
      index: number;
      source: string | null;
      hasContent: boolean;
      savedAs?: string;
    } = {
      index: i,
      source: source ?? null,
      hasContent,
    };
    if (saveSources && hasContent) {
      let safe = sanitizeSourcePath(source ?? '', i);
      if (used.has(safe)) safe = `${i}-${safe.replace(/\//g, '_')}`;
      used.add(safe);
      const target = confinedPath(`${cdp.outputDir}/sources`, safe);
      if (target) {
        mkdirSync(dirname(target), { recursive: true });
        writeFileSync(target, contents[i] as string, { mode: 0o600 });
        row.savedAs = `sources/${safe}`;
      }
    }
    return row;
  });

  const mapped = positions.map(p => {
    const hit = originalPositionFor(parsed, p.line - 1, p.column);
    return {
      generated: `${p.line}:${p.column}`,
      original: hit
        ? {
            source: hit.source,
            line: hit.line,
            column: hit.col,
            name: hit.name,
          }
        : null,
    };
  });

  const map = cdp.saveArtifact(mapName, mapText, 'text');
  const index = cdp.saveArtifact('source-map-index.json', {
    scriptId: script.scriptId,
    url: script.url,
    sourceMapURL: script.sourceMapURL.startsWith('data:')
      ? 'data: URI'
      : script.sourceMapURL,
    map: mapName,
    positionConvention:
      'generated line 1-based, column 0-based; original line 1-based, column 0-based',
    sources,
    positions: mapped,
  });

  out(
    `[METRIC] SOURCEMAP script=${script.scriptId} sources=${sources.length} withContent=${sources.filter(s => s.hasContent).length} positions=${mapped.length} unmapped=${mapped.filter(m => !m.original).length}`
  );
  for (const m of mapped) {
    if (m.original)
      out(
        `[POSITION] ${m.generated} -> ${m.original.source}:${m.original.line}:${m.original.column}${m.original.name ? ` ${m.original.name}` : ''}`
      );
    else
      out(
        `[FINDING] SOURCEMAP_POSITION_UNMAPPED ${JSON.stringify({ generated: m.generated })}`
      );
  }
  if (listSources) {
    for (const s of sources.slice(0, MAX_STDOUT_ITEMS))
      out(
        `[SOURCE] ${s.index} ${s.source} content=${s.hasContent ? 'yes' : 'no'}${s.savedAs ? ` saved=${s.savedAs}` : ''}`
      );
    if (sources.length > MAX_STDOUT_ITEMS)
      out(
        `[SOURCE] ${MAX_STDOUT_ITEMS} of ${sources.length} sources shown; every row is in ${index.file}`
      );
  }
  if (saveSources) out(`[ARTIFACT] SOURCEMAP_SOURCES ${cdp.outputDir}/sources`);
  void map;
}
