/**
 * script-sources: every script the page parses, with its full source saved to files.
 * Source text is evidence in files only; stdout carries the index lines.
 * Options: SCRIPTS_URL, SCRIPTS_RELOAD, SCRIPTS_MATCH, SCRIPTS_INLINE.
 */
import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { CheckSession } from '../cdp-runner-session.mjs';
import { errorMessage } from '../error-message.mjs';
import {
  collectScripts,
  documentUrls,
  loadAndSettle,
  type ScriptRow,
} from './script-collect.mjs';

type SourceRow = ScriptRow & {
  n: number;
  inline: boolean;
  file?: string;
  bytes?: number;
  sha256?: string;
  error?: string;
};

const MAX_STDOUT_ITEMS = Math.max(
  1,
  Number.parseInt(process.env.MAX_STDOUT_ITEMS ?? '10', 10) || 10
);

const out = (line: string) => {
  process.stdout.write(`${line}\n`);
};

export async function run(cdp: CheckSession) {
  const url = process.env.SCRIPTS_URL || undefined;
  const reload = process.env.SCRIPTS_RELOAD === '1';
  const match = process.env.SCRIPTS_MATCH || '';
  const inline = process.env.SCRIPTS_INLINE === '1';

  const scripts = await collectScripts(cdp);
  await loadAndSettle(cdp, { url, reload }, scripts);

  // Inline and eval scripts have no url, or carry their document's url.
  const documents = await documentUrls(cdp);
  const isInline = (u: string) => !u || documents.has(u);
  const wanted = [...scripts.values()].filter(
    s => (inline || !isInline(s.url)) && (!match || s.url.includes(match))
  );
  const dir = join(cdp.outputDir, 'scripts');
  mkdirSync(dir, { recursive: true });

  const rows: SourceRow[] = [];
  let failed = 0;
  for (const [i, s] of wanted.entries()) {
    const n = i + 1;
    const row: SourceRow = {
      n,
      ...s,
      inline: isInline(s.url),
    };
    try {
      const { scriptSource } = await cdp.send('Debugger.getScriptSource', {
        scriptId: s.scriptId,
      });
      const text = String(scriptSource ?? '');
      const hash = (s.hash || createHash('sha1').update(text).digest('hex'))
        .replace(/[^\w-]/g, '_')
        .slice(0, 64);
      const file = `scripts/${n}-${hash}.js`;
      writeFileSync(join(cdp.outputDir, file), text, { mode: 0o600 });
      row.file = file;
      row.bytes = Buffer.byteLength(text);
      row.sha256 = createHash('sha256').update(text).digest('hex');
    } catch (error) {
      failed++;
      row.error = errorMessage(error);
    }
    rows.push(row);
  }

  const index = cdp.saveArtifact('scripts.json', rows);
  const total = rows.reduce((sum, r) => sum + (r.bytes ?? 0), 0);
  out(
    `[METRIC] SCRIPTS count=${rows.length} seen=${scripts.size} bytes=${total} failed=${failed}`
  );
  for (const r of rows.slice(0, MAX_STDOUT_ITEMS))
    out(
      `[SCRIPT] ${r.n} ${r.url || '(inline)'} ${r.bytes === undefined ? 'unreadable' : r.bytes} sourceMap=${r.sourceMapURL ? 'yes' : 'no'}`
    );
  if (rows.length > MAX_STDOUT_ITEMS)
    out(
      `[SCRIPT] ${MAX_STDOUT_ITEMS} of ${rows.length} scripts shown; every row is in ${index.file}`
    );
  out(`[ARTIFACT] SCRIPT_SOURCES ${dir}`);
  for (const r of rows.filter(x => x.error))
    out(
      `[FINDING] SCRIPT_SOURCE_UNAVAILABLE ${JSON.stringify({ n: r.n, url: r.url, error: r.error })}`
    );
  if (!rows.length)
    out(
      '[FINDING] SCRIPTS_NONE no script matched; pass SCRIPTS_URL or SCRIPTS_RELOAD to capture a fresh load, or SCRIPTS_INLINE for inline scripts'
    );
}
