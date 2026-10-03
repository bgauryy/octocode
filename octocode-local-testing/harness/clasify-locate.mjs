// Requires a configured classification provider. Verifies bare next.clasify and deciding reads.
import fs from 'node:fs';
import path from 'node:path';
import { ROOT, startServer, nextHints, writeResults } from './mcp-client.mjs';
const dir = fs.mkdtempSync(ROOT + '/.octocode/tmp/improve-locate-');
const file = path.join(dir, 'retry-guide.md');
const lines = Array.from(
  { length: 429 },
  (_, i) =>
    `Line ${i + 1}: operational documentation filler explaining ordinary housekeeping and source ownership.`
);
lines[172] =
  'Retry only after a transient upstream timeout; stop retrying after three attempts.';
fs.writeFileSync(file, lines.join('\n') + '\n');
const c = await startServer({ timeoutMs: 120000 });
const pages = [];
const reads = [];
try {
  let e = await c.raw('clasify', {
    queries: [
      {
        goal: 'Locate the rule that limits retry attempts.',
        reasoning:
          'Read deciding source after the semantic scout identifies it.',
        resources: [
          {
            id: 'guide',
            tool: 'localFetch',
            query: { path: file, fullContent: true },
            maxChars: 10000,
          },
        ],
        questions: [
          { id: 'stop', type: 'locate', ask: 'When must a retry stop?' },
        ],
        debug: true,
      },
    ],
  });
  for (let i = 0; i < 10; i++) {
    pages.push(e);
    for (const h of nextHints(e.sc).filter(x => x.path.endsWith('.read')))
      reads.push(await c.raw(h.tool, h.query));
    const h = nextHints(e.sc).find(
      x => x.tool === 'clasify' && x.path.endsWith('.clasify')
    );
    if (!h) break;
    e = await c.raw(h.tool, h.query);
  }
  const ranges = pages.flatMap(e =>
    (e.sc?.queries ?? []).flatMap(q =>
      q.resources.flatMap(r =>
        (r.pages ?? []).map(p => p.scope).filter(Boolean)
      )
    )
  );
  const gapFree =
    ranges.length > 1 &&
    ranges[0].startLine === 1 &&
    ranges.at(-1).endLine === 429 &&
    ranges.every(
      (r, i) =>
        r.totalLines === 429 &&
        (!i || r.startLine === ranges[i - 1].endLine + 1)
    );
  const decidingRead = reads.some(e =>
    e.text.includes('stop retrying after three attempts')
  );
  const ok =
    pages.every(x => !x.isError && !x.rowErrors) &&
    reads.every(x => !x.isError && !x.rowErrors) &&
    !nextHints(pages.at(-1).sc).some(
      x => x.tool === 'clasify' && x.path.endsWith('.clasify')
    ) &&
    gapFree &&
    decidingRead;
  writeResults('clasify-locate', {
    ok,
    gapFree,
    decidingRead,
    ranges,
    pages,
    reads,
  });
  console.log(
    JSON.stringify({
      ok,
      gapFree,
      decidingRead,
      calls: pages.length,
      reads: reads.length,
      errors: pages.filter(x => x.isError || x.rowErrors).map(x => x.text),
    })
  );
  process.exitCode = ok ? 0 : 1;
} finally {
  c.close();
  fs.rmSync(dir, { recursive: true, force: true });
}
