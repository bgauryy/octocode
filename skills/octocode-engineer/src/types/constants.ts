import path from 'node:path';

import * as ts from 'typescript';

import type { AnalysisOptions, Thresholds } from './core.js';
import type { DependencyProfile } from './dependency.js';

export const DEFAULT_THRESHOLDS: Thresholds = {
  couplingThreshold: 15,
  fanInThreshold: 20,
  fanOutThreshold: 15,
  godModuleStatements: 500,
  godModuleExports: 20,
  barrelSymbolThreshold: 30,
  sdpMinDelta: 0.15,
  sdpMaxSourceInstability: 0.6,
  layerOrder: [],

  minFunctionStatements: 6,
  minFlowStatements: 6,
  criticalComplexityThreshold: 30,
  godFunctionStatements: 100,
  godFunctionMiThreshold: 10,
  cognitiveComplexityThreshold: 15,
  parameterThreshold: 5,
  halsteadEffortThreshold: 500_000,
  maintainabilityIndexThreshold: 20,
  anyThreshold: 5,
  flowDupThreshold: 3,
  similarityThreshold: 0.85,
  deepNestingThreshold: 5,
  multipleReturnThreshold: 6,
  magicStringMinOccurrences: 3,
  booleanParamThreshold: 3,

  overrideChainThreshold: 3,
  shotgunThreshold: 8,

  secretEntropyThreshold: 4.5,
  secretMinLength: 20,

  mockThreshold: 10,
};

export const DEFAULT_OPTS: AnalysisOptions = {
  root: process.cwd(),
  out: null,
  json: false,
  packageRoot: path.join(process.cwd(), 'packages'),
  parser: 'auto',
  includeTests: false,
  emitTree: true,
  treeDepth: 4,
  noCache: false,
  clearCache: false,
  semantic: false,
  graph: false,
  graphAdvanced: false,
  flow: false,
  scope: null,
  scopeSymbols: null,
  features: null,
  ignoreDirs: new Set([
    '.git',
    '.next',
    '.yarn',
    '.cache',
    '.octocode',
    'node_modules',
    'dist',
    'coverage',
    'out',
  ]),
  findingsLimit: Infinity,
  noDiversify: false,
  maxRecsPerCategory: 2,
  deepLinkTopN: 12,
  thresholds: { ...DEFAULT_THRESHOLDS },

  affected: null,
  saveBaseline: false,
  ignoreKnown: null,
  reporter: 'default',
  focus: null,
  focusDepth: 1,
  collapse: null,
  atLeast: null,
  configFile: null,
};

export const PILLAR_CATEGORIES: Record<string, string[]> = {
  architecture: [
    'dependency-cycle',
    'dependency-critical-path',
    'dependency-test-only',
    'architecture-sdp-violation',
    'high-coupling',
    'god-module-coupling',
    'orphan-module',
    'unreachable-module',
    'layer-violation',
    'low-cohesion',
    'mega-folder',
    'distance-from-main-sequence',
    'feature-envy',
    'untested-critical-code',
    'over-abstraction',
    'concrete-dependency',
    'circular-type-dependency',
    'shotgun-surgery',
    'import-side-effect-risk',
    'cycle-cluster',
    'broker-module',
    'bridge-module',
    'package-boundary-chatter',
    'startup-risk-hub',
    'namespace-import',
    'commonjs-in-esm',
    'export-star-leak',
    'mixed-module-format',
  ],
  'code-quality': [
    'duplicate-function-body',
    'duplicate-flow-structure',
    'function-optimization',
    'cognitive-complexity',
    'god-module',
    'god-function',
    'halstead-effort',
    'low-maintainability',
    'excessive-parameters',
    'unsafe-any',
    'empty-catch',
    'switch-no-default',
    'unused-parameter',
    'deep-override-chain',
    'interface-compliance',
    'type-assertion-escape',
    'promise-misuse',
    'narrowable-type',
    'missing-error-boundary',
    'await-in-loop',
    'sync-io',
    'uncleared-timer',
    'listener-leak-risk',
    'unbounded-collection',
    'similar-function-body',
    'message-chain',
    'deep-nesting',
    'multiple-return-paths',
    'catch-rethrow',
    'magic-string',
    'boolean-parameter-cluster',
    'promise-all-unhandled',
    'export-surface-density',
    'change-risk',
  ],
  'dead-code': [
    'dead-export',
    'dead-re-export',
    're-export-duplication',
    're-export-shadowed',
    'unused-npm-dependency',
    'package-boundary-violation',
    'barrel-explosion',
    'unused-import',
    'orphan-implementation',
    'move-to-caller',
    'semantic-dead-export',
    'dead-file',
  ],
  security: [
    'hardcoded-secret',
    'eval-usage',
    'unsafe-html',
    'sql-injection-risk',
    'unsafe-regex',
    'prototype-pollution-risk',
    'unvalidated-input-sink',
    'input-passthrough-risk',
    'path-traversal-risk',
    'command-injection-risk',
    'debug-log-leakage',
    'sensitive-data-logging',
  ],
  'test-quality': [
    'low-assertion-density',
    'test-no-assertion',
    'excessive-mocking',
    'shared-mutable-state',
    'missing-test-cleanup',
    'focused-test',
    'fake-timer-no-restore',
    'missing-mock-restoration',
  ],
};

