// Workspace discovery and build ordering: reads the root workspaces, classifies each
// package, and topologically sorts them by internal dependencies plus BUILD_INPUTS edges.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..', '..', '..');

// The skill collections, owned by skills/common/build.mjs; none in a checkout without it.
const skillsBuild = path.join(ROOT, 'skills', 'common', 'build.mjs');
export const SKILL_COLLECTIONS = fs.existsSync(skillsBuild) ? (await import(pathToFileURL(skillsBuild).href)).COLLECTIONS : [];

const PACKAGE_SCRIPT_POLICY = ['build', 'lint', 'test', 'typecheck', 'verify'];
// Python and scripts run from source: nothing to build or typecheck, so no stub scripts.
const SOURCE_ONLY_PACKAGES = new Set(['packages/octocode-benchmark']);
const SOURCE_ONLY_SCRIPT_POLICY = ['lint', 'test', 'verify'];
export const SKILL_SCRIPT_POLICY = ['build', 'lint', 'test'];
// Build-order edges that are not package dependencies: the consumer's build
// reads or rewrites files another workspace's build generates, so they must
// never run concurrently (or out of order).
// - native's build.rs embeds octocode-config's config-contract.json and
//   contract/, which config's build regenerates.
const BUILD_INPUTS = {
  octocode: ['@octocodeai/skills-common'],
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

export function readJson(filePath) {
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

export function discoverWorkspaces() {
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
          ? SOURCE_ONLY_PACKAGES.has(relativePath) ? SOURCE_ONLY_SCRIPT_POLICY : PACKAGE_SCRIPT_POLICY
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

export function getWorkspaceMap(workspaces) {
  return new Map(workspaces.map(workspace => [workspace.name, workspace]));
}

export function collectInternalDependencies(workspace, workspaceMap) {
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

export function collectBuildDependencies(workspace, workspaceMap) {
  const dependencies = collectInternalDependencies(workspace, workspaceMap);
  for (const input of BUILD_INPUTS[workspace.name] ?? []) {
    if (workspaceMap.has(input)) dependencies.add(input);
  }
  return dependencies;
}

export function topologicallySort(workspaces) {
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
