#!/usr/bin/env node
import { cpSync, existsSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const skill = join(dirname(fileURLToPath(import.meta.url)), '..');
if (process.argv.includes('--help') || process.argv.includes('-h')) {
  console.log('Usage: portability-self-test.mjs\n\nCopies this skill to a temporary folder and tests standalone help, optional-dependency errors, forwarding, and the installed octocode-scraping integration when available.');
  process.exit(0);
}
const root = mkdtempSync(join(tmpdir(), 'octocode-chrome-portability-'));
try {
const copy = join(root, 'octocode-chrome-devtools');
cpSync(skill, copy, { recursive: true });

function run(script, args) {
  return spawnSync(process.execPath, [join(copy, 'scripts', script), ...args], { encoding: 'utf8', cwd: root });
}
function assert(value, message) { if (!value) throw new Error(message); }

for (const script of ['har-ingest-to-scrape.mjs', 'corpus-run-local.mjs']) {
  const help = run(script, ['--help']);
  assert(help.status === 0 && help.stdout.includes('--scraping-skill-dir'), `${script} standalone help failed`);
}

const har = join(root, 'finite.har');
const artifacts = join(root, 'artifacts');
writeFileSync(har, JSON.stringify({ log: { entries: [] } }));
mkdirSync(artifacts);
writeFileSync(join(artifacts, 'body.json'), '{"offerId":7}\n');

const sibling = join(root, 'octocode-scraping');
mkdirSync(join(sibling, 'scripts'), { recursive: true });
writeFileSync(join(sibling, 'scripts', 'har-ingest.mjs'), 'process.exit(0)\n');
const missing = run('har-ingest-to-scrape.mjs', ['--scraping-skill-dir', join(root, 'missing'), har]);
assert(missing.status !== 0 && missing.stderr.includes('OPTIONAL_DEPENDENCY_MISSING'), 'missing dependency contract unclear');
const duplicate = run('corpus-run-local.mjs', ['--scraping-skill-dir', sibling, '--scraping-skill-dir', sibling]);
assert(duplicate.status === 2 && duplicate.stderr.includes('INVALID_ARGUMENT'), 'duplicate dependency flag was not rejected');

const scraping = join(root, 'optional-scraping');
mkdirSync(join(scraping, 'scripts'), { recursive: true });
for (const [name, expected] of [['har-ingest.mjs', 'finite.har'], ['corpus-run.mjs', 'artifacts']]) {
  writeFileSync(join(scraping, 'scripts', name), `const ok=process.argv.some(x=>x.endsWith(${JSON.stringify(expected)})); console.log(JSON.stringify({ok,bridge:${JSON.stringify(name)}})); process.exit(ok?0:2);\n`);
}
const ingest = run('har-ingest-to-scrape.mjs', ['--scraping-skill-dir', scraping, har]);
const corpus = run('corpus-run-local.mjs', ['--scraping-skill-dir', scraping, '--artifact-dir', artifacts, '--regex', 'offerId']);
assert(ingest.status === 0 && ingest.stdout.includes('"ok":true'), 'HAR bridge did not forward finite fixture');
assert(corpus.status === 0 && corpus.stdout.includes('"ok":true'), 'corpus bridge did not forward finite fixture');
const realScraping = join(skill, '..', 'octocode-scraping');
let realIntegration = 'unavailable';
if (existsSync(join(realScraping, 'scripts', 'har-ingest.mjs'))) {
  const session = join(root, 'session');
  mkdirSync(session);
  writeFileSync(join(session, 'AGENT_INDEX.json'), '{}\n');
  const realIngest = run('har-ingest-to-scrape.mjs', ['--scraping-skill-dir', realScraping, '--session-dir', session, '--har', har]);
  const realCorpus = run('corpus-run-local.mjs', ['--scraping-skill-dir', realScraping, '--artifact-dir', artifacts, '--regex', 'offerId']);
  assert(realIngest.status === 0 && realIngest.stdout.includes('"ok": true'), 'real HAR integration failed');
  assert(realCorpus.status === 0 && realCorpus.stdout.includes('"ok": true'), 'real corpus integration failed');
  realIntegration = 'passed';
}
console.log(JSON.stringify({ ok: true, suite: 'chrome-devtools-portability', fixtures: 2, dependencyCases: 2, realIntegration }));
} finally {
  rmSync(root, { recursive: true, force: true });
}
