import path from 'node:path';

import {
  buildTreeSitterTree,
  hashString,
  increment,
  makeTreeSitterFingerprint,
} from '../common/utils.js';
import {
  isPythonFile,
  PY_TREE_SITTER_CONTROL_TYPES,
  PY_TREE_SITTER_FUNCTION_TYPES,
  TS_TREE_SITTER_CONTROL_TYPES,
  TS_TREE_SITTER_FUNCTION_TYPES,
} from '../types/index.js';

import type {
  AnalysisOptions,
  FlowEntry,
  FlowMaps,
  FunctionEntry,
  Location,
  NodeBudget,
  SyntaxNode,
  TreeSitterFileEntry,
  TreeSitterMetrics,
  TreeSitterRuntime,
} from '../types/index.js';
import type Parser from 'tree-sitter';

let treeSitterRuntime: TreeSitterRuntime | null = null;

export function getTreeSitterRuntime(): TreeSitterRuntime | null {
  return treeSitterRuntime;
}

function hasLogicalOperator(node: SyntaxNode): boolean {
  for (const child of node.children) {
    if (!child.isNamed && (child.type === '&&' || child.type === '||'))
      return true;
  }
  return false;
}

function collectTreeSitterMetrics(
  node: SyntaxNode,
  _sourceText: string
): TreeSitterMetrics {
  const metrics: TreeSitterMetrics = {
    complexity: 1,
    maxBranchDepth: 0,
    maxLoopDepth: 0,
    returns: 0,
    awaits: 0,
    calls: 0,
    loops: 0,
    statements: 0,
  };

  const visit = (
    n: SyntaxNode,
    branchDepth: number,
    loopDepth: number
  ): void => {
    metrics.statements += 1;

    if (
      [
        'if_statement',
        'while_statement',
        'do_statement',
        'for_statement',
        'for_in_statement',
        'for_of_statement',
        'for_await_statement',
        'switch_statement',
        'catch_clause',
      ].includes(n.type)
    ) {
      metrics.complexity += 1;
      branchDepth += 1;
      metrics.maxBranchDepth = Math.max(metrics.maxBranchDepth, branchDepth);
    }

    if (n.type === 'conditional_expression') {
      metrics.complexity += 1;
    }

    if (n.type === 'binary_expression' && hasLogicalOperator(n)) {
      metrics.complexity += 1;
    }

    if (n.type === 'return_statement' || n.type === 'throw_statement') {
      metrics.returns += 1;
    }

    if (n.type === 'await_expression') {
      metrics.awaits += 1;
    }

    if (n.type === 'call_expression') {
      metrics.calls += 1;
    }

    if (
      [
        'for_statement',
        'for_in_statement',
        'for_of_statement',
        'for_await_statement',
        'while_statement',
        'do_statement',
      ].includes(n.type)
    ) {
      const nextLoopDepth = loopDepth + 1;
      metrics.loops += 1;
      metrics.maxLoopDepth = Math.max(metrics.maxLoopDepth, nextLoopDepth);
      for (const child of n.children) {
        visit(child, branchDepth, nextLoopDepth);
      }
      return;
    }

    for (const child of n.children) {
      visit(child, branchDepth, loopDepth);
    }
  };

  visit(node, 0, 0);
  return metrics;
}

const COGNITIVE_NESTING_TYPES = new Set([
  'if_statement',
  'for_statement',
  'for_in_statement',
  'for_of_statement',
  'for_await_statement',
  'while_statement',
  'do_statement',
  'catch_clause',
  'conditional_expression',
  'switch_statement',
]);

const COGNITIVE_LOGICAL_TYPES = new Set(['&&', '||', '??']);

function computeTreeSitterCognitiveComplexity(node: SyntaxNode): number {
  let total = 0;

  const visit = (current: SyntaxNode, nesting: number): void => {
    let increment = 0;
    let nestable = false;

    if (COGNITIVE_NESTING_TYPES.has(current.type)) {
      increment = 1;
      nestable = true;
    }

    if (current.type === 'binary_expression') {
      for (const child of current.children) {
        if (!child.isNamed && COGNITIVE_LOGICAL_TYPES.has(child.type)) {
          increment = 1;
          break;
        }
      }
    }

    if (
      current.type === 'if_statement' &&
      current.parent?.type === 'else_clause'
    ) {
      increment = 1;
      nestable = false;
    }

    if (nestable) {
      total += increment + nesting;
      for (const child of current.children) {
        visit(child, nesting + 1);
      }
      return;
    }

    total += increment;
    for (const child of current.children) {
      visit(child, nesting);
    }
  };

  visit(node, 0);
  return total;
}

