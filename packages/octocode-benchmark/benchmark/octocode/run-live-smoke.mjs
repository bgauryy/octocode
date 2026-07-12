#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import path, { dirname, join, relative } from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const benchmarkRoot = dirname(here);
const packageRoot = dirname(benchmarkRoot);
const repoRoot = dirname(dirname(packageRoot));
const cliPath = join(repoRoot, 'packages/octocode/out/octocode.js');
const skipNetwork =
  process.argv.includes('--skip-network') || process.argv.includes('--no-network');

if (!existsSync(cliPath)) {
  console.error(
    `Built CLI not found at ${relative(repoRoot, cliPath)}. Run yarn build first.`
  );
  process.exit(2);
}

const generatedAt = new Date();
const timestamp = generatedAt
  .toISOString()
  .replace(/[-:]/g, '')
  .replace(/\.\d{3}Z$/, 'Z');
const benchmarkName = skipNetwork
  ? 'octocode-live-smoke-local'
  : 'octocode-live-smoke';
const runId = `${benchmarkName}-${timestamp}`;
const outDir = join(packageRoot, 'output', runId);
const rawDir = join(outDir, 'raw');
const schemesDir = join(outDir, 'schemes');
const artifactsDir = join(outDir, 'artifacts');

mkdirSync(rawDir, { recursive: true });
mkdirSync(schemesDir, { recursive: true });
mkdirSync(artifactsDir, { recursive: true });

const commands = [];
const flowResults = [];

function repoRelative(filePath) {
  return relative(repoRoot, filePath).split(path.sep).join('/');
}

function quoteArg(arg) {
  if (/^[A-Za-z0-9_./:=@+-]+$/.test(arg)) return arg;
  return JSON.stringify(arg);
}

function commandString(args) {
  return ['node', repoRelative(cliPath), ...args].map(quoteArg).join(' ');
}

