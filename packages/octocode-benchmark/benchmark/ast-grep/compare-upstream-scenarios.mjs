#!/usr/bin/env node
// Optional CLI-to-CLI comparison using ast-grep's upstream benchmark scenarios.
//
// Upstream's public benchmark is an agent/outline benchmark. This runner reuses
// the same scenario repositories as a deterministic structural-search corpus and
// compares an installed ast-grep CLI with the Octocode CLI on identical temp
// file sets. It intentionally shells out to both CLIs.

import { spawnSync } from 'node:child_process'
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { createRequire } from 'node:module'
import { basename, dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { performance } from 'node:perf_hooks'
import { createHash } from 'node:crypto'
import { engine as sharedEngine } from '../_engine.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const benchmarkRoot = join(here, '..')
const packageRoot = join(benchmarkRoot, '..')
const monorepoRoot = join(packageRoot, '..', '..')
const defaultRepoDir = join(packageRoot, 'target', 'ast-grep-upstream', 'repos')
const defaultOutputDir = join(packageRoot, 'target', 'ast-grep-upstream')
const defaultOctocodeBin = join(monorepoRoot, 'packages', 'octocode', 'out', 'octocode.js')
const AST_GREP_BIN = process.env.AST_GREP_BIN || 'ast-grep'
const require = createRequire(import.meta.url)

const INSTALL_GUIDANCE = `ast-grep CLI is required for this optional comparison.

Install one of:
  brew install ast-grep
  npm install --global @ast-grep/cli
  pip install ast-grep-cli
  cargo install ast-grep --locked
  cargo binstall ast-grep

Then verify:
  ast-grep --version
`

const OCTOCODE_GUIDANCE = `Octocode CLI is required for this comparison.

Build the workspace CLI first:
  yarn workspace octocode build:dev

Or point to an installed CLI:
  OCTOCODE_BIN=/path/to/octocode yarn workspace @octocodeai/octocode-benchmark ast:compare:upstream
`

const COMPARISON_CASES = {
  'vscode-extension-host': {
    extensions: ['ts'],
    octocodeType: 'ts',
    kind: 'call_expression',
  },
  'excalidraw-render-update': {
    extensions: ['tsx'],
    octocodeType: 'tsx',
    kind: 'call_expression',
  },
  'django-queryset-execution': {
    extensions: ['py'],
    octocodeType: 'py',
    kind: 'call',
  },
  'tokio-runtime-scheduling': {
    extensions: ['rs'],
    octocodeType: 'rs',
    kind: 'call_expression',
  },
  'okhttp-interceptor-chain': {
    extensions: ['java'],
    octocodeType: 'java',
    kind: 'method_invocation',
  },
  'gin-middleware-routing': {
    extensions: ['go'],
    octocodeType: 'go',
    kind: 'call_expression',
  },
  'alamofire-request-lifecycle': {
    skip: 'Octocode structural search does not support Swift yet.',
  },
}

function parseArgs(argv) {
  const out = {
    syncRepos: false,
    scenario: null,
    repoDir: defaultRepoDir,
    outputDir: defaultOutputDir,
    filesPerScenario: 80,
    maxFileBytes: 350_000,
    repeats: 3,
    warmups: 1,
    keepCorpus: false,
    json: false,
    strict: false,
  }

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    const next = () => {
      const value = argv[++i]
      if (!value) throw new Error(`Missing value for ${arg}`)
      return value
    }

    if (arg === '--sync-repos') out.syncRepos = true
    else if (arg === '--scenario') out.scenario = next()
    else if (arg === '--repo-dir') out.repoDir = next()
    else if (arg === '--output-dir') out.outputDir = next()
    else if (arg === '--files-per-scenario') out.filesPerScenario = Number(next())
    else if (arg === '--max-file-bytes') out.maxFileBytes = Number(next())
    else if (arg === '--repeats') out.repeats = Number(next())
    else if (arg === '--warmups') out.warmups = Number(next())
    else if (arg === '--keep-corpus') out.keepCorpus = true
    else if (arg === '--json') out.json = true
    else if (arg === '--strict') out.strict = true
    else if (arg === '--help' || arg === '-h') {
      printHelp()
      process.exit(0)
    } else {
      throw new Error(`Unknown option: ${arg}`)
    }
  }

  if (!Number.isFinite(out.filesPerScenario) || out.filesPerScenario < 1) {
    throw new Error('--files-per-scenario must be a positive number')
  }
  if (!Number.isFinite(out.maxFileBytes) || out.maxFileBytes < 1) {
    throw new Error('--max-file-bytes must be a positive number')
  }
  if (!Number.isInteger(out.repeats) || out.repeats < 1) {
    throw new Error('--repeats must be a positive integer')
  }
  if (!Number.isInteger(out.warmups) || out.warmups < 0) {
    throw new Error('--warmups must be a non-negative integer')
  }
  return out
}