function inferTreeSitterFunctionName(node: SyntaxNode, _text: string): string {
  const identifier = node.namedChildren.find(child =>
    ['identifier', 'property_identifier', 'type_identifier'].includes(
      child.type
    )
  );
  if (identifier) return identifier.text;

  let parent = node.parent;
  while (parent) {
    if (parent.type === 'variable_declarator') {
      const id = parent.namedChildren.find(child =>
        [
          'identifier',
          'property_identifier',
          'array_pattern',
          'object_pattern',
          'shorthand_property_identifier_pattern',
        ].includes(child.type)
      );
      if (id && id.type === 'identifier') {
        return id.text;
      }
      break;
    }

    if (parent.type === 'pair') {
      const key = parent.namedChildren.find(child =>
        [
          'identifier',
          'string',
          'shorthand_property_identifier_pattern',
          'property_identifier',
        ].includes(child.type)
      );
      if (key) return key.text;
      break;
    }

    if (
      [
        'assignment_expression',
        'method_definition',
        'property_signature',
        'public_field_definition',
      ].includes(parent.type)
    ) {
      const key = parent.namedChildren.find(child =>
        [
          'identifier',
          'property_identifier',
          'string',
          'private_property_identifier',
        ].includes(child.type)
      );
      if (key) return key.text;
    }

    if (parent.type === 'statement_block' || parent.type === 'program') break;
    parent = parent.parent;
  }

  return '<anonymous>';
}

function countTreeSitterStatements(node: SyntaxNode): number {
  const body = node.namedChildren.find(
    child => child.type === 'statement_block'
  );
  if (!body) return 1;
  return body.namedChildren.length;
}

function countControlFlowBodyStatements(node: SyntaxNode): number {
  const body = node.namedChildren.find(
    child => child.type === 'statement_block' || child.type === 'switch_body'
  );
  if (body) return body.namedChildren.length;
  return node.namedChildren.length;
}

function makeLocationFromTree(
  node: SyntaxNode,
  repoRoot: string,
  filePath: string
): Location {
  return {
    file: path.relative(repoRoot, filePath),
    lineStart: node.startPosition.row + 1,
    lineEnd: node.endPosition.row + 1,
    columnStart: node.startPosition.column + 1,
    columnEnd: node.endPosition.column + 1,
  };
}

