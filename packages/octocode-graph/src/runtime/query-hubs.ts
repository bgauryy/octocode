// hubs [n|all] [<dir>]   files (under <dir>) ranked by what they cost the code that imports them: runtime importers ×
//                 the bytes one import loads (the file and everything it pulls in at run time). Type-only importers
//                 cost nothing and do not count; `--eager` counts only what loads at startup.
// hubs <file>     each runtime importer of the file: the names it takes from it, where each is declared, and whether
//                 it takes only names the file re-exports, so it can import them from their declaring files instead
//                 and skip the file's own closure. Names come from the engine's `uses` rows (data.json `uses`); calls
//                 into files the hub re-exports stand in where the engine resolved no name (a re-export from another
//                 chunk).
import {
  QueryError,
  evidenceOf,
  indexOf,
  resolveFile,
  type QueryIndex,
} from './query-index.ts';
import {
  fileRow,
  plural,
  type OpResult,
  type QueryArgs,
  type Row,
} from './query-ops.ts';
import { bytesOf, formatBytes } from './query-group.ts';
import { fileOf } from '../ids.ts';
import { keeps, loadOfKind } from '../loads.ts';

/** Rows `hubs` shows unless asked for `all` or a count. */
const SHOWN = 20;

/** Files and bytes a file loads at run time: itself and every file its kept imports reach. */
function closures(R: QueryIndex) {
  const { G } = R;
  const stamp = new Int32Array(G.n).fill(-1);
  const queue = new Int32Array(G.n);
  return (start: number) => {
    let head = 0,
      tail = 0,
      bytes = 0,
      known = true;
    queue[tail++] = start;
    stamp[start] = start;
    while (head < tail) {
      const u = queue[head++];
      const b = bytesOf(R, G.ids[u]);
      if (b == null) {
        if (!G.ids[u].startsWith('pkg:')) known = false;
      } else bytes += b;
      for (let k = G.out.offsets[u]; k < G.out.offsets[u + 1]; k++) {
        const v = G.out.targets[k];
        if (stamp[v] !== start) {
          stamp[v] = start;
          queue[tail++] = v;
        }
      }
    }
    return { files: tail, bytes: known ? bytes : null };
  };
}

const runtimeIndex = (I: QueryIndex) =>
  indexOf(I.D, { tests: I.tests, load: I.load ?? 'runtime' });

function ranking(I: QueryIndex, args: string[], dir: string | null): OpResult {
  const R = runtimeIndex(I);
  const closure = closures(R);
  const ranked: Row[] = [];
  for (let v = 0; v < R.G.n; v++) {
    const id = R.G.ids[v];
    const importers = R.G.in.offsets[v + 1] - R.G.in.offsets[v];
    if (!importers || id.startsWith('pkg:') || (dir && !id.startsWith(dir)))
      continue;
    const c = closure(v);
    const cost = c.bytes ?? c.files;
    ranked.push(
      fileRow(R, id, {
        importers,
        closureFiles: c.files,
        closureBytes: c.bytes,
        value: importers * cost,
      })
    );
  }
  ranked.sort(
    (a, b) => Number(b.value) - Number(a.value) || (a.id < b.id ? -1 : 1)
  );
  const all = (args[0] ?? '').toLowerCase() === 'all';
  const n = all ? ranked.length : Number.parseInt(args[0] ?? '', 10) || SHOWN;
  const shown = ranked.slice(0, n);
  const rows: Row[] = shown.map((r, i) => ({ ...r, rank: i + 1 }));
  const unit = shown.some(r => r.closureBytes == null)
    ? 'bytes (files where a size is missing)'
    : 'bytes';
  return {
    rows,
    edges: [],
    nodes: rows.map(r => r.id),
    stats: { ranked: ranked.length, shown: rows.length, load: R.load },
    summary: `top ${rows.length} of ${ranked.length} imported files${dir ? ` under ${dir}` : ''} by cost to importers: runtime importers × ${unit} one import loads${ranked.length > rows.length ? '; `hubs all` ranks all' : ''}; \`hubs <file>\` lists what each importer takes`,
  };
}

interface Taken {
  name: string;
  declaredIn: string;
  how: string;
}

