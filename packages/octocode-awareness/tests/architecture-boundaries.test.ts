import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import ts from 'typescript';

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)));

function typescriptFiles(root: string): string[] {
  return readdirSync(root, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith('.ts'))
    .map((entry) => join(entry.parentPath, entry.name));
}

describe('Awareness architecture boundaries', () => {
  it('keeps forwarding exports at the published entrypoint', () => {
    const sourceRoot = join(packageRoot, 'src');
    const violations = typescriptFiles(sourceRoot).flatMap((file) => {
      if (file === join(sourceRoot, 'index.ts')) return [];
      const source = ts.createSourceFile(file, readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
      return source.statements.filter(ts.isExportDeclaration).map(() => file.slice(sourceRoot.length + 1));
    });
    expect(violations).toEqual([]);
  });

  it('imports notification SQL from its owning module, not the aggregate SQL barrel', () => {
    const consumers = ['notifications-core.ts', 'notifications-inbox.ts', 'notifications-signals.ts'];
    const violations = consumers.filter((file) => readFileSync(join(packageRoot, 'src', file), 'utf8').includes('./sql/index.js'));
    expect(violations).toEqual([]);
  });

  it('keeps every internal SQL module and exported statement reachable', () => {
    const runtimeFiles = [...typescriptFiles(join(packageRoot, 'src')), ...typescriptFiles(join(packageRoot, 'bin'))];
    const proofFiles = [...runtimeFiles, ...typescriptFiles(join(packageRoot, 'tests'))];
    const sqlRoot = join(packageRoot, 'src', 'sql');
    const sqlFiles = typescriptFiles(sqlRoot);

    const unreachableModules = sqlFiles.flatMap((file) => {
      const moduleName = file.slice(sqlRoot.length + 1, -'.ts'.length);
      const importSuffix = `/sql/${moduleName}.js`;
      const reachable = runtimeFiles.some((candidate) => candidate !== file && readFileSync(candidate, 'utf8').includes(importSuffix));
      return reachable ? [] : [moduleName];
    });

    const unreferencedStatements = sqlFiles.flatMap((file) => {
      const contents = readFileSync(file, 'utf8');
      const statements = [...contents.matchAll(/^export const ([A-Z][A-Z0-9_]*)/gmu)].flatMap((match) => (match[1] === undefined ? [] : [match[1]]));
      return statements.filter((statement) => {
        const reference = new RegExp(`\\b${statement}\\b`, 'u');
        return !proofFiles.some((candidate) => candidate !== file && reference.test(readFileSync(candidate, 'utf8')));
      });
    });

    expect(unreachableModules).toEqual([]);
    expect(unreferencedStatements).toEqual([]);
  });
});