function runGit(args) {
  const result = spawnSync('git', args, {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  return (result.stdout || '').trim();
}

function packageManager() {
  if (existsSync(join(repoRoot, 'yarn.lock'))) return 'yarn';
  if (existsSync(join(repoRoot, 'pnpm-lock.yaml'))) return 'pnpm';
  if (existsSync(join(repoRoot, 'package-lock.json'))) return 'npm';
  return 'unknown';
}

function writeJson(filePath, value) {
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function parseJson(text) {
  try {
    return { ok: true, value: JSON.parse(text) };
  } catch {
    return { ok: false, value: undefined };
  }
}

function rawExtension(text) {
  return parseJson(text).ok ? 'json' : 'txt';
}

function writeRaw(id, stream, text) {
  const ext = rawExtension(text);
  const fileName = `${id}.${stream}.${ext}`;
  writeFileSync(join(rawDir, fileName), text);
  return `raw/${fileName}`;
}

function assertJsonPath(value, pathParts) {
  let cursor = value;
  for (const part of pathParts) {
    if (cursor === undefined || cursor === null) return false;
    cursor = cursor[part];
  }
  return cursor !== undefined && cursor !== null;
}

function runCommand(id, args, options = {}) {
  const started = performance.now();
  const result = spawnSync(process.execPath, [cliPath, ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: { ...process.env, NO_COLOR: '1' },
    maxBuffer: 20 * 1024 * 1024,
  });
  const durationMs = Math.round(performance.now() - started);
  const stdout = result.stdout ?? '';
  const stderr = result.stderr ?? '';
  const stdoutFile = writeRaw(id, 'stdout', stdout);
  const stderrFile = writeRaw(id, 'stderr', stderr);
  const parsed = parseJson(stdout);
  const exitCode = result.status ?? 1;
  const expectedExitCodes = options.expectedExitCodes ?? [0];
  const checkOk = options.check ? options.check(parsed.value, stdout) : true;
  const status =
    expectedExitCodes.includes(exitCode) && checkOk ? 'pass' : 'fail';
  const command = {
    id,
    command: commandString(args),
    exitCode,
    durationMs,
    stdoutFile,
    stderrFile,
    status,
    jsonOk: parsed.ok,
    parsed: parsed.value,
  };
  commands.push(command);
  return { ...command, parsed: parsed.value };
}

function skippedFlow(name, reason) {
  return {
    name,
    commands: [],
    status: 'warn',
    evidence: {
      kind: 'skipped',
      answerReady: false,
      complete: false,
      reason,
    },
    outputQualityScore: 3,
    notes: reason,
  };
}

function runFlow(flow) {
  if (flow.network && skipNetwork) {
    const skipped = skippedFlow(flow.name, 'Skipped by --skip-network.');
    flowResults.push(skipped);
    return skipped;
  }

  const flowCommands = flow.commands.map(command =>
    runCommand(command.id, command.args, command)
  );
  const failCount = flowCommands.filter(command => command.status === 'fail')
    .length;
  const status = failCount > 0 ? 'fail' : 'pass';
  const evidence = flow.evidence
    ? flow.evidence(flowCommands)
    : {
        kind: flow.kind,
        answerReady: status === 'pass',
        complete: status === 'pass',
      };
  const result = {
    name: flow.name,
    commands: flowCommands.map(({ parsed: _parsed, jsonOk: _jsonOk, status: _status, ...command }) => command),
    status,
    evidence,
    anchors: flow.anchors ?? [],
    ...(flow.bestContinuation
      ? { bestContinuation: flow.bestContinuation }
      : {}),
    outputQualityScore: status === 'pass' ? 5 : 1,
    ...(flow.notes ? { notes: flow.notes } : {}),
  };
  flowResults.push(result);
  return result;
}

const localOql = JSON.stringify({
  target: 'code',
  from: { kind: 'local', path: 'packages/octocode/src/cli/commands/search.ts' },
  where: { kind: 'text', value: 'runOqlSearch' },
  controls: { search: { maxMatchesPerFile: 2, contextLines: 1 } },
  itemsPerPage: 1,
  page: 1,
});

const flows = [
  {
    name: 'metadata-and-schema',
    kind: 'schema',
    anchors: ['tools', 'localSearchCode scheme'],
    commands: [
      {
        id: 'META-TOOLS',
        args: ['tools', '--json', '--compact', '--no-color'],
        check: json => Number(json?.toolCount) >= 12,
      },
      {
        id: 'SCHEME-LOCAL-SEARCH',
        args: ['tools', 'localSearchCode', '--scheme', '--no-color'],
        check: (_json, stdout) => stdout.includes('localSearchCode'),
      },
    ],
    evidence: flowCommands => ({
      kind: 'schema',
      answerReady: true,
      complete: true,
      toolCount: flowCommands[0]?.parsed?.toolCount,
    }),
  },
  {
    name: 'local-grep-pagination',
    kind: 'local',
    anchors: ['packages/octocode/src/cli/commands/search.ts', 'runOqlSearch'],
    bestContinuation: 'Use returned next/search pagination from raw output.',
    commands: [
      {
        id: 'LOCAL-GREP',
        args: [
          'search',
          'runOqlSearch',
          'packages/octocode/src/cli/commands/search.ts',
          '--context-lines',
          '1',
          '--max-matches',
          '2',
          '--json',
          '--compact',
          '--no-color',
        ],
        check: json => assertJsonPath(json, ['results', 0]),
      },
      {
        id: 'LOCAL-ONLY-MATCHING',
        args: [
          'search',
          '--regex',
          'getString\\([^)]*\\)',
          'packages/octocode/src/cli/commands/search.ts',
          '--only-matching',
          '--unique',
          '--match-window',
          '12',
          '--max-matches',
          '5',
          '--json',
          '--compact',
          '--no-color',
        ],
        check: json => assertJsonPath(json, ['results', 0]),
      },
    ],
  },
  {
    name: 'local-ast-lsp-oql',
    kind: 'semantic-proof',
    anchors: ['AST', 'LSP documentSymbols', 'OQL target:code'],
    commands: [
      {
        id: 'LOCAL-AST',
        args: [
          'search',
          '--pattern',
          'getString($$$ARGS)',
          'packages/octocode/src/cli/commands/search.ts',
          '--lang',
          'ts',
          '--json',
          '--compact',
          '--no-color',
        ],
        check: json => assertJsonPath(json, ['results', 0]),
      },
      {
        id: 'LOCAL-LSP-SYMBOLS',
        args: [
          'search',
          'packages/octocode/src/cli/commands/search.ts',
          '--op',
          'documentSymbols',
          '--json',
          '--compact',
          '--no-color',
        ],
        check: json => assertJsonPath(json, ['results', 0]),
      },
      {
        id: 'LOCAL-OQL',
        args: ['search', '--query', localOql, '--json', '--compact', '--no-color'],
        check: json => assertJsonPath(json, ['results', 0]),
      },
    ],
  },
  {
    name: 'cache-fetch-file-tree-clone',
    kind: 'remote-as-local',
    network: true,
    anchors: ['pmndrs/zustand', 'README.md', 'src'],
    bestContinuation: 'Follow location.localPath for local search/AST/LSP proof.',
    commands: [
      {
        id: 'CACHE-FILE',
        args: [
          'cache',
          'fetch',
          'pmndrs/zustand',
          'README.md',
          '--depth',
          'file',
          '--json',
          '--no-color',
        ],
        check: json =>
          json?.success === true &&
          json?.location?.kind === 'file' &&
          typeof json?.localPath === 'string',
      },
      {
        id: 'CACHE-TREE',
        args: [
          'cache',
          'fetch',
          'pmndrs/zustand',
          'src',
          '--depth',
          'tree',
          '--json',
          '--no-color',
        ],
        check: json =>
          json?.success === true &&
          json?.location?.kind === 'directory' &&
          typeof json?.localPath === 'string',
      },
      {
        id: 'CACHE-CLONE',
        args: [
          'cache',
          'fetch',
          'pmndrs/zustand',
          'src',
          '--depth',
          'clone',
          '--json',
          '--no-color',
        ],
        check: json =>
          json?.success === true &&
          json?.location?.kind === 'repo' &&
          typeof json?.localPath === 'string',
      },
    ],
  },
  {
    name: 'external-package-github-remote-local',
    kind: 'external-workflow',
    network: true,
    anchors: ['zustand', 'pmndrs/zustand', 'createStore'],
    commands: [
      {
        id: 'NPM-PACKAGE',
        args: [
          'search',
          'zustand',
          '--target',
          'packages',
          '--json',
          '--compact',
          '--no-color',
        ],
        check: json => assertJsonPath(json, ['results', 0]),
      },
      {
        id: 'GH-CODE',
        args: [
          'search',
          'createStore',
          'pmndrs/zustand',
          '--lang',
          'ts',
          '--limit',
          '3',
          '--json',
          '--compact',
          '--no-color',
        ],
        check: json => assertJsonPath(json, ['results', 0]),
      },
      {
        id: 'REMOTE-AS-LOCAL',
        args: [
          'search',
          'createStore',
          'src',
          '--repo',
          'pmndrs/zustand',
          '--lang',
          'ts',
          '--limit',
          '3',
          '--json',
          '--compact',
          '--no-color',
        ],
        check: json => assertJsonPath(json, ['results', 0]),
      },
    ],
  },
];

for (const flow of flows) {
  runFlow(flow);
}

const schemeCommand = commands.find(command => command.id === 'SCHEME-LOCAL-SEARCH');
if (schemeCommand) {
  const source = join(outDir, schemeCommand.stdoutFile);
  if (existsSync(source)) {
    writeFileSync(
      join(schemesDir, 'localSearchCode.scheme.txt'),
      readFileSync(source, 'utf8')
    );
  }
}

const passFlows = flowResults.filter(flow => flow.status === 'pass').length;
const warnFlows = flowResults.filter(flow => flow.status === 'warn').length;
const failFlows = flowResults.filter(flow => flow.status === 'fail').length;
const verdictStatus = failFlows > 0 ? 'fail' : warnFlows > 0 ? 'warn' : 'pass';
const toolCount =
  commands.find(command => command.id === 'META-TOOLS')?.parsed?.toolCount ?? 0;
const ratings = {
  rawMcpTools: {
    score: toolCount >= 12 ? 9 : 6,
    reason: `tools --json reported ${toolCount} tool(s).`,
  },
  oqlSearch: {
    score: commands.find(command => command.id === 'LOCAL-OQL')?.status === 'pass' ? 9 : 5,
    reason: 'OQL local code route is exercised as a durable command artifact.',
  },
  quickCli: {
    score: failFlows === 0 ? 9 : 6,
    reason: 'search/cache quick commands are represented with raw outputs.',
  },
  flowQuality: {
    score: failFlows === 0 ? 9 : 5,
    reason: `${passFlows}/${flowResults.length} flow(s) passed.`,
  },
  schemaQuality: {
    score: 9,
    reason: 'The runner writes summary.json directly in output-run.schema.json shape.',
  },
  dataQuality: {
    score: failFlows === 0 ? 9 : 6,
    reason: 'Every command writes stdout/stderr files plus commands.ndjson.',
  },
  researchQuality: {
    score: failFlows === 0 ? 9 : 6,
    reason: 'The run covers grep, AST, LSP, OQL, and remote-as-local proof paths.',
  },
  outputQuality: {
    score: 9,
    reason: 'README, manifest, summary, commands, results, reflection, ratings, raw, schemes, and artifacts are all emitted.',
  },
  remoteAsLocal: {
    score:
      flowResults.find(flow => flow.name === 'cache-fetch-file-tree-clone')
        ?.status === 'pass'
        ? 9
        : skipNetwork
          ? 5
          : 4,
    reason: skipNetwork
      ? 'Remote-as-local flow was skipped by --skip-network.'
      : 'cache fetch file/tree/clone and search --repo are exercised.',
  },
};
const reflection = {
  whatWorked: [
    'Live benchmark output is created in the required artifact layout.',
    'The run exercises local grep, AST, LSP, OQL, and CLI metadata lanes.',
  ],
  whatDidNotWork:
    failFlows > 0
      ? [`${failFlows} flow(s) failed; inspect raw/*.stderr.* and commands.ndjson.`]
      : [],
  missing: skipNetwork
    ? ['Network-dependent GitHub/npm/cache flows were intentionally skipped.']
    : [],
  possibleImprovements: [
    'Promote selected live flows into a scheduled job with credentials and cache warming.',
  ],
  praises: [
    'The cache and search commands expose typed location/evidence fields that can be validated without parsing prose.',
  ],
  nextFix:
    'Keep adding focused live flows only when they assert durable evidence, not just command success.',
};
const summary = {
  schemaVersion: '1.0.0',
  benchmark: {
    name: benchmarkName,
    kind: skipNetwork ? 'cli-tools' : 'remote-as-local',
    recipe: 'packages/octocode-benchmark/benchmark/octocode/README.md',
    description: 'Automated durable live smoke run for Octocode CLI/tool workflows.',
  },
  run: {
    runId,
    timestampUtc: generatedAt.toISOString(),
    outputDir: repoRelative(outDir),
    gitCommit: runGit(['rev-parse', '--short', 'HEAD']) || '0000000',
    gitDirty: runGit(['status', '--short']).length > 0,
  },
  environment: {
    os: process.platform,
    arch: process.arch,
    node: process.version,
    packageManager: packageManager(),
    cliCommand: `node ${repoRelative(cliPath)}`,
    authState: 'unknown',
  },
  determinism: {
    cacheMode: skipNetwork ? 'not-applicable' : 'mixed',
    fixedInputs: [
      'packages/octocode/src/cli/commands/search.ts',
      '.octocode/eval-fixtures/sample.tgz',
      'pmndrs/zustand',
      'zustand',
      'createStore',
    ],
    paginationFixed: true,
    networkUsed: !skipNetwork,
    knownNondeterminism: skipNetwork
      ? []
      : ['GitHub and npm provider latency/rate limits can vary.'],
  },
  surfaces: [
    {
      name: 'raw tools',
      expected: 12,
      observed: Number(toolCount),
      status: toolCount >= 12 ? 'pass' : 'fail',
    },
    {
      name: 'live flows',
      expected: flowResults.length,
      observed: passFlows,
      status: verdictStatus,
      notes: `${warnFlows} warn/skipped, ${failFlows} failed.`,
    },
  ],
  flows: flowResults,
  ratings,
  reflection,
  verdict: {
    status: verdictStatus,
    summary: `${passFlows}/${flowResults.length} live smoke flow(s) passed.`,
    ...(failFlows > 0
      ? { blockingIssues: ['One or more live smoke flows failed.'] }
      : {}),
  },
};

writeJson(join(outDir, 'manifest.json'), {
  schemaVersion: '1.0.0',
  benchmarkName,
  runId,
  createdAt: generatedAt.toISOString(),
  outputDir: repoRelative(outDir),
  gitCommit: summary.run.gitCommit,
  gitDirty: summary.run.gitDirty,
  cliCommand: summary.environment.cliCommand,
  skipNetwork,
});
writeJson(join(outDir, 'summary.json'), summary);
writeJson(join(outDir, 'ratings.json'), ratings);
writeJson(join(outDir, 'results.json'), flowResults);
writeFileSync(
  join(outDir, 'commands.ndjson'),
  commands
    .map(command =>
      JSON.stringify({
        id: command.id,
        command: command.command,
        durationMs: command.durationMs,
        exitCode: command.exitCode,
        status: command.status,
        stdoutFile: command.stdoutFile,
        stderrFile: command.stderrFile,
      })
    )
    .join('\n') + '\n'
);
writeFileSync(
  join(outDir, 'README.md'),
  `# ${runId}\n\nAutomated Octocode live smoke benchmark.\n\n- Summary: summary.json\n- Commands: commands.ndjson\n- Raw outputs: raw/\n- Schemes: schemes/\n- Ratings: ratings.json\n- Reflection: reflection.md\n\nVerdict: ${summary.verdict.status} - ${summary.verdict.summary}\n`
);
writeFileSync(
  join(outDir, 'results.md'),
  [
    `# Results: ${runId}`,
    '',
    '| Flow | Status | Commands |',
    '|---|---|---:|',
    ...flowResults.map(
      flow => `| ${flow.name} | ${flow.status} | ${flow.commands.length} |`
    ),
    '',
  ].join('\n')
);
writeFileSync(
  join(outDir, 'reflection.md'),
  [
    '# Reflection',
    '',
    '## What Worked',
    ...reflection.whatWorked.map(item => `- ${item}`),
    '',
    '## What Did Not Work',
    ...(reflection.whatDidNotWork.length
      ? reflection.whatDidNotWork
      : ['No blocking failures recorded.']
    ).map(item => `- ${item}`),
    '',
    '## Missing',
    ...(reflection.missing.length ? reflection.missing : ['No required artifact is missing.']).map(
      item => `- ${item}`
    ),
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
  ].join('\n')
);

const validation = spawnSync(
  process.execPath,
  [join(here, 'validate-output-runs.mjs'), '--dir', repoRelative(outDir)],
  {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: 'inherit',
    env: { ...process.env, NO_COLOR: '1' },
  }
);

if ((validation.status ?? 1) !== 0) {
  process.exit(validation.status ?? 1);
}

console.log(`\nWrote ${repoRelative(outDir)}`);
if (verdictStatus === 'fail') process.exit(1);
