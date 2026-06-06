import * as ts from 'typescript';

import { findParentBlock } from './effects.js';
import { isFunctionLike } from '../ast/helpers.js';
import { getLineAndCharacter } from '../common/utils.js';

const DEEP_MERGE_NAMES = new Set([
  'merge',
  'deepMerge',
  'deepAssign',
  'extend',
  'deepExtend',
  'defaults',
  'defaultsDeep',
  'assign',
  'mixin',
]);


function isKeyFromInternalIteration(
  node: ts.ElementAccessExpression,
  sourceFile: ts.SourceFile
): boolean {
  const keyExpr = node.argumentExpression;
  if (!keyExpr || !ts.isIdentifier(keyExpr)) return false;
  const keyName = keyExpr.getText(sourceFile);

  let current: ts.Node | undefined = node.parent;
  while (current) {
    if (ts.isForOfStatement(current) || ts.isForInStatement(current)) {
      const init = current.initializer;
      if (init) {
        const initText = init.getText(sourceFile);
        if (initText.includes(keyName)) {
          const expr = current.expression.getText(sourceFile);
          if (
            /Object\.(keys|values|entries|getOwnPropertyNames)\(/.test(expr) ||
            /\.keys\(\)|\.values\(\)|\.entries\(\)/.test(expr) ||
            /Array\.from\(/.test(expr)
          ) {
            return true;
          }
        }
      }
    }
    if (isFunctionLike(current)) break;
    current = current.parent;
  }
  return false;
}


function hasProtoKeyGuard(node: ts.Node, sourceFile: ts.SourceFile): boolean {
  const block = findParentBlock(node);
  if (!block) return false;
  const blockText = block.getText(sourceFile);
  return (
    /__proto__|constructor|prototype/.test(blockText) &&
    (blockText.includes('===') ||
      blockText.includes('!==') ||
      blockText.includes('includes(') ||
      blockText.includes('hasOwnProperty'))
  );
}


function isTargetSafeObject(
  node: ts.ElementAccessExpression,
  sourceFile: ts.SourceFile
): boolean {
  const objText = node.expression.getText(sourceFile);
  let current: ts.Node | undefined = node.parent;
  while (current) {
    if (ts.isBlock(current) || ts.isSourceFile(current)) {
      const text = current.getText(sourceFile);
      const createNullPattern = new RegExp(
        `${objText.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*=\\s*Object\\.create\\(null\\)`
      );
      const mapSetPattern = new RegExp(
        `${objText.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*=\\s*new\\s+(Map|Set)\\b`
      );
      if (createNullPattern.test(text) || mapSetPattern.test(text)) return true;
      break;
    }
    current = current.parent;
  }
  return false;
}

export function collectPrototypePollutionSites(
  sourceFile: ts.SourceFile
): Array<{
  kind: string;
  detail: string;
  lineStart: number;
  lineEnd: number;
  guarded: boolean;
}> {
  const sites: Array<{
    kind: string;
    detail: string;
    lineStart: number;
    lineEnd: number;
    guarded: boolean;
  }> = [];

  const visit = (node: ts.Node): void => {
    if (ts.isCallExpression(node)) {
      const text = node.expression.getText(sourceFile);
      if (text === 'Object.assign' && node.arguments.length >= 2) {
        const loc = getLineAndCharacter(sourceFile, node);
        sites.push({
          kind: 'object-assign',
          detail: `Object.assign() merges properties without __proto__ guard`,
          lineStart: loc.lineStart,
          lineEnd: loc.lineEnd,
          guarded: false,
        });
      }
      const calleeName = text.split('.').pop() || '';
      if (DEEP_MERGE_NAMES.has(calleeName) && node.arguments.length >= 1) {
        const loc = getLineAndCharacter(sourceFile, node);
        sites.push({
          kind: 'deep-merge',
          detail: `${calleeName}() deep-merges without prototype guard`,
          lineStart: loc.lineStart,
          lineEnd: loc.lineEnd,
          guarded: false,
        });
      }
    }

    if (
      ts.isElementAccessExpression(node) &&
      node.argumentExpression &&
      !ts.isStringLiteral(node.argumentExpression) &&
      !ts.isNumericLiteral(node.argumentExpression) &&
      node.parent &&
      ts.isBinaryExpression(node.parent) &&
      node.parent.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
      node.parent.left === node
    ) {
      const guarded =
        isKeyFromInternalIteration(node, sourceFile) ||
        hasProtoKeyGuard(node, sourceFile) ||
        isTargetSafeObject(node, sourceFile);
      const loc = getLineAndCharacter(sourceFile, node);
      sites.push({
        kind: 'computed-property-write',
        detail: `Dynamic bracket assignment: ${node.getText(sourceFile).slice(0, 40)}`,
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
        guarded,
      });
    }

    ts.forEachChild(node, visit);
  };

  ts.forEachChild(sourceFile, visit);
  return sites;
}
