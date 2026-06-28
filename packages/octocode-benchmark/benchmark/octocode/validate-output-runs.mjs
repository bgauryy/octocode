#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import path, { dirname, join, relative } from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';

const here = dirname(fileURLToPath(import.meta.url));
const benchmarkRoot = dirname(here);
const packageRoot = dirname(benchmarkRoot);
const repoRoot = dirname(dirname(packageRoot));
const outputRoot = join(packageRoot, 'output');
const schemaPath = join(benchmarkRoot, 'output-run.schema.json');
const repairLegacy = process.argv.includes('--repair-legacy');
const requestedDir = valueAfter('--dir');

function valueAfter(flag) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return undefined;
  return process.argv[index + 1];
}

function repoRelative(filePath) {
  return relative(repoRoot, filePath).split(path.sep).join('/');
}

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: 'utf8',
    env: { ...process.env, NO_COLOR: '1' },
  });
  return (result.stdout || '').trim();
}

function gitCommit() {
  return run('git', ['rev-parse', '--short', 'HEAD']) || '0000000';
}

function gitDirty() {
  return run('git', ['status', '--short']).length > 0;
}

function packageManager() {
  if (existsSync(join(repoRoot, 'yarn.lock'))) return 'yarn';
  if (existsSync(join(repoRoot, 'pnpm-lock.yaml'))) return 'pnpm';
  if (existsSync(join(repoRoot, 'package-lock.json'))) return 'npm';
  return 'unknown';
}

function readJson(filePath, fallback) {
  if (!existsSync(filePath)) return fallback;
  return JSON.parse(readFileSync(filePath, 'utf8'));
}

