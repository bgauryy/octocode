// One source for stack detection: detect-workspace.mjs reports languages from it, verify-project.mjs picks gates from it.
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

export const readText = file => { try { return readFileSync(file, 'utf8'); } catch { return ''; } };
export const readJson = file => { try { return JSON.parse(readFileSync(file, 'utf8')); } catch { return null; } };
export const entries = dir => { try { return readdirSync(dir); } catch { return []; } };

/** Stack name → whether a folder's entry names mark that stack. */
export const STACK_MARKERS = {
  javascript: names => names.includes('package.json'),
  deno: names => names.includes('deno.json') || names.includes('deno.jsonc'),
  rust: names => names.includes('Cargo.toml'),
  python: names => names.some(name => ['pyproject.toml', 'requirements.txt', 'setup.py', 'uv.lock'].includes(name)),
  go: names => names.includes('go.mod') || names.includes('go.work'),
  dart: names => names.includes('pubspec.yaml'),
  dotnet: names => names.some(name => /\.(slnx?|csproj|fsproj|vbproj)$/.test(name)),
  jvm: names => names.some(name => /^((build|settings)\.gradle(\.kts)?|pom\.xml)$/.test(name)),
  swift: names => names.some(name => name === 'Package.swift' || name === 'Project.swift' || /\.(xcodeproj|xcworkspace)$/.test(name)),
  ruby: names => names.includes('Gemfile'),
  php: names => names.includes('composer.json'),
  elixir: names => names.includes('mix.exs'),
};

/** Stacks marked in one folder, in STACK_MARKERS order. */
export function stacksIn(dir) {
  const names = entries(dir);
  return Object.entries(STACK_MARKERS).filter(([, marks]) => marks(names)).map(([stack]) => stack);
}

/** Files under dir whose names match pattern, skipping dependency and build folders, at most depth levels down. */
export function findFiles(dir, pattern, depth = 3) {
  const skip = new Set(['node_modules', '.git', 'bin', 'obj', 'build', 'dist', 'target', '.build', 'vendor', 'deps', '_build', '.dart_tool', 'Pods']);
  const found = [];
  const walk = (current, level) => {
    let list;
    try { list = readdirSync(current, { withFileTypes: true }); } catch { return; }
    for (const entry of list) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) { if (level < depth && !skip.has(entry.name)) walk(path, level + 1); }
      else if (pattern.test(entry.name)) found.push(path);
    }
  };
  walk(dir, 0);
  return found;
}
