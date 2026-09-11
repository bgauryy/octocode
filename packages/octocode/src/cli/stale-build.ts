import { existsSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SOURCE_EXTENSIONS = new Set(['.ts', '.mts', '.cts']);
let warningShown = false;

function findPackageRoot(currentFile: string): string | undefined {
  let candidate = path.dirname(currentFile);
  for (let depth = 0; depth < 8; depth += 1) {
    if (existsSync(path.join(candidate, 'src', 'cli', 'index.ts'))) {
      return candidate;
    }
    const parent = path.dirname(candidate);
    if (parent === candidate) return undefined;
    candidate = parent;
  }
  return undefined;
}

function newestSourceFile(directory: string): {
  path: string;
  mtimeMs: number;
} | null {
  let newest: { path: string; mtimeMs: number } | null = null;
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      const nested = newestSourceFile(entryPath);
      if (nested && (!newest || nested.mtimeMs > newest.mtimeMs)) {
        newest = nested;
      }
      continue;
    }
    if (!entry.isFile() || !SOURCE_EXTENSIONS.has(path.extname(entry.name))) {
      continue;
    }
    const mtimeMs = statSync(entryPath).mtimeMs;
    if (!newest || mtimeMs > newest.mtimeMs)
      newest = { path: entryPath, mtimeMs };
  }
  return newest;
}

export function findStaleSourceInput(currentFile: string): string | undefined {
  if (!currentFile.includes(`${path.sep}out${path.sep}`)) return undefined;
  const packageRoot = findPackageRoot(currentFile);
  if (!packageRoot || !existsSync(currentFile)) return undefined;
  const newest = newestSourceFile(path.join(packageRoot, 'src'));
  if (!newest || newest.mtimeMs <= statSync(currentFile).mtimeMs + 1000) {
    return undefined;
  }
  return path.relative(packageRoot, newest.path);
}

export function maybeWarnAboutStaleBuild(
  options: {
    currentFile?: string;
    env?: NodeJS.ProcessEnv;
    warn?: (message: string) => void;
  } = {}
): void {
  const env = options.env ?? process.env;
  if (warningShown || env.OCTOCODE_NO_STALE_BUILD_WARNING) return;
  warningShown = true;
  const staleSource = findStaleSourceInput(
    options.currentFile ?? fileURLToPath(import.meta.url)
  );
  if (!staleSource) return;
  (options.warn ?? console.error)(
    `  Warning: built CLI output looks older than ${staleSource}. Run \`yarn build\` before dogfooding source edits.`
  );
}

export function resetStaleBuildWarningForTests(): void {
  warningShown = false;
}