function importersOf(I: QueryIndex, file: string): OpResult {
  const R = runtimeIndex(I);
  const v = R.G.index.get(file);
  if (v === undefined) throw new QueryError(`${file} is not in the file graph`);
  const closure = closures(R);
  const own = closure(v);
  // Files the hub re-exports, and the names each importer takes, by import line.
  const reexported = new Set(
    (I.D.imports ?? [])
      .filter(
        e => e.source === file && e.resolution.some(k => /reexport/.test(k))
      )
      .map(e => e.target)
  );
  const uses = Map.groupBy(
    (I.D.uses ?? []).filter(u => I.tests || !u.test),
    u => u.source
  );
  const callsBy = Map.groupBy(
    (I.D.calls ?? []).filter(c => I.tests || !c.test),
    c => fileOf(c.source)
  );
  const rows: Row[] = [];
  const verdicts: Record<string, number> = {};
  for (let k = R.G.in.offsets[v]; k < R.G.in.offsets[v + 1]; k++) {
    const importer = R.G.ids[R.G.in.targets[k]];
    // The import lines that run: a name an `import type` beside an `import()` takes is no runtime use.
    const ev = evidenceOf(R, importer, file);
    const load = R.load ?? 'runtime';
    const running = Object.entries(ev.kindLines).flatMap(([kind, at]) => {
      const l = loadOfKind(kind, ev.loads);
      return l && keeps(load, l) ? at : [];
    });
    const lines = new Set(
      Object.keys(ev.kindLines).length ? running : ev.imports
    );
    const taken: Taken[] = [];
    for (const u of uses.get(importer) ?? [])
      // A re-export of the hub's names (`export { x } from './hub'`) takes them too (`reexported-name`).
      if (u.line != null && lines.has(u.line))
        taken.push({
          name: u.target.includes('#')
            ? u.target.slice(u.target.indexOf('#') + 1)
            : '*',
          declaredIn: fileOf(u.target),
          how: u.detail,
        });
    // A name re-exported from another chunk has no `uses` row: a call into a file the hub re-exports stands in.
    const named = new Set(taken.map(t => `${t.declaredIn}#${t.name}`));
    for (const c of callsBy.get(importer) ?? []) {
      const target = fileOf(c.target);
      if (!reexported.has(target) || named.has(c.target)) continue;
      named.add(c.target);
      taken.push({
        name: c.target.slice(c.target.indexOf('#') + 1),
        declaredIn: target,
        how: 'call-through-reexport',
      });
    }
    const whole = taken.some(t => t.how === 'namespace');
    const elsewhere = taken.filter(t => t.declaredIn !== file);
    const verdict = whole
      ? 'whole-file'
      : !taken.length
        ? 'names-unknown'
        : elsewhere.length === taken.length
          ? 'reexports-only'
          : elsewhere.length
            ? 'mixed'
            : 'own-names';
    verdicts[verdict] = (verdicts[verdict] ?? 0) + 1;
    const direct = [...new Set(elsewhere.map(t => t.declaredIn))].toSorted();
    const directLoad =
      verdict === 'reexports-only'
        ? direct.reduce<{ files: number; bytes: number | null }>(
            (acc, d) => {
              const i = R.G.index.get(d);
              const c =
                i === undefined ? { files: 0, bytes: null } : closure(i);
              return {
                files: acc.files + c.files,
                bytes:
                  acc.bytes == null || c.bytes == null
                    ? null
                    : acc.bytes + c.bytes,
              };
            },
            { files: 0, bytes: 0 }
          )
        : null;
    rows.push(
      fileRow(R, importer, {
        line: Math.min(...lines),
        verdict,
        ...(ev.load && ev.load !== 'eager' && { load: ev.load }),
        names: taken.map(
          t =>
            `${t.name} (${t.how}${t.declaredIn === file ? '' : ` from ${t.declaredIn}`})`
        ),
        ...(direct.length && { direct }),
        ...(directLoad && { directClosure: directLoad }),
      })
    );
  }
  const order = [
    'reexports-only',
    'mixed',
    'names-unknown',
    'whole-file',
    'own-names',
  ];
  rows.sort(
    (a, b) =>
      order.indexOf(String(a.verdict)) - order.indexOf(String(b.verdict)) ||
      (a.id < b.id ? -1 : 1)
  );
  const parts = order
    .filter(x => verdicts[x])
    .map(x => `${x} ${verdicts[x]}`)
    .join(', ');
  return {
    rows,
    edges: rows.map(r => [r.id, file] as [string, string]),
    nodes: [file, ...rows.map(r => r.id)],
    stats: {
      importers: rows.length,
      closureFiles: own.files,
      closureBytes: own.bytes,
      verdicts,
      load: R.load,
    },
    summary: `${file}: ${plural(rows.length, 'runtime importer')}, each import loads ${plural(own.files, 'file')} ${formatBytes(own.bytes)}; ${parts || 'none'}. reexports-only: the importer takes only names declared elsewhere: import them from \`direct\` and skip this file's closure`,
  };
}

// `hubs <file>` asks about one file; `hubs [n|all] [<dir>]` ranks, under a directory when one is named.
export function hubs(I: QueryIndex, q: QueryArgs): OpResult {
  const count = q.args.filter(a => /^(\d+|all)$/i.test(a));
  const named = q.args.filter(a => !count.includes(a));
  if (named.length > 1)
    throw new QueryError(
      'hubs takes one file or directory: hubs [n|all] [<dir>] or hubs <file>'
    );
  const arg = named[0];
  if (!arg) return ranking(I, count, null);
  const dir = `${arg.replace(/^\.\//, '').replace(/\/+$/, '')}/`;
  if (!I.file.has(arg) && I.ids.some(id => id.startsWith(dir)))
    return ranking(I, count, dir);
  return importersOf(I, resolveFile(I, arg));
}