function printHelp() {
  console.log(`Usage:
  yarn workspace @octocodeai/octocode-benchmark ast:compare:upstream -- --sync-repos --scenario gin-middleware-routing

Options:
  --sync-repos              Clone missing upstream scenario repos into target/
  --scenario <name>         Run one scenario
  --repo-dir <path>         Scenario repo cache directory
  --files-per-scenario <n>  Deterministic file sample size per scenario (default: 80)
  --max-file-bytes <n>      Skip very large files (default: 350000)
  --repeats <n>             Fixed command repetitions; reports median ms (default: 3)
  --warmups <n>             Unmeasured warmup runs before fixed repetitions (default: 1)
  --keep-corpus             Keep temp corpora for inspection
  --json                    Print JSON summary instead of a table
  --strict                  Exit non-zero when match counts differ
`)
}

function commandSpecForOctocode() {
  const bin = process.env.OCTOCODE_BIN || defaultOctocodeBin
  if (bin.endsWith('.js')) return { command: process.execPath, baseArgs: [bin], label: bin }
  return { command: bin, baseArgs: [], label: bin }
}

function runCommand(command, args, options = {}) {
  const started = performance.now()
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    maxBuffer: 1024 * 1024 * 120,
    env: {
      ...process.env,
      NO_COLOR: '1',
    },
    ...options,
  })
  return {
    ...result,
    durationMs: performance.now() - started,
  }
}

function assertCliAvailable(octocode) {
  const astProbe = runCommand(AST_GREP_BIN, ['--version'])
  if (astProbe.error?.code === 'ENOENT') {
    console.error(INSTALL_GUIDANCE)
    process.exit(2)
  }
  if (astProbe.status !== 0) {
    console.error(`Could not run ${AST_GREP_BIN} --version`)
    console.error(astProbe.stderr || astProbe.stdout)
    process.exit(astProbe.status || 1)
  }

  const octoProbe = runCommand(octocode.command, [...octocode.baseArgs, '--version'])
  if (octoProbe.error?.code === 'ENOENT') {
    console.error(OCTOCODE_GUIDANCE)
    process.exit(2)
  }
  if (octoProbe.status !== 0) {
    console.error(`Could not run ${octocode.label} --version`)
    console.error(octoProbe.stderr || octoProbe.stdout)
    process.exit(octoProbe.status || 1)
  }

  return {
    astGrepVersion: astProbe.stdout.trim(),
    octocodeVersion: octoProbe.stdout.trim(),
  }
}

function loadScenarios(options) {
  const manifest = JSON.parse(readFileSync(join(here, 'upstream-outline-scenarios.json'), 'utf8'))
  const scenarios = manifest.scenarios.filter(
    scenario => !options.scenario || scenario.name === options.scenario,
  )
  if (options.scenario && scenarios.length === 0) {
    throw new Error(`Unknown scenario: ${options.scenario}`)
  }
  return { manifest, scenarios }
}

