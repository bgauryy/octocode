import { createReadStream, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

export async function paginate({ lists, files, dir, args, script, defaultLimit = 20 }) {
  try {
    const get = (flag, fallback) => args.includes(flag) ? args[args.indexOf(flag) + 1] : fallback;
    const limit = Number(get('--limit', defaultLimit)), view = get('--view');
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > 300 || view && !Object.hasOwn(lists, view)) throw new Error('Invalid limit or view');
    const hash = createHash('sha256');
    for (const file of files) { hash.update(file); if (existsSync(file)) for await (const chunk of createReadStream(file)) hash.update(chunk); }
    const snapshot = hash.digest('hex'); if (get('--snapshot') && get('--snapshot') !== snapshot) throw new Error('Corpus changed; restart the query');
    const next = {}, output = {}, pagination = {}; let budget = 16000;
    for (const [field, rows] of Object.entries(lists)) {
      output[field] = []; if (view && field !== view) continue;
      const cursor = Number(get('--cursor-' + field, 0)); if (!Number.isSafeInteger(cursor) || cursor < 0 || cursor > rows.length) throw new Error('Invalid cursor');
      let at = cursor;
      while (at < rows.length && output[field].length < limit) {
        let row = rows[at], encoded = JSON.stringify(row), bytes = Buffer.byteLength(encoded);
        if (bytes > 14000) {
          const cache = join(dir, 'indexes', 'query-values'); mkdirSync(cache, { recursive: true });
          const rowSnapshot = createHash('sha256').update(encoded).digest('hex');
          const source = join(cache, rowSnapshot + '.json');
          if (!existsSync(source)) writeFileSync(source, encoded, { mode: 0o600 });
          row = { sourceIndex: at, oversized: true, bytes, next: { continue: { command: process.execPath, args: [fileURLToPath(new URL('../source-query.mjs', import.meta.url)), '--file', source, '--snapshot', rowSnapshot] } } }; bytes = Buffer.byteLength(JSON.stringify(row));
        }
        if (bytes > budget) break;
        output[field].push(row); budget -= bytes; at++;
      }
      pagination[field] = { cursor, returned: output[field].length, total: rows.length };
      if (at < rows.length) {
        const filtered = args.filter((_, i) => !['--snapshot', '--view', '--cursor-' + field].includes(args[i]) && !['--snapshot', '--view', '--cursor-' + field].includes(args[i - 1]));
        next[field] = { command: process.execPath, args: [script, ...filtered, '--view', field, '--cursor-' + field, String(at), '--snapshot', snapshot] };
      }
    }
    return { ...output, pagination, snapshot, ...(Object.keys(next).length ? { next } : {}) };
  } catch (error) { console.error(JSON.stringify({ ok: false, error: error.message })); process.exit(2); }
}
