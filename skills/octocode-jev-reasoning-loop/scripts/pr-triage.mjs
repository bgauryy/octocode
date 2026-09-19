#!/usr/bin/env node
// pr-triage.mjs — productized PR-triage scout: search rows -> one batched
// relevance judgment -> exactly one detail fetch recommendation.
//
//   node scripts/pr-triage.mjs --repo react/react --query "hydration mismatch" \
//     --question "which PR fixes the false-positive nonce mismatch" [--limit 8]
//
// Rows come from `gh api search/issues` (the GitHub search API does not follow
// repo renames — use canonical names). Output: ranked rows + the single PR to
// open. Verdicts are provisional; open the recommended PR before asserting.

import { execFileSync } from 'node:child_process';
import { parseFlags, print, stop } from './cli-json.mjs';
import { runScout } from './scout.mjs';

const options = parseFlags(process.argv.slice(2), ['--repo', '--query', '--question', '--limit'], ['--pretty']);
for (const key of ['--repo', '--query', '--question']) if (!options[key]) stop(`${key} is required.`, 2);
const limit = Math.min(Math.max(parseInt(options['--limit'] ?? '8', 10) || 8, 2), 12);

let rows;
try {
  const raw = execFileSync('gh', ['api',
    `search/issues?q=repo:${options['--repo']}+type:pr+${encodeURIComponent(options['--query'])}&per_page=${limit}`,
    '--jq', '[.items[] | {id: ("PR" + (.number|tostring)), source: ("' + options['--repo'] + '#" + (.number|tostring)), content: .title}]'
  ], { encoding: 'utf8', timeout: 30000 });
  rows = JSON.parse(raw);
} catch (error) {
  stop(`Cannot fetch PR rows via gh api: ${error.message.split('\n')[0]}`, 3);
}
if (!Array.isArray(rows) || rows.length < 2) stop(`Search returned ${rows?.length ?? 0} rows; need at least 2 to triage.`, 3);

const out = runScout({ claim: `the question: ${options['--question']}`, items: rows, taxonomy: 'relevance' });
const ranked = Object.entries(out.results)
  .map(([id, r]) => ({ id, source: rows.find(x => x.id === id)?.source, level: r.level, score: r.score, action: r.action }))
  .sort((a, b) => (b.score ?? -1) - (a.score ?? -1));
print({
  question: options['--question'],
  open_this: ranked.find(r => r.action === 'read') ?? ranked[0],
  ranked,
  jev_usage: out.metrics.jev,
  provisional: true,
  next: `gh api repos/${options['--repo']}/pulls/${(ranked.find(r => r.action === 'read') ?? ranked[0]).id.slice(2)} --jq .title`
}, options['--pretty']);