function syncRepo(options, scenario) {
  const path = join(options.repoDir, scenario.name)
  if (existsSync(path)) {
    if (!scenario.revision) return path
    const current = currentRepoRevision(path)
    if (current === scenario.revision) return path
    if (!options.syncRepos) {
      throw new Error(
        `Repo ${scenario.name} is at ${current || 'unknown revision'}, expected ${scenario.revision}. Re-run with --sync-repos.`
      )
    }

    const fetch = runCommand('git', ['-C', path, 'fetch', '--depth', '1', 'origin', scenario.revision], {
      stdio: options.json ? 'pipe' : 'inherit',
    })
    if (fetch.error) throw fetch.error
    if (fetch.status !== 0) throw new Error(`git fetch failed for ${scenario.name} revision ${scenario.revision}`)
    const checkout = runCommand('git', ['-C', path, 'checkout', '--detach', 'FETCH_HEAD'], {
      stdio: options.json ? 'pipe' : 'inherit',
    })
    if (checkout.error) throw checkout.error
    if (checkout.status !== 0) throw new Error(`git checkout failed for ${scenario.name} revision ${scenario.revision}`)
    return path
  }
  if (!options.syncRepos) return path

  mkdirSync(options.repoDir, { recursive: true })
  const result = scenario.revision
    ? runCommand('git', ['init', path], { stdio: options.json ? 'pipe' : 'inherit' })
    : runCommand('git', ['clone', '--depth', '1', scenario.repo, path], {
      stdio: options.json ? 'pipe' : 'inherit',
    })
  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(`git ${scenario.revision ? 'init' : 'clone'} failed for ${scenario.name} with exit ${result.status}`)
  }

  if (!scenario.revision) return path

  const remote = runCommand('git', ['-C', path, 'remote', 'add', 'origin', scenario.repo], {
    stdio: options.json ? 'pipe' : 'inherit',
  })
  if (remote.error) throw remote.error
  if (remote.status !== 0) throw new Error(`git remote add failed for ${scenario.name}`)

  const fetch = runCommand('git', ['-C', path, 'fetch', '--depth', '1', 'origin', scenario.revision], {
    stdio: options.json ? 'pipe' : 'inherit',
  })
  if (fetch.error) throw fetch.error
  if (fetch.status !== 0) throw new Error(`git fetch failed for ${scenario.name} revision ${scenario.revision}`)

  const checkout = runCommand('git', ['-C', path, 'checkout', '--detach', 'FETCH_HEAD'], {
    stdio: options.json ? 'pipe' : 'inherit',
  })
  if (checkout.error) throw checkout.error
  if (checkout.status !== 0) throw new Error(`git checkout failed for ${scenario.name} revision ${scenario.revision}`)
  return path
}

function currentRepoRevision(repoPath) {
  const result = runCommand('git', ['-C', repoPath, 'rev-parse', 'HEAD'])
  return result.status === 0 ? result.stdout.trim() : null
}

function gitTrackedFiles(repoPath) {
  const result = runCommand('git', ['-C', repoPath, 'ls-files'])
  if (result.status !== 0) return null
  return result.stdout.split('\n').map(line => line.trim()).filter(Boolean)
}

function walkFiles(root, prefix = '') {
  const entries = []
  for (const name of readdirSync(join(root, prefix), { withFileTypes: true })) {
    if (name.name === '.git' || name.name === 'node_modules' || name.name === 'target') continue
    const rel = prefix ? join(prefix, name.name) : name.name
    if (name.isDirectory()) entries.push(...walkFiles(root, rel))
    else if (name.isFile()) entries.push(rel)
  }
  return entries
}

function fileExtension(path) {
  const name = basename(path)
  const index = name.lastIndexOf('.')
  return index >= 0 ? name.slice(index + 1).toLowerCase() : ''
}

function hasHiddenPathSegment(path) {
  return path
    .split(/[\\/]/)
    .some(segment => segment.startsWith('.') && segment !== '.' && segment !== '..')
}

