#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT = path.resolve(__dirname, '..');

const PACKAGE_SCRIPT_POLICY = ['build', 'lint', 'test', 'typecheck', 'verify'];
const SKILL_SCRIPT_POLICY = ['build', 'lint', 'test'];
const VERIFY_ORDER = ['@octocodeai/octocode-tools-core', 'octocode-mcp', 'octocode', 'octocode-mcp-vscode'];
const BUILD_OUTPUTS = {
  'packages/octocode-tools-core': ['dist/index.js'],
  'packages/octocode-mcp': ['dist/index.js'],
  'packages/octocode': ['out/octocode.js'],
  'packages/octocode-vscode': ['out/extension.js'],
  'packages/octocode-skills': ['out/cli.js', 'out/index.js'],
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
  const dependencyFields = ['dependencies', 'devDependencies', 'peerDependencies'];
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

function topologicallySort(workspaces) {
  const workspaceMap = getWorkspaceMap(workspaces);
  const inDegree = new Map(workspaces.map(workspace => [workspace.name, 0]));
  const dependents = new Map(workspaces.map(workspace => [workspace.name, new Set()]));

  for (const workspace of workspaces) {
    const dependencies = collectInternalDependencies(workspace, workspaceMap);
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

function runWorkspaceScript(workspaces, scriptName) {
  const eligibleWorkspaces = workspaces.filter(workspace =>
    workspace.requiredScripts.includes(scriptName)
  );

  const orderedWorkspaces = topologicallySort(eligibleWorkspaces);

  for (const workspace of orderedWorkspaces) {
    console.log(`\n==> ${workspace.location}: ${scriptName}`);
    runCommand('yarn', ['workspace', workspace.name, 'run', scriptName]);
  }
}

function runVerify(workspaces) {
  checkRequiredScripts(workspaces);
  runCommand('node', ['scripts/docs-verify.mjs']);

  const packagesInVerifyOrder = VERIFY_ORDER
    .map(packageName => workspaces.find(workspace => workspace.name === packageName))
    .filter(Boolean);

  for (const workspace of packagesInVerifyOrder) {
    console.log(`\n==> ${workspace.location}: verify`);
    runCommand('yarn', ['workspace', workspace.name, 'run', 'verify']);
  }

  const nonVerifyWorkspaces = workspaces.filter(
    workspace => !VERIFY_ORDER.includes(workspace.name)
  );

  for (const scriptName of SKILL_SCRIPT_POLICY) {
    runWorkspaceScript(nonVerifyWorkspaces, scriptName);
  }

  checkBuildOutputs(workspaces);
}

function main() {
  const args = process.argv.slice(2);
  const excludes = [];
  const positional = [];
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--exclude' && i + 1 < args.length) {
      excludes.push(args[++i]);
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
        console.error('Usage: node scripts/workspace-health.mjs run <script> [--exclude <workspace>]');
        process.exit(1);
      }
      checkRequiredScripts(workspaces);
      runWorkspaceScript(workspaces, scriptName);
      return;
    case 'verify':
      runVerify(workspaces);
      return;
    default:
      console.error(`Unknown workspace-health mode: ${mode}`);
      process.exit(1);
  }
}

main();
