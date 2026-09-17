#!/usr/bin/env node
import { readFileSync, realpathSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';

const statuses = ['supported', 'contradicted', 'insufficient', 'conflicting'];
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const nonempty = value => typeof value === 'string' && value.trim().length > 0;
const sameKeys = (value, keys) => object(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const reject = reason => ({ usable: false, advisoryOnly: true, reason, next: 'Do not use this advice. Repair the packet or inspect the original evidence; do not repeat-vote on unchanged context.' });
export const hashResearchRequest = request => createHash('sha256').update(JSON.stringify(request)).digest('hex');

// Complements the Rust transport validator; does not verify source truth or freshness.
export function checkResearch(request, response) {
  if (!object(request?.state) || !nonempty(request.state.claim) || !Array.isArray(request.state.evidence)) return reject('Missing claim or evidence array.');
  const evidence = request.state.evidence;
  if (evidence.length === 0 || evidence.length > 254) return reject('Supply 1–254 evidence items, leaving one Choice option for none.');
  if (evidence.some(e => !object(e) || !nonempty(e.id) || e.id === 'none' || !nonempty(e.source) || !nonempty(e.scope) || !nonempty(e.content))) return reject('Each evidence item needs a non-reserved ID, source, scope and exact content.');
  const ids = evidence.map(e => e.id);
  if (new Set(ids).size !== ids.length) return reject('Evidence IDs must be unique.');
  const bases = request.state.evidence_bases;
  if (!Array.isArray(bases) || bases.length === 0 || bases.length > 254) return reject('Supply 1–254 evidence bases, leaving one Choice option for none.');
  if (bases.some(b => !object(b) || !nonempty(b.id) || b.id === 'none' || !nonempty(b.description) || !Array.isArray(b.evidenceIds) || b.evidenceIds.length === 0 || b.evidenceIds.some(id => !ids.includes(id)) || new Set(b.evidenceIds).size !== b.evidenceIds.length)) return reject('Each evidence basis needs a unique ID, description and one or more existing evidence IDs.');
  const basisIds = bases.map(b => b.id);
  if (new Set(basisIds).size !== basisIds.length) return reject('Evidence basis IDs must be unique.');
  const questions = request.questions;
  if (!sameKeys(questions, ['claim_status', 'decisive_basis']) || questions.claim_status?.type !== 'choice' || questions.decisive_basis?.type !== 'choice') return reject('Use exactly the two Choice questions in the research template.');
  if (!sameKeys(questions.claim_status.criteria, statuses) || !sameKeys(questions.decisive_basis.criteria, [...basisIds, 'none'])) return reject('Question candidates must match the status set and supplied evidence bases plus none.');
  if (!nonempty(response?.model)) return reject('Missing resolved model.');
  if (!nonempty(request.model) || ['jev-latest', 'jev-preview'].includes(request.model)) return reject('Pin a versioned request model for this research contract.');
  if (request.model !== response.model) return reject('Response model differs from the pinned request model.');
  const answers = response.answers;
  if (!sameKeys(answers, ['claim_status', 'decisive_basis']) || answers.claim_status?.type !== 'choice' || answers.decisive_basis?.type !== 'choice') return reject('Missing or unexpected research answers.');
  const status = answers.claim_status.choice;
  const basisId = answers.decisive_basis.choice;
  if (!statuses.includes(status) || ![...basisIds, 'none'].includes(basisId)) return reject('Answer is outside the supplied candidates.');
  const decisive = ['supported', 'contradicted'].includes(status);
  if (decisive === (basisId === 'none')) return reject('Claim status and decisive basis disagree.');
  const next = {
    supported: 'Independently confirm the scoped inference in the original source before citing it.',
    contradicted: 'Inspect the counterevidence and revise the claim only if it holds.',
    insufficient: 'Retrieve the missing evidence before reconsidering the claim.',
    conflicting: 'Check provenance and scope; obtain evidence that distinguishes the competing records.'
  }[status];
  const evidenceIds = basisId === 'none' ? [] : bases.find(b => b.id === basisId).evidenceIds;
  return { usable: true, advisoryOnly: true, model: response.model, status, basisId, evidenceIds, next };
}

function main(argv) {
  if (argv.length === 1 && ['--help', '-h'].includes(argv[0])) {
    console.log('Usage: node scripts/check-research.mjs --request request.json --response response-envelope.json\nThe response envelope must bind the exact request SHA-256. Local consistency check; no API calls.\nExit 0: internally coherent advice (not verified fact); 4: reject advice; 2: invalid files or options.');
    return;
  }
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    if (!['--request', '--response'].includes(key) || options[key] || !argv[index + 1] || argv[index + 1].startsWith('--')) throw new Error('options');
    options[key] = argv[index + 1];
  }
  if (!options['--request'] || !options['--response']) throw new Error('options');
  const read = path => {
    if (statSync(path).size > 4 * 1024 * 1024) throw new Error('size');
    return JSON.parse(readFileSync(path, 'utf8'));
  };
  const request = read(options['--request']);
  const envelope = read(options['--response']);
  if (!object(envelope) || envelope.protocol !== 'octocode-jev-research/v2' || envelope.requestSha256 !== hashResearchRequest(request) || !object(envelope.response)) {
    console.log(JSON.stringify(reject('Response envelope is missing or does not match the exact request.')));
    process.exitCode = 4;
    return;
  }
  const result = checkResearch(request, envelope.response);
  console.log(JSON.stringify(result));
  process.exitCode = result.usable ? 0 : 4;
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(resolve(process.argv[1]))).href) {
  try { main(process.argv.slice(2)); }
  catch { console.error(JSON.stringify({ error: 'Cannot check research advice. Supply readable JSON files up to 4 MiB each with --request and --response; use --help.' })); process.exitCode = 2; }
}