function selectFiles(repoPath, testCase, options) {
  const tracked = gitTrackedFiles(repoPath) ?? walkFiles(repoPath)
  const allowed = new Set(testCase.extensions)
  const files = []
  for (const rel of tracked.sort()) {
    if (hasHiddenPathSegment(rel)) continue
    if (!allowed.has(fileExtension(rel))) continue
    const abs = join(repoPath, rel)
    let stat
    try {
      stat = statSync(abs)
    } catch {
      continue
    }
    if (!stat.isFile() || stat.size > options.maxFileBytes) continue
    files.push({ rel, abs, bytes: stat.size })
    if (files.length >= options.filesPerScenario) break
  }
  return files
}

function materializeCorpus(options, scenario, files) {
  const corpusRoot = join(options.outputDir, 'corpus')
  mkdirSync(corpusRoot, { recursive: true })
  const dir = mkdtempSync(join(corpusRoot, `${scenario.name}-`))
  let bytes = 0
  for (const file of files) {
    const dest = join(dir, file.rel)
    mkdirSync(dirname(dest), { recursive: true })
    copyFileSync(file.abs, dest)
    bytes += file.bytes
  }
  return { dir, bytes }
}

function corpusHash(files) {
  const hash = createHash('sha256')
  for (const file of files) {
    hash.update(file.rel)
    hash.update('\0')
    hash.update(readFileSync(file.abs))
    hash.update('\0')
  }
  return hash.digest('hex')
}

function parseAstGrepJson(output) {
  const text = output.trim()
  if (!text) return []
  if (text.startsWith('[')) return JSON.parse(text)
  return text
    .split('\n')
    .map(line => line.trim())
    .filter(Boolean)
    .flatMap(line => {
      const parsed = JSON.parse(line)
      if (Array.isArray(parsed)) return parsed
      if (Array.isArray(parsed.matches)) return parsed.matches
      return [parsed]
    })
}

function astGrepFileOf(match) {
  return match.file || match.path || match.filePath || match.uri || match.range?.file || ''
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b)
  const mid = Math.floor(sorted.length / 2)
  return sorted.length % 2 === 0
    ? (sorted[mid - 1] + sorted[mid]) / 2
    : sorted[mid]
}

function assertStableCounts(label, runs) {
  const first = runs[0]
  for (const run of runs.slice(1)) {
    if (run.matches !== first.matches || run.files !== first.files) {
      throw new Error(
        `${label} produced non-deterministic counts across repeats: ${JSON.stringify(runs.map(({ matches, files }) => ({ matches, files })))}`
      )
    }
  }
}

function assertWarmupCounts(label, runs, warmups) {
  if (warmups.length === 0) return
  const first = runs[0]
  for (const run of warmups) {
    if (run.matches !== first.matches || run.files !== first.files) {
      throw new Error(
        `${label} warmup produced different counts than measured runs: ${JSON.stringify({
          warmups: warmups.map(({ matches, files }) => ({ matches, files })),
          measured: runs.map(({ matches, files }) => ({ matches, files })),
        })}`
      )
    }
  }
}

// Canonical engine — shared loader (index.cjs), no bespoke require path here.
function loadEngine() {
  return sharedEngine
}

let directToolModule = null
async function loadDirectToolModule() {
  if (!directToolModule) {
    // The localSearchCode lane is the CLI direct-tool path. Declare the CLI
    // runtime surface first — exactly as the octocode binary does — so local
    // tools are enabled without needing ENABLE_LOCAL (the CLI ignores it).
    const configModule = await import(
      pathToFileURL(
        join(
          monorepoRoot,
          'packages',
          'octocode-tools-core',
          'dist',
          'shared',
          'config',
          'index.js',
        ),
      ).href
    )
    configModule.setRuntimeSurface('cli')
    configModule.invalidateConfigCache?.()
    directToolModule = await import(
      pathToFileURL(join(monorepoRoot, 'packages', 'octocode-tools-core', 'dist', 'direct.js')).href
    )
  }
  return directToolModule
}

