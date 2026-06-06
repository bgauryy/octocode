import * as ts from 'typescript';

import { getLineAndCharacter } from '../common/utils.js';

import type { FileEntry, MessageChainEntry } from '../types/index.js';


const MIN_CHAIN_DEPTH = 4;


function measureChain(node: ts.Node, sourceFile: ts.SourceFile): { text: string; depth: number } | null {
  let depth = 0;
  let current: ts.Node = node;

  while (
    ts.isPropertyAccessExpression(current) ||
    ts.isElementAccessExpression(current)
  ) {
    depth++;
    current = (current as ts.PropertyAccessExpression | ts.ElementAccessExpression).expression;
  }

  if (depth < MIN_CHAIN_DEPTH) return null;

  const parent = node.parent;
  if (
    ts.isPropertyAccessExpression(parent) ||
    ts.isElementAccessExpression(parent)
  ) {
    return null;
  }

  return { text: node.getText(sourceFile), depth };
}

export function collectMessageChains(
  sourceFile: ts.SourceFile,
  _fileRelative: string,
  fileEntry: FileEntry
): void {
  const chains: MessageChainEntry[] = [];

  const visit = (node: ts.Node): void => {
    if (
      ts.isPropertyAccessExpression(node) ||
      ts.isElementAccessExpression(node)
    ) {
      const result = measureChain(node, sourceFile);
      if (result) {
        const loc = getLineAndCharacter(sourceFile, node);
        chains.push({
          chain: result.text.slice(0, 80),
          depth: result.depth,
          lineStart: loc.lineStart,
          lineEnd: loc.lineEnd,
        });
      }
    }
    ts.forEachChild(node, visit);
  };

  ts.forEachChild(sourceFile, visit);

  if (chains.length > 0) {
    fileEntry.messageChains = chains;
  }
}
