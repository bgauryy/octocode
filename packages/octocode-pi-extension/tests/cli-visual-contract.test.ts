import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
import { test } from 'vitest';

const packageRoot = path.resolve(import.meta.dirname, '..');
const sourceRoot = path.join(packageRoot, 'src');

const expectedRendererFallbackDebt: Record<string, number> = {};

const expectedLowLevelFallbacks: Record<string, number> = {
  'src/tui/cli-design.ts': 2,
  'src/tui/palette.ts': 1,
};

function listTypeScriptFiles(dir: string, files: string[] = []): string[] {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'dist' || entry.name === 'node_modules') continue;
    const next = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      listTypeScriptFiles(next, files);
    } else if (entry.isFile() && entry.name.endsWith('.ts')) {
      files.push(next);
    }
  }
  return files;
}

function countDirectThemeFallbacks(filePath: string): number {
  const sourceText = fs.readFileSync(filePath, 'utf8');
  const sourceFile = ts.createSourceFile(filePath, sourceText, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  let count = 0;

  const visit = (node: ts.Node): void => {
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.QuestionQuestionToken) {
      const leftText = node.left.getText(sourceFile);
      if (/(?:\b\w+\.)?theme\??\.fg\(/.test(leftText)) {
        count += 1;
      }
    }
    ts.forEachChild(node, visit);
  };

  visit(sourceFile);
  return count;
}

function collectDirectFallbackDebt(): Record<string, number> {
  const debt: Record<string, number> = {};
  for (const filePath of listTypeScriptFiles(sourceRoot)) {
    const count = countDirectThemeFallbacks(filePath);
    if (count > 0) {
      debt[path.relative(packageRoot, filePath)] = count;
    }
  }
  return Object.fromEntries(Object.entries(debt).sort(([a], [b]) => a.localeCompare(b)));
}

test('CLI visual contract pins direct renderer theme fallback debt', () => {
  const debt = collectDirectFallbackDebt();
  const rendererDebt = Object.fromEntries(
    Object.entries(debt).filter(([file]) => !file.startsWith('src/tui/')),
  );

  assert.deepEqual(rendererDebt, expectedRendererFallbackDebt);
});

test('low-level visual helpers are the only allowed fallback owners outside renderer debt', () => {
  const debt = collectDirectFallbackDebt();
  const lowLevelFallbacks = Object.fromEntries(
    Object.entries(debt).filter(([file]) => file.startsWith('src/tui/')),
  );

  assert.deepEqual(lowLevelFallbacks, expectedLowLevelFallbacks);
});