function runAstGrepOnce(corpusDir, testCase) {
  const result = runCommand(AST_GREP_BIN, [
    'run',
    '--json=stream',
    '--kind',
    testCase.kind,
    corpusDir,
  ])
  if (result.error) throw result.error
  if (result.status !== 0 && !(result.status === 1 && !result.stderr.trim())) {
    throw new Error(result.stderr.trim() || `${AST_GREP_BIN} exited ${result.status}`)
  }
  const matches = parseAstGrepJson(result.stdout)
  return {
    durationMs: result.durationMs,
    matches: matches.length,
    files: new Set(matches.map(astGrepFileOf).filter(Boolean)).size,
  }
}

function runAstGrep(corpusDir, testCase, options) {
  const warmups = Array.from({ length: options.warmups }, () => runAstGrepOnce(corpusDir, testCase))
  const runs = Array.from({ length: options.repeats }, () => runAstGrepOnce(corpusDir, testCase))
  assertStableCounts('ast-grep', runs)
  assertWarmupCounts('ast-grep', runs, warmups)
  return summarizeRuns(runs, warmups)
}

function ruleFor(testCase) {
  return `rule:\n  kind: ${testCase.kind}\n`
}

function includeGlobs(testCase) {
  return testCase.extensions.map(ext => `*.${ext}`)
}

async function runRawNativeOnce(corpusDir, testCase, maxFileBytes) {
  const engine = loadEngine()
  const started = performance.now()
  const result = await engine.structuralSearchFiles({
    path: corpusDir,
    rule: ruleFor(testCase),
    include: includeGlobs(testCase),
    excludeDir: [],
    maxFiles: 50_000,
    maxFileBytes,
  })
  return {
    durationMs: performance.now() - started,
    matches: result.totalMatches,
    files: result.files.length,
    parsedFiles: result.parsedFiles,
    skippedByPreFilter: result.skippedByPreFilter,
  }
}

async function runRawNative(corpusDir, testCase, options) {
  const warmups = []
  for (let i = 0; i < options.warmups; i++) {
    warmups.push(await runRawNativeOnce(corpusDir, testCase, options.maxFileBytes))
  }
  const runs = []
  for (let i = 0; i < options.repeats; i++) {
    runs.push(await runRawNativeOnce(corpusDir, testCase, options.maxFileBytes))
  }
  assertStableCounts('octocode raw native', runs)
  assertWarmupCounts('octocode raw native', runs, warmups)
  return summarizeRuns(runs, warmups)
}

function parseStructuredSearchResult(structured) {
  const data = structured?.results?.[0]?.data ?? structured?.data ?? structured
  const files = data?.files ?? structured?.files ?? []
  const pagination = data?.pagination ?? structured?.pagination ?? {}
  const totalMatches =
    pagination.totalMatches ??
    files.reduce((sum, file) => sum + (file.matchCount ?? file.matches?.length ?? 0), 0)
  return {
    matches: totalMatches,
    files: pagination.totalFiles ?? files.length,
  }
}

async function runLocalSearchToolOnce(corpusDir, testCase) {
  const { executeDirectTool } = await loadDirectToolModule()
  const started = performance.now()
  const result = await executeDirectTool('localSearchCode', {
    queries: [
      {
        id: 'layer-localSearchCode',
        path: corpusDir,
        mode: 'structural',
        rule: ruleFor(testCase),
        include: includeGlobs(testCase),
        maxFiles: 50_000,
        maxMatchesPerFile: 5000,
        itemsPerPage: 5000,
        page: 1,
        researchGoal: 'Benchmark Octocode structural search direct tool path',
        reasoning: 'Deterministic ast-grep comparison benchmark',
      },
    ],
  })
  const durationMs = performance.now() - started
  if (result.isError) {
    const text = result.content?.find(item => item.type === 'text')?.text
    throw new Error(text || 'localSearchCode returned an error')
  }
  return {
    durationMs,
    ...parseStructuredSearchResult(result.structuredContent),
  }
}

async function runLocalSearchTool(corpusDir, testCase, options) {
  const warmups = []
  for (let i = 0; i < options.warmups; i++) {
    warmups.push(await runLocalSearchToolOnce(corpusDir, testCase))
  }
  const runs = []
  for (let i = 0; i < options.repeats; i++) {
    runs.push(await runLocalSearchToolOnce(corpusDir, testCase))
  }
  assertStableCounts('octocode localSearchCode tool', runs)
  assertWarmupCounts('octocode localSearchCode tool', runs, warmups)
  return summarizeRuns(runs, warmups)
}

