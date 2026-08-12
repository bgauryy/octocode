#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

const here = new URL('.', import.meta.url).pathname;
const fetchScript = resolve(here, 'fetch.mjs');
const inspectScript = resolve(here, 'corpus-inspect.mjs');
const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log('Usage: fetch-and-brief.mjs --url <url> [fetch.mjs options...]');
  process.exit(0);
}
const fetched = spawnSync(process.execPath, [fetchScript, ...args], { encoding: 'utf8', cwd: process.cwd() });
let data = null;
try { data = JSON.parse(fetched.stdout); } catch {}
if (!data?.sessionDir) {
  process.stdout.write(fetched.stdout || '');
  process.stderr.write(fetched.stderr || '');
  process.exit(fetched.status || 1);
}
const inspected = spawnSync(process.execPath, [inspectScript, '--session-dir', data.sessionDir, '--limit', '10'], { encoding: 'utf8', cwd: process.cwd() });
let brief = null;
try { brief = JSON.parse(inspected.stdout); } catch {}
console.log(JSON.stringify({
  ok: fetched.status === 0 && inspected.status === 0,
  fetch: data,
  brief,
  agentNext: [
    `Read ${data.sessionDir}/AGENT_INDEX.json`,
    `Inspect ${data.sessionDir}/graph/workflows.json`,
    `Search with: node skills/octocode-scraping/scripts/corpus-find.mjs --session-dir ${data.sessionDir} --query <term>`
  ]
}, null, 2));
process.exit(fetched.status === 0 && inspected.status === 0 ? 0 : 1);