export function analyzeTreeSitterFile(
  filePath: string,
  sourceText: string,
  options: AnalysisOptions,
  packageName: string,
  maps: FlowMaps | null
): TreeSitterFileEntry | null {
  if (!treeSitterRuntime?.available) return null;

  const ext = path.extname(filePath);
  let parser: Parser | null;
  if (isPythonFile(ext)) {
    parser = treeSitterRuntime.parserPy;
  } else if (ext === '.tsx' || ext === '.jsx') {
    parser = treeSitterRuntime.parserTsx;
  } else {
    parser = treeSitterRuntime.parserTs;
  }
  if (!parser) return null;

  const tree = parser.parse(sourceText);
  const fileRelative = path.relative(options.root, filePath);
  const fileEntry: TreeSitterFileEntry = {
    parseEngine: 'tree-sitter',
    nodeCount: 0,
    functions: [],
    flows: [],
  };

  if (options.emitTree) {
    const nodeBudget: NodeBudget = { size: 8000 };
    const snapshot = buildTreeSitterTree(
      tree.rootNode,
      sourceText,
      options.treeDepth,
      nodeBudget
    );
    if (snapshot) {
      fileEntry.tree = snapshot;
    }
  }

  const isPy = isPythonFile(ext);
  const functionTypes = isPy
    ? PY_TREE_SITTER_FUNCTION_TYPES
    : TS_TREE_SITTER_FUNCTION_TYPES;
  const controlTypes = isPy
    ? PY_TREE_SITTER_CONTROL_TYPES
    : TS_TREE_SITTER_CONTROL_TYPES;

  const visit = (node: SyntaxNode): void => {
    fileEntry.nodeCount += 1;

    if (functionTypes.has(node.type)) {
      const loc = makeLocationFromTree(node, options.root, filePath);
      const metrics = collectTreeSitterMetrics(node, sourceText);
      const statementCount = countTreeSitterStatements(node);
      const name = inferTreeSitterFunctionName(node, sourceText);
      const params = node.childForFieldName('parameters');
      const paramCount = params ? params.namedChildren.length : 0;

      const entry: FunctionEntry = {
        kind: node.type,
        name,
        nameHint: name,
        file: fileRelative,
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
        columnStart: loc.columnStart,
        columnEnd: loc.columnEnd,
        statementCount,
        lengthLines: loc.lineEnd - loc.lineStart + 1,
        params: paramCount,
        complexity: metrics.complexity,
        maxBranchDepth: metrics.maxBranchDepth,
        maxLoopDepth: metrics.maxLoopDepth,
        returns: metrics.returns,
        awaits: metrics.awaits,
        calls: metrics.calls,
        loops: metrics.loops,
        cognitiveComplexity: computeTreeSitterCognitiveComplexity(node),
        source: 'tree-sitter',
      };

      fileEntry.functions.push(entry);

      if (maps && statementCount >= options.thresholds.minFunctionStatements) {
        const body = node.namedChildren.find(
          child => child.type === 'statement_block'
        );
        const bodyHash = body
          ? makeTreeSitterFingerprint(body)
          : hashString(fileRelative);
        increment(maps.flowMap, `${bodyHash}|${node.type}`, {
          ...entry,
          hash: bodyHash,
          metrics,
        });
      }
    }

    if (controlTypes.has(node.type)) {
      const loc = makeLocationFromTree(node, options.root, filePath);
      const statementCount = countControlFlowBodyStatements(node);
      const flowEntry: FlowEntry = {
        kind: node.type,
        file: fileRelative,
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
        columnStart: loc.columnStart,
        columnEnd: loc.columnEnd,
        statementCount,
      };
      fileEntry.flows.push(flowEntry);

      if (maps && statementCount >= options.thresholds.minFlowStatements) {
        const flowHash = makeTreeSitterFingerprint(node);
        increment(maps.controlMap, `${flowHash}|${node.type}`, {
          ...flowEntry,
          hash: flowHash,
        });
      }
    }

    for (const child of node.children) {
      visit(child);
    }
  };

  visit(tree.rootNode);
  return fileEntry;
}

export async function resolveTreeSitter(): Promise<TreeSitterRuntime> {
  if (treeSitterRuntime !== null) return treeSitterRuntime;

  try {
    const parserMod = await import('tree-sitter');
    const typescriptMod: Record<string, unknown> =
      await import('tree-sitter-typescript');

    const ParserClass = parserMod.default || parserMod;

    const tsLang =
      (typescriptMod as Record<string, unknown>).typescript ||
      (
        (typescriptMod as Record<string, unknown>).default as Record<
          string,
          unknown
        >
      )?.typescript;
    const tsxLang =
      (typescriptMod as Record<string, unknown>).tsx ||
      (
        (typescriptMod as Record<string, unknown>).default as Record<
          string,
          unknown
        >
      )?.tsx;

    if (!ParserClass || !tsLang) {
      throw new Error(
        'Tree-sitter or tree-sitter-typescript did not expose expected exports'
      );
    }

    const parserTs = new (ParserClass as new () => Parser)();
    parserTs.setLanguage(tsLang as Parameters<Parser['setLanguage']>[0]);

    const parserTsx = new (ParserClass as new () => Parser)();
    parserTsx.setLanguage(
      (tsxLang || tsLang) as Parameters<Parser['setLanguage']>[0]
    );

    let parserPy: Parser | null = null;
    try {
      const pythonMod: Record<string, unknown> =
        // @ts-expect-error tree-sitter-python has no type declarations
        await import('tree-sitter-python');
      const pyLang =
        pythonMod.default || pythonMod;
      if (pyLang) {
        parserPy = new (ParserClass as new () => Parser)();
        parserPy.setLanguage(pyLang as Parameters<Parser['setLanguage']>[0]);
      }
    } catch {
    void 0;
  }

    treeSitterRuntime = {
      available: true,
      parserTs,
      parserTsx,
      parserPy,
    };
    return treeSitterRuntime;
  } catch (error: unknown) {
    treeSitterRuntime = {
      available: false,
      parserTs: null,
      parserTsx: null,
      parserPy: null,
      error: String((error as Error)?.message || error),
    };
    return treeSitterRuntime;
  }
}
