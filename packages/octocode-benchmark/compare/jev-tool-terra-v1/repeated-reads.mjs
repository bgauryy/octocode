import fs from 'node:fs';
import path from 'node:path';

const home = path.resolve(process.argv[2] ?? '');
if (!process.argv[2] || !fs.existsSync(path.join(home, 'freeze.json'))) throw new Error('Pass the frozen campaign directory');
const cases = [];
for (let n = 1; n <= 30; n++) {
  const id = `Q${n}`, file = path.join(home, 'runs', id, 'calls.jsonl');
  if (!fs.existsSync(file)) continue;
  const seen = new Map(), reads = [];
  for (const call of fs.readFileSync(file, 'utf8').split('\n').filter(Boolean).map(JSON.parse)) {
    if (call.event !== 'call' || call.name !== 'ghGetFileContent') continue;
    for (const row of call.result?.structuredContent?.results ?? []) {
      const query = (call.input.queries ?? [call.input])[row.index];
      for (const source of row.data?.files ?? []) {
        const revision = source.resolvedBranch ?? (/^[a-f0-9]{40}$/i.test(query?.branch ?? '') ? query.branch : null);
        if (!revision || !source.content || !Array.isArray(source.sourceLineRanges) || source.contentView === 'symbols' || query?.minify === 'symbols') continue;
        const key = [row.data.owner, row.data.repo, revision, source.path].join('/');
        const prior = seen.get(key) ?? new Set(), lines = new Set();
        for (const range of source.sourceLineRanges) {
          if (!Number.isSafeInteger(range.start) || !Number.isSafeInteger(range.end) || range.start < 1 || range.end < range.start || range.end - range.start > 100000) throw new Error(`Invalid source range in ${id}`);
          for (let line = range.start; line <= range.end; line++) lines.add(line);
        }
        const repeated = [...lines].filter(line => prior.has(line)).length;
        reads.push({ call: call.id, row: row.index, key, sourceLineRanges: source.sourceLineRanges,
          sourceLineAppearances: lines.size, repeatedSourceLines: repeated,
          whollyPreviouslyReturned: lines.size > 0 && repeated === lines.size,
          contentBytes: Buffer.byteLength(source.content) });
        for (const line of lines) prior.add(line);
        seen.set(key, prior);
      }
    }
  }
  cases.push({ id, reads, sourceLineAppearances: reads.reduce((v, r) => v + r.sourceLineAppearances, 0),
    repeatedSourceLines: reads.reduce((v, r) => v + r.repeatedSourceLines, 0),
    whollyPreviouslyReturnedReads: reads.filter(r => r.whollyPreviouslyReturned).length });
}
const totals = Object.fromEntries(['sourceLineAppearances', 'repeatedSourceLines', 'whollyPreviouslyReturnedReads'].map(key => [key, cases.reduce((v, r) => v + r[key], 0)]));
const report = { scope: 'ghGetFileContent source-line range metadata with nonempty content, same owner/repo/resolved revision (or explicit full commit SHA)/path within a case. Counts each response once, ignoring duplicate MCP text. Excludes symbols views, missing bodies, unresolved refs, history patches and cross-case reuse. Ranges may include minified context, so this is source-span overlap, not duplicate rendered text, token savings, redundancy necessity, or proof the model ingested an unclipped response.', totals, cases };
fs.writeFileSync(path.join(home, 'repeated-reads.json'), JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify({ totals, cases: cases.map(({ id, reads, ...metrics }) => ({ id, ...metrics })) }));