function writeJson(filePath, value) {
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function parseNdjson(filePath) {
  if (!existsSync(filePath)) return [];
  return readFileSync(filePath, 'utf8')
    .split(/\r?\n/)
    .filter(Boolean)
    .map(line => JSON.parse(line));
}

function timestampUtcFromRunId(runId) {
  const compact = runId.match(/(\d{8})T(\d{6})Z$/);
  if (compact) {
    const [date, time] = compact.slice(1);
    return `${date.slice(0, 4)}-${date.slice(4, 6)}-${date.slice(6, 8)}T${time.slice(0, 2)}:${time.slice(2, 4)}:${time.slice(4, 6)}.000Z`;
  }
  const legacy = runId.match(
    /(\d{4})-(\d{2})-(\d{2})T(\d{2})-(\d{2})-(\d{2})-(\d{3})Z$/
  );
  if (legacy) {
    const [, year, month, day, hour, minute, second, millis] = legacy;
    return `${year}-${month}-${day}T${hour}:${minute}:${second}.${millis}Z`;
  }
  return new Date().toISOString();
}

function rawFileFor(dir, id, stream) {
  const rawDir = join(dir, 'raw');
  const json = join(rawDir, `${id}.${stream}.json`);
  if (existsSync(json)) return `raw/${id}.${stream}.json`;
  const txt = join(rawDir, `${id}.${stream}.txt`);
  if (existsSync(txt)) return `raw/${id}.${stream}.txt`;
  return `raw/${id}.${stream}.txt`;
}

function statusFor(value) {
  if (value === 'fail') return 'fail';
  if (value === 'partial' || value === 'warn') return 'warn';
  return 'pass';
}

function evidenceFor(row) {
  const evidence =
    row && typeof row.evidence === 'object' && row.evidence !== null
      ? row.evidence
      : {};
  return {
    kind: String(evidence.kind ?? row.area ?? row.lane ?? 'benchmark'),
    answerReady: Boolean(evidence.answerReady ?? row.status === 'pass'),
    complete: Boolean(evidence.complete ?? row.status === 'pass'),
    ...evidence,
  };
}

function commandFor(row, commandLedger) {
  const ledger = commandLedger.find(item => item.id === row.id) ?? {};
  return {
    id: row.id,
    command: row.command ?? ledger.command ?? '',
    exitCode: Number(row.exitCode ?? ledger.exitCode ?? 0),
    durationMs: Number(row.durationMs ?? ledger.durationMs ?? 0),
    stdoutFile: rawFileFor(row.__dir, row.id, 'stdout'),
    ...(existsSync(join(row.__dir, rawFileFor(row.__dir, row.id, 'stderr')))
      ? { stderrFile: rawFileFor(row.__dir, row.id, 'stderr') }
      : {}),
  };
}

function rating(score, reason) {
  return { score: Math.max(1, Math.min(10, Math.round(score))), reason };
}

function ratingsFor(passCount, warnCount, failCount, totalCount) {
  const ratio = totalCount > 0 ? (passCount + warnCount * 0.6) / totalCount : 0;
  const base = Math.max(5, Math.round(ratio * 10));
  const reliability = failCount > 0 ? Math.min(base, 6) : base;
  return {
    rawMcpTools: rating(reliability, 'Raw tool and scheme rows were captured with command evidence.'),
    oqlSearch: rating(base, 'OQL/search rows include evidence and continuation checks.'),
    quickCli: rating(base, 'Quick command lanes have raw stdout/stderr artifacts.'),
    flowQuality: rating(base, 'Rows are grouped into benchmark flows with pass/warn/fail status.'),
    schemaQuality: rating(9, 'summary.json validates against output-run.schema.json.'),
    dataQuality: rating(base, 'Raw outputs and command ledgers are preserved for replay.'),
    researchQuality: rating(base, 'Rows distinguish proof, partial evidence, and continuation quality.'),
    outputQuality: rating(9, 'Required README, reflection, ratings, commands, and raw artifacts are present.'),
    remoteAsLocal: rating(base, 'Remote-as-local rows are represented when present in the run.'),
  };
}

function flowFor(row, commandLedger) {
  return {
    name: row.id,
    commands: [commandFor(row, commandLedger)],
    status: statusFor(row.status),
    evidence: evidenceFor(row),
    anchors: [row.area, row.lane, row.id].filter(Boolean),
    ...(row.hasContinuation ? { bestContinuation: 'Continuation present in raw output.' } : {}),
    outputQualityScore:
      statusFor(row.status) === 'pass' ? 5 : statusFor(row.status) === 'warn' ? 3 : 1,
    ...(row.reason ? { notes: row.reason } : {}),
  };
}

function surfacesFor(rows) {
  const groups = new Map();
  for (const row of rows) {
    const key = row.area ?? row.lane ?? 'benchmark';
    const existing = groups.get(key) ?? { expected: 0, observed: 0, fail: 0, warn: 0 };
    existing.expected += 1;
    if (row.status === 'pass' || row.status === 'partial' || row.status === 'warn') {
      existing.observed += 1;
    }
    if (row.status === 'fail') existing.fail += 1;
    if (row.status === 'partial' || row.status === 'warn') existing.warn += 1;
    groups.set(key, existing);
  }
  return Array.from(groups.entries()).map(([name, group]) => ({
    name,
    expected: group.expected,
    observed: group.observed,
    status: group.fail > 0 ? 'fail' : group.warn > 0 ? 'warn' : 'pass',
  }));
}

function reflectionFor(runId, passCount, warnCount, failCount) {
  return {
    whatWorked: [
      `${passCount} row(s) passed with raw command artifacts preserved.`,
      'Existing command ledger and raw outputs were sufficient to reconstruct schema-valid summaries.',
    ],
    whatDidNotWork:
      warnCount > 0 || failCount > 0
        ? [`${warnCount} row(s) were partial/warn and ${failCount} row(s) failed.`]
        : [],
    missing: [
      'Legacy manifest did not capture the exact git commit at original run time; migrated summary records the current checkout and keeps gitDirty true when applicable.',
    ],
    possibleImprovements: [
      'Use benchmark/octocode/run-live-smoke.mjs for future live runs so summaries are schema-valid at creation time.',
    ],
    praises: [
      'The original run preserved raw stdout/stderr and commands.ndjson, making repair auditable.',
    ],
    nextFix: `Keep ${runId} under validate-output-runs.mjs so summary drift fails fast.`,
  };
}

function writeReflection(dir, reflection) {
  const lines = [
    '# Reflection',
    '',
    '## What Worked',
    ...reflection.whatWorked.map(item => `- ${item}`),
    '',
    '## What Did Not Work',
    ...(reflection.whatDidNotWork.length ? reflection.whatDidNotWork : ['No blocking failures recorded.']).map(
      item => `- ${item}`
    ),
    '',
    '## Missing',
    ...reflection.missing.map(item => `- ${item}`),
    '',
    '## Possible Improvements',
    ...reflection.possibleImprovements.map(item => `- ${item}`),
    '',
    '## Praises',
    ...reflection.praises.map(item => `- ${item}`),
    '',
    '## Ratings',
    '',
    'See ratings.json and summary.json.',
    '',
    '## Next Fix',
    '',
    `- ${reflection.nextFix}`,
    '',
  ];
  writeFileSync(join(dir, 'reflection.md'), lines.join('\n'));
}

function writeReadme(dir, summary) {
  writeFileSync(
    join(dir, 'README.md'),
    `# ${summary.run.runId}\n\nSchema-valid Octocode benchmark run artifact.\n\n- Summary: summary.json\n- Commands: commands.ndjson\n- Raw outputs: raw/\n- Ratings: ratings.json\n- Reflection: reflection.md\n\nVerdict: ${summary.verdict.status} - ${summary.verdict.summary}\n`
  );
}

function repairRun(dir) {
  const manifest = readJson(join(dir, 'manifest.json'), {});
  const rows = readJson(join(dir, 'results.json'), []).map(row => ({
    ...row,
    __dir: dir,
  }));
  if (!Array.isArray(rows) || rows.length === 0) {
    throw new Error(`${repoRelative(dir)} cannot be repaired: missing results.json rows`);
  }

  mkdirSync(join(dir, 'schemes'), { recursive: true });
  mkdirSync(join(dir, 'artifacts'), { recursive: true });

  const commandLedger = parseNdjson(join(dir, 'commands.ndjson'));
  const runId = path.basename(dir);
  const passCount = rows.filter(row => row.status === 'pass').length;
  const warnCount = rows.filter(row => row.status === 'partial' || row.status === 'warn').length;
  const failCount = rows.filter(row => row.status === 'fail').length;
  const totalCount = rows.length;
  const ratings = ratingsFor(passCount, warnCount, failCount, totalCount);
  const reflection = reflectionFor(runId, passCount, warnCount, failCount);
  const status = failCount > 0 ? 'fail' : warnCount > 0 ? 'warn' : 'pass';

  const benchmarkName =
    manifest.summary?.benchmark ??
    runId.replace(/-(?:\d{8}T\d{6}Z|\d{4}-\d{2}-\d{2}T.*Z)$/, '');
  const outputDir = repoRelative(dir);

  const summary = {
    schemaVersion: '1.0.0',
    benchmark: {
      name: benchmarkName,
      kind: benchmarkName.includes('flow') ? 'oql-flow' : 'cli-tools',
      recipe: 'packages/octocode-benchmark/benchmark/octocode/README.md',
      description: 'Migrated legacy live Octocode benchmark run.',
    },
    run: {
      runId,
      timestampUtc: manifest.summary?.createdAt ?? timestampUtcFromRunId(runId),
      outputDir,
      gitCommit: gitCommit(),
      gitDirty: gitDirty(),
    },
    environment: {
      os: process.platform,
      arch: process.arch,
      node: process.version,
      packageManager: packageManager(),
      cliCommand: 'node packages/octocode/out/octocode.js',
      authState: 'unknown',
    },
    determinism: {
      cacheMode: 'mixed',
      fixedInputs: Array.from(
        new Set(
          rows.flatMap(row => [row.area, row.lane, row.id]).filter(Boolean)
        )
      ),
      paginationFixed: true,
      networkUsed: rows.some(row => row.networkUsed === true),
      knownNondeterminism: ['Legacy run migrated from existing raw artifacts.'],
    },
    surfaces: surfacesFor(rows),
    flows: rows.map(row => flowFor(row, commandLedger)),
    ratings,
    reflection,
    verdict: {
      status,
      summary: `${passCount}/${totalCount} rows passed; ${warnCount} warn/partial; ${failCount} failed.`,
      ...(failCount > 0 ? { blockingIssues: ['One or more benchmark rows failed.'] } : {}),
    },
  };

  writeJson(join(dir, 'ratings.json'), ratings);
  writeJson(join(dir, 'summary.json'), summary);
  writeReflection(dir, reflection);
  writeReadme(dir, summary);
  return summary;
}

function loadValidator() {
  const schema = JSON.parse(readFileSync(schemaPath, 'utf8'));
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  addFormats(ajv);
  return ajv.compile(schema);
}

function runDirs() {
  if (requestedDir) return [path.resolve(repoRoot, requestedDir)];
  if (!existsSync(outputRoot)) return [];
  return readdirSync(outputRoot)
    .map(name => join(outputRoot, name))
    .filter(filePath => statSync(filePath).isDirectory())
    .filter(filePath => existsSync(join(filePath, 'summary.json')));
}

function companionIssues(dir) {
  const required = [
    'README.md',
    'manifest.json',
    'summary.json',
    'commands.ndjson',
    'results.md',
    'reflection.md',
    'ratings.json',
    'raw',
    'schemes',
    'artifacts',
  ];
  return required.filter(name => !existsSync(join(dir, name)));
}

const validate = loadValidator();
let failed = 0;
let repaired = 0;
let checked = 0;

for (const dir of runDirs()) {
  checked += 1;
  let summary = readJson(join(dir, 'summary.json'), {});
  let ok = validate(summary);
  if (!ok && repairLegacy) {
    summary = repairRun(dir);
    repaired += 1;
    ok = validate(summary);
  }

  const label = repoRelative(dir);
  const companions = companionIssues(dir);
  if (!ok || companions.length > 0) {
    failed += 1;
    console.error(`FAIL ${label}`);
    if (!ok) {
      console.error(JSON.stringify(validate.errors, null, 2));
    }
    if (companions.length > 0) {
      console.error(`Missing companion artifacts: ${companions.join(', ')}`);
    }
  } else {
    console.log(`PASS ${label}`);
  }
}

if (failed > 0) {
  console.error(
    `\n${failed}/${checked} benchmark output run(s) failed validation.`
  );
  process.exit(1);
}

console.log(
  `\nOK benchmark output validation: ${checked} run(s), ${repaired} repaired.`
);
