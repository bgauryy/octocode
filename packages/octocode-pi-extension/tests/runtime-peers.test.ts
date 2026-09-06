import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
import { test } from 'vitest';

const packageRoot = path.resolve(import.meta.dirname, '..');

test('startup imports cannot depend on an optional peer', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'));
  const optional = new Set(Object.entries(manifest.peerDependenciesMeta ?? {})
    .filter(([, meta]) => (meta as { optional?: boolean }).optional)
    .map(([name]) => name));
  const checked = new Set<string>();
  function visit(file: string): void {
    if (checked.has(file)) return;
    checked.add(file);
    const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
    for (const statement of source.statements) {
      if (!ts.isImportDeclaration(statement) && !ts.isExportDeclaration(statement)) continue;
      const specifier = statement.moduleSpecifier;
      if (!specifier || !ts.isStringLiteral(specifier)) continue;
      const name = specifier.text;
      if (name.startsWith('.')) {
        visit(path.resolve(path.dirname(file), name));
      } else {
        const pkg = name.startsWith('@') ? name.split('/').slice(0, 2).join('/') : name.split('/')[0];
        assert.ok(!optional.has(pkg), `${file} eagerly imports optional peer ${pkg}`);
      }
    }
  }
  visit(path.join(packageRoot, 'dist/index.js'));
  assert.ok(checked.size > 1, 'check the emitted startup import graph');
});