export const ALL_CATEGORIES = new Set(Object.values(PILLAR_CATEGORIES).flat());

export const SEMANTIC_CATEGORIES = new Set([
  'over-abstraction',
  'concrete-dependency',
  'circular-type-dependency',
  'unused-parameter',
  'deep-override-chain',
  'interface-compliance',
  'unused-import',
  'orphan-implementation',
  'shotgun-surgery',
  'move-to-caller',
  'narrowable-type',
  'semantic-dead-export',
]);
export const ALLOWED_EXTS = new Set([
  '.ts',
  '.tsx',
  '.js',
  '.jsx',
  '.mjs',
  '.cjs',
  '.py',
]);
export const IMPORT_RESOLVE_EXTS = [
  '.ts',
  '.tsx',
  '.js',
  '.jsx',
  '.mjs',
  '.cjs',
  '.d.ts',
];

export const TS_CONTROL_KINDS = new Set<number>([
  ts.SyntaxKind.IfStatement,
  ts.SyntaxKind.SwitchStatement,
  ts.SyntaxKind.TryStatement,
  ts.SyntaxKind.ForStatement,
  ts.SyntaxKind.WhileStatement,
  ts.SyntaxKind.DoStatement,
  ts.SyntaxKind.ForOfStatement,
  ts.SyntaxKind.ForInStatement,
  ts.SyntaxKind.ConditionalExpression,
]);

export const TS_TREE_SITTER_CONTROL_TYPES = new Set<string>([
  'if_statement',
  'switch_statement',
  'try_statement',
  'for_statement',
  'while_statement',
  'do_statement',
  'for_in_statement',
  'for_of_statement',
  'for_await_statement',
  'conditional_expression',
  'catch_clause',
]);

export const TS_TREE_SITTER_FUNCTION_TYPES = new Set<string>([
  'function_declaration',
  'function',
  'generator_function',
  'generator_function_declaration',
  'method_definition',
  'arrow_function',
  'function_expression',
]);

export const PY_TREE_SITTER_CONTROL_TYPES = new Set<string>([
  'if_statement',
  'for_statement',
  'while_statement',
  'try_statement',
  'except_clause',
  'with_statement',
  'conditional_expression',
  'match_statement',
]);

export const PY_TREE_SITTER_FUNCTION_TYPES = new Set<string>([
  'function_definition',
]);

export const PYTHON_EXTS = new Set(['.py']);

export function isPythonFile(ext: string): boolean {
  return PYTHON_EXTS.has(ext);
}

export const EMPTY_DEPENDENCY_PROFILE: DependencyProfile = {
  internalDependencies: [],
  externalDependencies: [],
  unresolvedDependencies: [],
  declaredExports: [],
  importedSymbols: [],
  reExports: [],
};

interface SeverityOrder {
  [key: string]: number;
}

export const SEVERITY_ORDER: SeverityOrder = {
  critical: 4,
  high: 3,
  medium: 2,
  low: 1,
  info: 0,
};