function parseOctocodeJson(output) {
  const parsed = JSON.parse(output)
  if (parsed?.success === false) throw new Error(parsed.error || 'octocode CLI returned success:false')
  const sc = parsed?.structuredContent ?? parsed
  const results = Array.isArray(sc?.results) ? sc.results : []

  // OQL CLI shape: `results` is a FLAT array of code-match rows
  // ({ kind:'code', path, line, ... }) — one row per match. Count the rows and
  // their distinct files.
  const codeRows = results.filter(
    r => r?.kind === 'code' || (r?.path && typeof r?.line === 'number'),
  )
  if (codeRows.length > 0) {
    return { matches: codeRows.length, files: new Set(codeRows.map(r => r.path)).size }
  }

  // Legacy grouped shape: results[0].data.files[].matches.
  const data = sc?.results?.[0]?.data ?? sc?.data ?? sc
  const files = data?.files ?? sc?.files ?? []
  const pagination = data?.pagination ?? sc?.pagination ?? {}
  const totalMatches =
    pagination.totalMatches ??
    files.reduce((sum, file) => sum + (file.matchCount ?? file.matches?.length ?? 0), 0)
  return { matches: totalMatches, files: pagination.totalFiles ?? files.length }
}

function runOctocodeOnce(octocode, corpusDir, testCase) {
  const rule = `rule:\n  kind: ${testCase.kind}\n`
  const result = runCommand(octocode.command, [
    ...octocode.baseArgs,
    'search',
    corpusDir,
    '--rule',
    rule,
    '--lang',
    testCase.octocodeType,
    '--json',
    // High caps so no scenario is truncated (the largest corpus tops 8k
    // matches); the comparison must count EVERY match to match ast-grep.
    '--limit',
    '200000',
    '--items-per-page',
    '200000',
    '--max-matches',
    '200000',
  ])
  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || result.stdout.trim() || `octocode exited ${result.status}`)
  }
  return {
    durationMs: result.durationMs,
    ...parseOctocodeJson(result.stdout),
  }
}

function runOctocode(octocode, corpusDir, testCase, options) {
  const warmups = Array.from(
    { length: options.warmups },
    () => runOctocodeOnce(octocode, corpusDir, testCase),
  )
  const runs = Array.from({ length: options.repeats }, () => runOctocodeOnce(octocode, corpusDir, testCase))
  assertStableCounts('octocode', runs)
  assertWarmupCounts('octocode', runs, warmups)
  return summarizeRuns(runs, warmups)
}

function summarizeRuns(runs, warmups = []) {
  const runDurationsMs = runs.map(run => run.durationMs)
  const warmupDurationsMs = warmups.map(run => run.durationMs)
  return {
    durationMs: median(runDurationsMs),
    minMs: Math.min(...runDurationsMs),
    maxMs: Math.max(...runDurationsMs),
    repeats: runs.length,
    warmups: warmups.length,
    ...(warmupDurationsMs.length > 0 ? {
      warmupMedianMs: median(warmupDurationsMs),
      warmupMinMs: Math.min(...warmupDurationsMs),
      warmupMaxMs: Math.max(...warmupDurationsMs),
    } : {}),
    runDurationsMs,
    warmupDurationsMs,
    matches: runs[0].matches,
    files: runs[0].files,
    ...(runs[0].parsedFiles !== undefined ? { parsedFiles: runs[0].parsedFiles } : {}),
    ...(runs[0].skippedByPreFilter !== undefined ? { skippedByPreFilter: runs[0].skippedByPreFilter } : {}),
  }
}

function percentDelta(octo, ast) {
  if (ast === 0) return octo === 0 ? '0%' : 'n/a'
  return `${(((octo - ast) / ast) * 100).toFixed(1)}%`
}

function pad(value, width) {
  return String(value).padEnd(width)
}

