// Workspace contract checks: required scripts, declared internal imports, core layering,
// and expected build outputs (including staged native artifact freshness).
import fs from 'node:fs';
import path from 'node:path';
import { stagedNativeArtifacts, staleArtifacts } from '../artifact-freshness.mjs';
import { ROOT, collectInternalDependencies, getWorkspaceMap, readJson } from './workspaces.mjs';

// The core layering: interfaces (mcp, cli) sit on config (bundled at build time) and native
// (loaded at runtime); config and native depend on no workspace package; mcp-cli is a standalone
// library. A workspace listed here may depend on exactly the packages it maps to.
const CORE_LAYERING = {
  'octocode-mcp-cli': [],
  '@octocodeai/config': [],
  '@octocodeai/octocode-native': [],
  'octocode-mcp': ['@octocodeai/config', '@octocodeai/octocode-native'],
  // The CLI bundles native's launcher scripts and installs only the binary
  // of its platform (`optionalDependencies` on the per-platform packages).
  octocode: ['@octocodeai/config'],
};
// Bundled into the interface's output, so it must never be a published runtime dependency.
const BUNDLED_WORKSPACES = ['@octocodeai/config'];

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

function layeringFailures(workspaces) {
  const workspaceMap = getWorkspaceMap(workspaces);
  const failures = [];
  for (const [name, allowed] of Object.entries(CORE_LAYERING)) {
    const workspace = workspaceMap.get(name);
    if (!workspace) continue;
    for (const dependency of collectInternalDependencies(workspace, workspaceMap)) {
      // Native owns its per-platform binary packages.
      if (workspaceMap.get(dependency).kind === 'native-platform') continue;
      if (!allowed.includes(dependency)) {
        failures.push(`${name} must not depend on ${dependency}; allowed: ${allowed.join(', ') || 'no workspace package'}`);
      }
    }
    for (const bundled of BUNDLED_WORKSPACES) {
      const fields = ['dependencies', 'peerDependencies', 'optionalDependencies'];
      if (fields.some(field => workspace.packageJson[field]?.[bundled])) {
        failures.push(`${name} bundles ${bundled}; declare it in devDependencies, not as a published runtime dependency`);
      }
    }
  }
  return failures;
}

export function checkLayering(workspaces) {
  const failures = layeringFailures(workspaces);
  if (failures.length > 0) {
    console.error('Workspace layering check failed:');
    for (const failure of failures) console.error(`- ${failure}`);
    process.exit(1);
  }
}

export function checkRequiredScripts(workspaces) {
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

/** Staged native artifacts built for another version or contract. */
function staleNativeFailures(options) {
  const artifacts = stagedNativeArtifacts(path.join(ROOT, 'packages/octocode-native/npm'), options);
  if (artifacts.length === 0) return options.all ? [] : ['Host native binary is missing; run build:dev.'];
  const { contractFingerprint } = readJson(path.join(ROOT, 'packages/octocode-config/contract/provenance.json'));
  const { version } = readJson(path.join(ROOT, 'packages/octocode-native/package.json'));
  return staleArtifacts(artifacts, contractFingerprint, version).map(
    file => `${path.relative(ROOT, file)} is stale for ${version} / contract ${contractFingerprint.slice(0, 12)}; rebuild with ${options.all ? 'build:publish before publishing' : 'build:dev'}`
  );
}

export function checkBuildOutputs(workspaces, options = {}) {
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
  if (workspaces.some(workspace => workspace.name === '@octocodeai/octocode-native')) {
    failures.push(...staleNativeFailures(options));
  }

  if (failures.length > 0) {
    console.error('Build output verification failed:');
    for (const failure of failures) {
      console.error(`- ${failure}`);
    }
    process.exit(1);
  }
}
