#!/usr/bin/env node

import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT = path.resolve(__dirname, '..', '..', '..');

const PACKAGE_SCRIPT_POLICY = ['build', 'lint', 'test', 'typecheck', 'verify'];
const SKILL_SCRIPT_POLICY = ['build', 'lint', 'test'];
const VERIFY_ORDER = ['@octocodeai/octocode-native', 'octocode-mcp', 'octocode', 'octocode-mcp-vscode'];
// Build-order edges that are not package dependencies: the consumer's build
// reads or rewrites files another workspace's build generates, so they must
// never run concurrently (or out of order).
// - native's build.rs embeds octocode-config's config-contract.json and
//   contract/, which config's build regenerates.
const BUILD_INPUTS = {
  '@octocodeai/octocode-native': ['@octocodeai/config'],
  '@octocodeai/codex-plugin': ['octocode'],
  '@octocodeai/claude-plugin': ['octocode'],
};
const BUILD_OUTPUTS = {
  'packages/octocode-mcp': ['dist/index.js'],
  'packages/octocode': ['out/octocode.js'],
  'packages/octocode-vscode': ['out/extension.js'],
  'packages/octocode-claude-plugin': ['.claude-plugin/plugin.json', '.mcp.json', 'skills/octocode-get-started/SKILL.md'],
  'packages/octocode-codex-plugin': ['plugin.json', 'mcp.json', 'skills/octocode-get-started/SKILL.md'],
};

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function resolveWorkspaceDirs(pattern) {
  // Direct path (e.g. "skills") — single workspace package at that directory.
  if (!pattern.includes('*')) {
    const dirPath = path.join(ROOT, pattern);
    if (fs.existsSync(path.join(dirPath, 'package.json'))) {
      return [dirPath];
    }
    return [];
  }

  if (!pattern.endsWith('/*')) {
    throw new Error(`Unsupported workspace pattern: ${pattern}`);
  }

  const baseDir = path.join(ROOT, pattern.slice(0, -2));
  if (!fs.existsSync(baseDir)) {
    return [];
  }

  return fs
    .readdirSync(baseDir, { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .map(entry => path.join(baseDir, entry.name))
    .filter(dirPath => fs.existsSync(path.join(dirPath, 'package.json')));
}

function discoverWorkspaces() {
  const rootPackageJson = readJson(path.join(ROOT, 'package.json'));
  const workspaceDirs = rootPackageJson.workspaces.flatMap(resolveWorkspaceDirs);
  const seen = new Set();

  return workspaceDirs
    .filter(dirPath => {
      const relativePath = path.relative(ROOT, dirPath);
      if (seen.has(relativePath)) {
        return false;
      }
      seen.add(relativePath);
      return true;
    })
    .map(dirPath => {
      const packageJsonPath = path.join(dirPath, 'package.json');
      const packageJson = readJson(packageJsonPath);
      const relativePath = path.relative(ROOT, dirPath);
      const isNativePlatformPackage = /\/npm\/[^/]+$/.test(relativePath);
      const kind = isNativePlatformPackage
        ? 'native-platform'
        : relativePath.startsWith('packages/')
          ? 'package'
          : 'skill';
      const requiredScripts = isNativePlatformPackage
        ? []
        : kind === 'package'
          ? PACKAGE_SCRIPT_POLICY
          : SKILL_SCRIPT_POLICY;

      return {
        name: packageJson.name,
        location: relativePath,
        packageJson,
        kind,
        requiredScripts,
        expectedOutputs: BUILD_OUTPUTS[relativePath] || [],
      };
    })
    .sort((left, right) => left.location.localeCompare(right.location));
}

function getWorkspaceMap(workspaces) {
  return new Map(workspaces.map(workspace => [workspace.name, workspace]));
}

function collectInternalDependencies(workspace, workspaceMap) {
  const dependencyFields = ['dependencies', 'devDependencies', 'peerDependencies', 'optionalDependencies'];
  const internalDependencies = new Set();

  for (const field of dependencyFields) {
    const dependencies = workspace.packageJson[field] || {};
    for (const dependencyName of Object.keys(dependencies)) {
      if (workspaceMap.has(dependencyName)) {
        internalDependencies.add(dependencyName);
      }
    }
  }

  return internalDependencies;
}

function collectBuildDependencies(workspace, workspaceMap) {
  const dependencies = collectInternalDependencies(workspace, workspaceMap);
  for (const input of BUILD_INPUTS[workspace.name] ?? []) {
    if (workspaceMap.has(input)) dependencies.add(input);
  }
  return dependencies;
}

function topologicallySort(workspaces) {
  const workspaceMap = getWorkspaceMap(workspaces);
  const inDegree = new Map(workspaces.map(workspace => [workspace.name, 0]));
  const dependents = new Map(workspaces.map(workspace => [workspace.name, new Set()]));

  for (const workspace of workspaces) {
    const dependencies = collectBuildDependencies(workspace, workspaceMap);
    for (const dependencyName of dependencies) {
      if (!inDegree.has(dependencyName)) {
        continue;
      }
      inDegree.set(workspace.name, (inDegree.get(workspace.name) || 0) + 1);
      dependents.get(dependencyName).add(workspace.name);
    }
  }

  const queue = workspaces
    .filter(workspace => (inDegree.get(workspace.name) || 0) === 0)
    .sort((left, right) => left.location.localeCompare(right.location));

  const sorted = [];

  while (queue.length > 0) {
    const next = queue.shift();
    sorted.push(next);

    for (const dependentName of dependents.get(next.name) || []) {
      const remaining = (inDegree.get(dependentName) || 0) - 1;
      inDegree.set(dependentName, remaining);
      if (remaining === 0) {
        queue.push(workspaceMap.get(dependentName));
        queue.sort((left, right) => left.location.localeCompare(right.location));
      }
    }
  }

  if (sorted.length !== workspaces.length) {
    throw new Error('Workspace dependency cycle detected while sorting health tasks');
  }

  return sorted;
}

function printScriptMatrix(workspaces) {
  const rows = workspaces.map(workspace => {
    const availableScripts = workspace.packageJson.scripts || {};
    const missingScripts = workspace.requiredScripts.filter(
      scriptName => !availableScripts[scriptName]
    );

    return {
      workspace: workspace.location,
      kind: workspace.kind,
      required: workspace.requiredScripts.join(', '),
      missing: missingScripts.length > 0 ? missingScripts.join(', ') : 'none',
    };
  });

  console.table(rows);
}

function sourceFiles(root) {
  const skippedDirectories = new Set([
    '.git', '.yarn', 'coverage', 'dist', 'node_modules', 'npm', 'out', 'target',
  ]);
  const extensions = new Set(['.cjs', '.cts', '.js', '.jsx', '.mjs', '.mts', '.ts', '.tsx']);
  const files = [];

  function visit(directory) {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      if (entry.isDirectory() && skippedDirectories.has(entry.name)) continue;
      const absolutePath = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(absolutePath);
      else if (extensions.has(path.extname(entry.name))) files.push(absolutePath);
    }
  }

  visit(root);
  return files;
}

function importedPackageNames(source) {
  const names = new Set();
  const pattern = /(?:\bfrom\s*|\bimport\s+|\bimport\s*\(|\brequire\s*\()\s*['"]([^'".][^'"]*)['"]/g;
  for (const match of source.matchAll(pattern)) {
    const specifier = match[1];
    const segments = specifier.split('/');
    names.add(specifier.startsWith('@') ? segments.slice(0, 2).join('/') : segments[0]);
  }
  return names;
}

function checkDeclaredInternalImports(workspaces) {
  const workspaceMap = getWorkspaceMap(workspaces);
  const failures = [];

  for (const workspace of workspaces.filter(candidate => candidate.kind !== 'native-platform')) {
    const declared = collectInternalDependencies(workspace, workspaceMap);
    const workspaceRoot = path.join(ROOT, workspace.location);
    for (const filePath of sourceFiles(workspaceRoot)) {
      const source = fs.readFileSync(filePath, 'utf8');
      for (const importedName of importedPackageNames(source)) {
        if (
          workspaceMap.has(importedName) &&
          importedName !== workspace.name &&
          !declared.has(importedName)
        ) {
          failures.push(
            `${path.relative(ROOT, filePath)} imports undeclared workspace package ${importedName}`
          );
        }
      }
    }
  }

  if (failures.length > 0) {
    console.error('Workspace dependency declaration check failed:');
    for (const failure of [...new Set(failures)].sort()) console.error(`- ${failure}`);
    process.exit(1);
  }
}

function checkRequiredScripts(workspaces) {
  const failures = [];

  for (const workspace of workspaces) {
    const availableScripts = workspace.packageJson.scripts || {};
    for (const requiredScript of workspace.requiredScripts) {
      if (!availableScripts[requiredScript]) {
        failures.push(
          `${workspace.location} is missing required script "${requiredScript}"`
        );
      }
    }
  }

  if (failures.length > 0) {
    console.error('Workspace contract check failed:');
    for (const failure of failures) {
      console.error(`- ${failure}`);
    }
    process.exit(1);
  }

  checkDeclaredInternalImports(workspaces);
}

function checkBuildOutputs(workspaces) {
  const failures = [];

  for (const workspace of workspaces) {
    for (const relativeOutputPath of workspace.expectedOutputs) {
      const absoluteOutputPath = path.join(ROOT, workspace.location, relativeOutputPath);
      if (!fs.existsSync(absoluteOutputPath)) {
        failures.push(
          `${workspace.location} is missing expected build output ${relativeOutputPath}`
        );
      }
    }
  }

  if (failures.length > 0) {
    console.error('Build output verification failed:');
    for (const failure of failures) {
      console.error(`- ${failure}`);
    }
    process.exit(1);
  }
}

function runCommand(command, args, cwd = ROOT) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: 'inherit',
    env: process.env,
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function resolveScript(workspace, scriptName, preferScript) {
  const scripts = workspace.packageJson.scripts || {};
  return preferScript && scripts[preferScript] ? preferScript : scriptName;
}

function runWorkspaceScript(workspaces, scriptName, preferScript) {
  const eligibleWorkspaces = workspaces.filter(workspace =>
    workspace.requiredScripts.includes(scriptName)
  );

  const orderedWorkspaces = topologicallySort(eligibleWorkspaces);

  for (const workspace of orderedWorkspaces) {
    const script = resolveScript(workspace, scriptName, preferScript);
    console.log(`\n==> ${workspace.location}: ${script}`);
    runCommand('yarn', ['workspace', workspace.name, 'run', script]);
  }
}

function runPrefixed(label, command, args) {
  return new Promise(resolve => {
    const child = spawn(command, args, { cwd: ROOT, env: process.env, stdio: ['ignore', 'pipe', 'pipe'] });
    for (const stream of [child.stdout, child.stderr]) {
      let pending = '';
      stream.setEncoding('utf8');
      stream.on('data', chunk => {
        const lines = (pending + chunk).split('\n');
        pending = lines.pop();
        for (const line of lines) process.stdout.write(`[${label}] ${line}\n`);
      });
      stream.on('end', () => pending && process.stdout.write(`[${label}] ${pending}\n`));
    }
    child.on('error', error => resolve({ status: 1, error }));
    child.on('close', status => resolve({ status: status ?? 1 }));
  });
}

/**
 * Runs each workspace as soon as its dependencies (package + BUILD_INPUTS
 * edges) finished. Independent packages — the separate Cargo workspaces
 * (native) each own a target dir, so
 * they never contend on a Cargo lock — overlap. After a failure no new task
 * starts; running ones finish, then the run exits non-zero.
 */
async function runWorkspaceScriptParallel(workspaces, scriptName, preferScript) {
  const eligibleWorkspaces = workspaces.filter(workspace =>
    workspace.requiredScripts.includes(scriptName)
  );
  const workspaceMap = getWorkspaceMap(eligibleWorkspaces);
  const results = new Map();
  let failed = false;
  const started = Date.now();

  for (const workspace of topologicallySort(eligibleWorkspaces)) {
    const dependencies = [...collectBuildDependencies(workspace, workspaceMap)].map(name => results.get(name));
    results.set(
      workspace.name,
      Promise.all(dependencies).then(async () => {
        if (failed) return { workspace, skipped: true };
        const script = resolveScript(workspace, scriptName, preferScript);
        const taskStarted = Date.now();
        console.log(`==> ${workspace.location}: ${script}`);
        const { status, error } = await runPrefixed(workspace.location, 'yarn', ['workspace', workspace.name, 'run', script]);
        const seconds = ((Date.now() - taskStarted) / 1000).toFixed(1);
        if (status !== 0) {
          failed = true;
          console.error(`<== ${workspace.location}: ${script} FAILED (${error?.message ?? `exit ${status}`}) after ${seconds}s`);
        } else {
          console.log(`<== ${workspace.location}: ${script} ok in ${seconds}s`);
        }
        return { workspace, status, seconds };
      })
    );
  }

  const outcomes = await Promise.all(results.values());
  console.table(
    outcomes.map(({ workspace, status, seconds, skipped }) => ({
      workspace: workspace.location,
      result: skipped ? 'skipped' : status === 0 ? 'ok' : 'FAILED',
      seconds: seconds ?? '-',
    }))
  );
  console.log(`${scriptName}: ${((Date.now() - started) / 1000).toFixed(1)}s wall`);
  if (failed) process.exit(1);
}

function runVerify(workspaces) {
  checkRequiredScripts(workspaces);
  runCommand('node', ['--test', 'skills-dev/octocode-dev/scripts/workspace-health.test.mjs']);
  runCommand('node', ['skills-dev/octocode-dev/scripts/dedupe-deps.mjs']);
  runCommand('node', ['skills-dev/octocode-dev/scripts/docs-verify.mjs']);

  const packagesInVerifyOrder = VERIFY_ORDER
    .map(packageName => workspaces.find(workspace => workspace.name === packageName))
    .filter(Boolean);

  for (const workspace of packagesInVerifyOrder) {
    console.log(`\n==> ${workspace.location}: verify`);
    runCommand('yarn', ['workspace', workspace.name, 'run', 'verify']);
  }

  const remaining = workspaces.filter(
    workspace => !VERIFY_ORDER.includes(workspace.name)
  );
  const nonVerifyWorkspaces = remaining.filter(workspace => !workspace.packageJson.scripts?.verify);

  for (const scriptName of SKILL_SCRIPT_POLICY) {
    runWorkspaceScript(nonVerifyWorkspaces, scriptName);
  }

  // A workspace owns its complete gate, including checks beyond build/lint/test.
  for (const workspace of remaining.filter(workspace => workspace.packageJson.scripts?.verify)) {
    console.log(`\n==> ${workspace.location}: verify`);
    runCommand('yarn', ['workspace', workspace.name, 'run', 'verify']);
  }

  checkBuildOutputs(workspaces);
}

async function main() {
  const args = process.argv.slice(2);
  const excludes = [];
  const positional = [];
  let parallel = false;
  let preferScript;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--exclude' && i + 1 < args.length) {
      excludes.push(args[++i]);
    } else if (args[i] === '--parallel') {
      parallel = true;
    } else if (args[i] === '--prefer' && i + 1 < args.length) {
      preferScript = args[++i];
    } else {
      positional.push(args[i]);
    }
  }
  const [mode = 'report', scriptName] = positional;

  let workspaces = discoverWorkspaces();
  if (excludes.length > 0) {
    workspaces = workspaces.filter(ws => !excludes.includes(ws.name));
  }

  switch (mode) {
    case 'report':
      printScriptMatrix(workspaces);
      return;
    case 'check':
      checkRequiredScripts(workspaces);
      console.log('Workspace contract check passed.');
      return;
    case 'check-outputs':
      checkBuildOutputs(workspaces);
      console.log('Build outputs verified.');
      return;
    case 'run':
      if (!scriptName) {
        console.error(
          'Usage: node skills-dev/octocode-dev/scripts/workspace-health.mjs run <script> [--parallel] [--prefer <script>] [--exclude <workspace>]'
        );
        process.exit(1);
      }
      checkRequiredScripts(workspaces);
      if (parallel) await runWorkspaceScriptParallel(workspaces, scriptName, preferScript);
      else runWorkspaceScript(workspaces, scriptName, preferScript);
      return;
    case 'verify':
      runVerify(workspaces);
      return;
    default:
      console.error(`Unknown workspace-health mode: ${mode}`);
      process.exit(1);
  }
}

await main();