function printTable(summary) {
  console.log(`\nast-grep upstream scenario benchmark`)
  console.log(`source: ${summary.source.repo} ${summary.source.commit}`)
  console.log(`ast-grep: ${summary.versions.astGrepVersion}`)
  console.log(`octocode: ${summary.versions.octocodeVersion}`)
  console.log(`repoDir: ${summary.repoDir}`)
  console.log(`warmups: ${summary.options.warmups}; repeats: ${summary.options.repeats} fixed measured runs; displayed ms is measured median after warmup`)
  console.log(`warm ms: median warmup duration. It is shown separately and excluded from measured ms.`)
  console.log(`node/process note: public octocode search still pays Node process startup on every measured run. localSearchCode adds validation, sanitization, pagination, and result shaping. Raw native isolates matcher cost.`)
  console.log(`\n${pad('Scenario', 28)} ${pad('kind', 19)} ${pad('files', 7)} ${pad('hash', 10)} ${pad('lane', 28)} ${pad('warm ms', 9)} ${pad('ms', 9)} ${pad('matches', 8)} status`)
  console.log('-'.repeat(137))
  for (const row of summary.rows) {
    if (row.skipped) {
      console.log(`${pad(row.scenario, 28)} ${pad('-', 19)} ${pad('-', 7)} ${pad('-', 10)} ${pad('-', 28)} ${pad('-', 9)} ${pad('-', 9)} ${pad('-', 8)} SKIP ${row.skipped}`)
      continue
    }
    if (row.error) {
      for (const lane of layerRows(row)) {
        console.log(`${pad(row.scenario, 28)} ${pad(row.kind, 19)} ${pad(row.selectedFiles, 7)} ${pad(row.corpusHash?.slice(0, 8) ?? '-', 10)} ${pad(lane.name, 28)} ${pad(lane.warmMs, 9)} ${pad(lane.ms, 9)} ${pad(lane.matches, 8)} ${lane.status}`)
      }
      console.log(`${pad(row.scenario, 28)} ${pad(row.kind, 19)} ${pad(row.selectedFiles, 7)} ${pad(row.corpusHash?.slice(0, 8) ?? '-', 10)} ${pad('error', 28)} ${pad('-', 9)} ${pad('-', 9)} ${pad('-', 8)} ERROR ${row.error}`)
      continue
    }
    for (const lane of layerRows(row)) {
      console.log(`${pad(row.scenario, 28)} ${pad(row.kind, 19)} ${pad(row.selectedFiles, 7)} ${pad(row.corpusHash.slice(0, 8), 10)} ${pad(lane.name, 28)} ${pad(lane.warmMs, 9)} ${pad(lane.ms, 9)} ${pad(lane.matches, 8)} ${lane.status}`)
    }
  }
  console.log(`\nsummary: ${summary.okRows} compared, ${summary.diffRows} count differences, ${summary.errorRows} errors, ${summary.skippedRows} skipped`)
  console.log(`wrote: ${summary.outputPath}`)
}

function layerRows(row) {
  const baseline = row.astGrep?.matches
  const lanes = [
    ['ast-grep CLI', row.astGrep],
    ['octocode raw native', row.octocodeRawNative],
    ['octocode localSearchCode tool', row.octocodeLocalSearchCode],
    ['octocode search CLI', row.octocodeCli ?? row.octocode],
  ]
  return lanes
    .filter(([, value]) => value)
    .map(([name, value]) => ({
      name,
      warmMs: value.warmupMedianMs === undefined ? '-' : value.warmupMedianMs.toFixed(1),
      ms: value.durationMs.toFixed(1),
      matches: value.matches,
      status:
        baseline === undefined
          ? 'n/a'
          : value.matches === baseline
            ? 'MATCH'
            : `DIFF ${percentDelta(value.matches, baseline)}`,
    }))
}

const options = parseArgs(process.argv.slice(2))
const octocode = commandSpecForOctocode()
const versions = assertCliAvailable(octocode)
const { manifest, scenarios } = loadScenarios(options)
mkdirSync(options.outputDir, { recursive: true })

const rows = []
for (const scenario of scenarios) {
  const testCase = COMPARISON_CASES[scenario.name]
  if (!testCase) {
    rows.push({ scenario: scenario.name, skipped: 'No comparison case configured.' })
    continue
  }
  if (testCase.skip) {
    rows.push({ scenario: scenario.name, skipped: testCase.skip })
    continue
  }

  let corpus = null
  let selectedFiles = 0
  let selectedBytes = 0
  let astGrep = null
  let rawNative = null
  let localSearchCode = null
  let octocodeCli = null
  let hash = null
  try {
    const repoPath = syncRepo(options, scenario)
    if (!existsSync(repoPath)) {
      rows.push({
        scenario: scenario.name,
        skipped: `Repo not found. Re-run with --sync-repos to clone ${scenario.repo}.`,
      })
      continue
    }
    const files = selectFiles(repoPath, testCase, options)
    if (files.length === 0) {
      rows.push({
        scenario: scenario.name,
        kind: testCase.kind,
        selectedFiles: 0,
        error: `No .${testCase.extensions.join('/.')} files found in ${repoPath}`,
      })
      continue
    }
    corpus = materializeCorpus(options, scenario, files)
    selectedFiles = files.length
    selectedBytes = corpus.bytes
    hash = corpusHash(files)
    astGrep = runAstGrep(corpus.dir, testCase, options)
    rawNative = await runRawNative(corpus.dir, testCase, options)
    localSearchCode = await runLocalSearchTool(corpus.dir, testCase, options)
    octocodeCli = runOctocode(octocode, corpus.dir, testCase, options)
    rows.push({
      scenario: scenario.name,
      codebase: scenario.codebase,
      revision: scenario.revision,
      kind: testCase.kind,
      extensions: testCase.extensions,
      selectedFiles,
      selectedBytes,
      corpusHash: hash,
      corpusDir: options.keepCorpus ? corpus.dir : undefined,
      astGrep,
      octocodeRawNative: rawNative,
      octocodeLocalSearchCode: localSearchCode,
      octocodeCli,
      // Back-compatible name for earlier JSON consumers.
      octocode: octocodeCli,
      sameMatchCount: [
        rawNative.matches,
        localSearchCode.matches,
        octocodeCli.matches,
      ].every(count => count === astGrep.matches),
    })
  } catch (error) {
    rows.push({
      scenario: scenario.name,
      kind: testCase.kind,
      selectedFiles,
      selectedBytes,
      revision: scenario.revision,
      corpusHash: hash,
      astGrep,
      octocodeRawNative: rawNative,
      octocodeLocalSearchCode: localSearchCode,
      octocodeCli,
      octocode: octocodeCli,
      error: error instanceof Error ? error.message : String(error),
    })
  } finally {
    if (corpus && !options.keepCorpus) {
      rmSync(corpus.dir, { recursive: true, force: true })
    }
  }
}

const comparedRows = rows.filter(
  row => row.astGrep && row.octocodeRawNative && row.octocodeLocalSearchCode && row.octocodeCli,
)
const summary = {
  source: manifest.source,
  versions,
  repoDir: options.repoDir,
  options: {
    filesPerScenario: options.filesPerScenario,
    maxFileBytes: options.maxFileBytes,
    repeats: options.repeats,
    warmups: options.warmups,
    scenario: options.scenario,
  },
  rows,
  okRows: comparedRows.length,
  diffRows: comparedRows.filter(row => !row.sameMatchCount).length,
  errorRows: rows.filter(row => row.error).length,
  skippedRows: rows.filter(row => row.skipped).length,
  outputPath: join(options.outputDir, 'latest.json'),
}

writeFileSync(summary.outputPath, JSON.stringify(summary, null, 2) + '\n')

if (options.json) {
  console.log(JSON.stringify(summary, null, 2))
} else {
  printTable(summary)
}

if (summary.errorRows > 0) process.exit(1)
if (options.strict && summary.diffRows > 0) process.exit(1)
