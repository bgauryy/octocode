import { createHash } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { basename, extname, join, relative, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { engine } from '../_engine.mjs';

// Canonical engine via the shared loader (index.cjs napi binary).
const {
  applyContentViewMinification,
  applyMinification,
  extractSignatures,
  MINIFY_CONFIG,
  minifyContent,
  minifyContentSync,
  SUPPORTED_SIGNATURE_EXTENSIONS,
} = engine;

const DEFAULT_CORPUS_ROOT = '/tmp/octocode-context-real-corpus';
const EXCERPT_CHARS = 1800;
const SYMBOL_EXCERPT_CHARS = 2600;
const SKIPPED_DIRECTORY_NAMES = new Set([
  '.git',
  '.octocode',
  'node_modules',
  'dist',
  'out',
  'coverage',
  '.next',
  'target',
  'build',
  '.gradle',
]);

const LANGUAGE_NAMES = {
  adb: 'Ada Body',
  ads: 'Ada Spec',
  asm: 'Assembly',
  awk: 'Awk',
  bash: 'Bash',
  bzl: 'Bazel/Starlark',
  c: 'C',
  cc: 'C++',
  cfg: 'Config',
  cjs: 'CommonJS',
  clj: 'Clojure',
  cljs: 'ClojureScript',
  cmake: 'CMake',
  coffee: 'CoffeeScript',
  conf: 'Config',
  config: 'Config',
  cpp: 'C++',
  cs: 'C#',
  css: 'CSS',
  csv: 'CSV',
  dart: 'Dart',
  dockerignore: 'Dockerignore',
  ejs: 'EJS',
  elm: 'Elm',
  env: 'Env',
  erb: 'ERB',
  erl: 'Erlang',
  ex: 'Elixir',
  exs: 'Elixir Script',
  f: 'Fortran',
  f03: 'Fortran 2003',
  f08: 'Fortran 2008',
  f90: 'Fortran 90',
  f95: 'Fortran 95',
  fish: 'Fish Shell',
  for: 'Fortran',
  fs: 'F#',
  fsx: 'F# Script',
  gitignore: 'Gitignore',
  go: 'Go',
  gql: 'GraphQL',
  gradle: 'Gradle',
  graphql: 'GraphQL',
  groovy: 'Groovy',
  h: 'C Header',
  haml: 'Haml',
  handlebars: 'Handlebars',
  hbs: 'Handlebars',
  hpp: 'C++ Header',
  hrl: 'Erlang Header',
  hs: 'Haskell',
  htm: 'HTML',
  html: 'HTML',
  ini: 'INI',
  jade: 'Jade',
  java: 'Java',
  jinja: 'Jinja',
  jinja2: 'Jinja2',
  jl: 'Julia',
  js: 'JavaScript',
  json: 'JSON',
  json5: 'JSON5',
  jsonc: 'JSONC',
  jsx: 'JSX',
  kotlin: 'Kotlin',
  kt: 'Kotlin',
  less: 'Less',
  lhs: 'Literate Haskell',
  lisp: 'Lisp',
  log: 'Log',
  lsp: 'Lisp',
  lua: 'Lua',
  markdown: 'Markdown',
  md: 'Markdown',
  mjs: 'ESM JavaScript',
  mm: 'Objective-C++',
  mustache: 'Mustache',
  nasm: 'Netwide Assembly',
  nim: 'Nim',
  nix: 'Nix',
  pas: 'Pascal',
  perl: 'Perl',
  php: 'PHP',
  pkb: 'PL/SQL Body',
  pks: 'PL/SQL Spec',
  pl: 'Perl',
  pls: 'PL/SQL',
  plsql: 'PL/SQL',
  pm: 'Perl Module',
  pp: 'Puppet',
  properties: 'Properties',
  proto: 'Protocol Buffers',
  ps1: 'PowerShell',
  psd1: 'PowerShell Data',
  psm1: 'PowerShell Module',
  pug: 'Pug',
  py: 'Python',
  r: 'R',
  rb: 'Ruby',
  rkt: 'Racket',
  rs: 'Rust',
  rst: 'reStructuredText',
  rust: 'Rust',
  sass: 'Sass',
  scala: 'Scala',
  scm: 'Scheme',
  scss: 'SCSS',
  sh: 'Shell',
  slim: 'Slim',
  sql: 'SQL',
  star: 'Starlark',
  styl: 'Stylus',
  svelte: 'Svelte',
  svg: 'SVG',
  swift: 'Swift',
  tf: 'Terraform',
  tfvars: 'Terraform Vars',
  toml: 'TOML',
  ts: 'TypeScript',
  tsql: 'Transact-SQL',
  tsx: 'TSX',
  twig: 'Twig',
  txt: 'Text',
  v: 'V or Verilog',
  vb: 'Visual Basic',
  vbs: 'VBScript',
  vhd: 'VHDL',
  vhdl: 'VHDL',
  vue: 'Vue',
  wast: 'WebAssembly Text',
  wat: 'WebAssembly Text',
  xml: 'XML',
  xsl: 'XSLT',
  xslt: 'XSLT',
  yaml: 'YAML',
  yml: 'YAML',
  zig: 'Zig',
  zsh: 'Zsh',
};

const ENGINE_BACKED_EXTENSIONS = new Set([
  'js',
  'mjs',
  'cjs',
  'jsx',
  'ts',
  'tsx',
  'css',
  'less',
  'scss',
  'html',
  'htm',
  'vue',
  'svelte',
  'json',
  'jsonc',
  'json5',
]);

const COMMON_TYPE_MATRIX_EXTENSIONS = [
  'js',
  'cjs',
  'mjs',
  'jsx',
  'ts',
  'tsx',
  'json',
  'jsonc',
  'css',
  'scss',
  'html',
  'vue',
  'svelte',
  'py',
  'java',
  'go',
  'rs',
  'c',
  'cpp',
  'h',
  'hpp',
  'cs',
  'php',
  'rb',
  'sh',
  'sql',
  'yml',
  'toml',
  'lua',
  'graphql',
  'md',
  'rst',
  'scala',
  'swift',
  'kt',
  'dart',
  'r',
  'proto',
];

const AGENT_UNDERSTANDING_MARKERS = {
  c: [/#include\b/, /\b(?:static|extern|int|void|char|struct)\b/, /\breturn\b/],
  h: [
    /#(?:include|define|ifndef|pragma)\b/,
    /\b(?:typedef|struct|enum)\b/,
    /[;{}]/,
  ],
  cpp: [
    /#include\b/,
    /\b(?:namespace|class|template|std::|auto)\b/,
    /\breturn\b/,
  ],
  hpp: [
    /#(?:include|define|ifndef|pragma)\b/,
    /\b(?:namespace|class|template)\b/,
    /[;{}]/,
  ],
  cs: [
    /\bnamespace\b/,
    /\b(?:public|private|internal|class|record|struct)\b/,
    /\breturn\b/,
  ],
  css: [/[.#]?[\w-]+\s*\{/, /--[\w-]+\s*:/, /:\s*[^;{}]+;/],
  scss: [/[.#]?[\w-]+\s*\{/, /[$@][\w-]+/, /:\s*[^;{}]+;/],
  dart: [
    /\b(?:class|mixin|extension|enum)\b/,
    /\b(?:final|const|var|Future)\b/,
    /\breturn\b/,
  ],
  erl: [/-module\(/, /-export\(/, /\b\w+\([^)]*\)\s*->/],
  ex: [/\bdefmodule\b/, /\bdef(?:p)?\b/, /\b(?:do|end)\b/],
  go: [/\bpackage\s+\w+/, /\bfunc\s+\w+/, /\b(?:type|struct|interface)\b/],
  graphql: [
    /\b(?:query|mutation|subscription|fragment|type|schema)\b/,
    /\b\w+\s*[:(]/,
    /[{}]/,
  ],
  hs: [/\bmodule\b/, /::/, /\b(?:data|newtype|class|instance|where)\b/],
  html: [/<[a-z][^>]*>/i, /<\/[a-z][^>]*>/i, /\b(?:class|id|href|src)=/i],
  ini: [/^\s*[\w.-]+\s*=/m, /^\s*\[[^\]]+]/m, /[^\s]/],
  java: [
    /\bpackage\b/,
    /\b(?:public|private|protected|class|interface|enum)\b/,
    /\breturn\b/,
  ],
  js: [
    /\b(?:function|const|let|var|class)\b/,
    /=>/,
    /\b(?:import|export|require)\b/,
  ],
  cjs: [
    /\b(?:module\.exports|exports\.|require)\b/,
    /\b(?:function|const|let|var|class)\b|=>/,
    /[{}()[\];,]/,
  ],
  mjs: [
    /\bimport\s+[\w*{]/,
    /\bexport\s+(?:default|const|function|class)\b/,
    /[{}()[\];,]/,
  ],
  json: [/^\s*[{[]/, /"[^"]+"\s*:/, /[:,\]}]/],
  jsonc: [/^\s*[{[]/, /"[^"]+"\s*:/, /[:,\]}]/],
  jsx: [
    /<[A-Z_a-z][\w.:~-]*/,
    /\b(?:function|const|class)\b|=>/,
    /\b(?:import|export)\b/,
  ],
  kt: [
    /\bpackage\b/,
    /\b(?:class|object|interface|fun|val|var)\b/,
    /\breturn\b/,
  ],
  lua: [
    /\b(?:local\s+function|function)\b/,
    /\b(?:local|return|end)\b/,
    /[()]/,
  ],
  md: [
    /^#{1,6}\s+\S/m,
    /\[[^\]]+](?:\[[^\]]*]|\([^)]+\))/,
    /^(?:[-+*]|\d+[.)])\s+\S/m,
  ],
  php: [/<\?php/, /\b(?:class|function|namespace|use)\b/, /\$[A-Za-z_]\w*/],
  pl: [/\b(?:sub|my|our|use)\b/, /\$[A-Za-z_]\w*/, /\breturn\b/],
  pm: [/\b(?:package|sub|my|our|use)\b/, /\$[A-Za-z_]\w*/, /\breturn\b/],
  proto: [
    /\bsyntax\s*=/,
    /\b(?:message|service|enum|rpc)\b/,
    /\b(?:optional|repeated|reserved)\b/,
  ],
  py: [/\b(?:def|class)\s+\w+/, /\b(?:import|from)\s+\w+/, /\breturn\b/],
  r: [/\bfunction\s*\(/, /<-\s*/, /\b(?:library|return)\b/],
  rb: [/\b(?:class|module|def)\b/, /\b(?:require|include|extend)\b/, /\bend\b/],
  rs: [
    /\b(?:pub\s+)?(?:fn|struct|enum|trait|impl)\b/,
    /\buse\s+[\w:]+/,
    /\b(?:match|return|Self)\b/,
  ],
  rst: [/^[-=~`#*]{3,}$/m, /::\s*$/m, /^\.\.\s+\w+::/m],
  scala: [
    /\b(?:class|object|trait|enum|case class)\b/,
    /\b(?:def|val|var)\b/,
    /=>|:/,
  ],
  sh: [
    /^#!|(?:^|\s)(?:function\s+)?[A-Za-z_]\w*\s*\(\)/m,
    /\b(?:if|then|fi|case|esac|for|do|done)\b/,
    /\$\{?[A-Za-z_]\w*/,
  ],
  sql: [
    /\bSELECT\b/i,
    /\bFROM\b/i,
    /\b(?:WHERE|JOIN|GROUP BY|ORDER BY|INSERT|UPDATE|CREATE)\b/i,
  ],
  svelte: [/<script\b/i, /<[A-Z_a-z][\w.:~-]*/, /{[#/:@]?\w+/],
  swift: [
    /\b(?:class|struct|enum|protocol|extension)\b/,
    /\b(?:func|let|var)\b/,
    /\breturn\b/,
  ],
  toml: [/^\s*\[[^\]]+]/m, /^\s*[\w.-]+\s*=/m, /"[^"]*"|'[^']*'/],
  ts: [
    /\b(?:interface|type|enum|class|function|const|let)\b/,
    /=>/,
    /\b(?:import|export)\b/,
  ],
  tsx: [
    /<[A-Z_a-z][\w.:~-]*/,
    /\b(?:interface|type|function|const|class)\b|=>/,
    /\b(?:import|export)\b/,
  ],
  vb: [
    /\b(?:Public|Private|Friend|Class|Module|Function|Sub)\b/i,
    /\b(?:If|Then|End|Return)\b/i,
    /\b(?:Dim|As|New)\b/i,
  ],
  vue: [
    /<template\b|<script\b|<style\b/i,
    /<[A-Z_a-z][\w.:~-]*/,
    /(?:v-|:|@)[\w-]+=/,
  ],
  yml: [/^\s*[\w.-]+:/m, /^\s*-\s+\S/m, /[^\s]/],
};

const DEFAULT_UNDERSTANDING_MARKERS = [
  /\b[A-Za-z_]\w{2,}\b/,
  /[=:;{}()[\]-]/,
  /\n/,
];

function extensionFor(filePath) {
  return extname(filePath).slice(1).toLowerCase();
}

function isSupportedExtension(ext) {
  return Object.prototype.hasOwnProperty.call(MINIFY_CONFIG.fileTypes, ext);
}

function collectSamples(root) {
  const samplesByExtension = new Map();

  function visit(entryPath) {
    const stats = statSync(entryPath);
    if (stats.isDirectory()) {
      const name = basename(entryPath);
      if (SKIPPED_DIRECTORY_NAMES.has(name)) return;
      for (const child of readdirSync(entryPath).sort()) {
        visit(join(entryPath, child));
      }
      return;
    }

    if (!stats.isFile()) return;
    const ext = extensionFor(entryPath);
    if (!isSupportedExtension(ext)) return;
    if (samplesByExtension.has(ext)) return;

    const content = readFileSync(entryPath, 'utf8');
    if (content.trim().length === 0) return;

    samplesByExtension.set(ext, {
      ext,
      path: entryPath,
      relativePath: relative(root, entryPath),
      content,
      bytes: Buffer.byteLength(content, 'utf8'),
    });
  }

  visit(root);
  return [...samplesByExtension.values()].sort((a, b) =>
    a.ext.localeCompare(b.ext)
  );
}

function timed(operation) {
  const start = performance.now();
  const value = operation();
  return { value, durationMs: performance.now() - start };
}

async function timedAsync(operation) {
  const start = performance.now();
  const value = await operation();
  return { value, durationMs: performance.now() - start };
}

function byteLength(value) {
  return Buffer.byteLength(value, 'utf8');
}

function cutPercent(originalBytes, outputBytes) {
  if (originalBytes === 0) return 0;
  return ((originalBytes - outputBytes) / originalBytes) * 100;
}

function round(value, digits = 1) {
  return Number(value.toFixed(digits));
}

function hashContent(content) {
  return createHash('sha256').update(content).digest('hex');
}

function clampScore(score) {
  return Math.max(0, Math.min(10, round(score)));
}

function markerPatternsFor(ext) {
  return AGENT_UNDERSTANDING_MARKERS[ext] ?? DEFAULT_UNDERSTANDING_MARKERS;
}

function syntaxAnchorRating(ext, output) {
  const patterns = markerPatternsFor(ext);
  const markerInput = output.replace(/^\s*\d+\|\s?/gm, '');
  const hits = patterns.filter(pattern => pattern.test(markerInput)).length;
  return {
    score: clampScore((hits / patterns.length) * 10),
    hits,
    total: patterns.length,
  };
}

function countCharacter(content, target) {
  let count = 0;
  for (const character of content) {
    if (character === target) count++;
  }
  return count;
}

function delimiterStructureScore(output) {
  const pairs = [
    ['(', ')'],
    ['[', ']'],
    ['{', '}'],
  ];
  const activePairs = pairs
    .map(([open, close]) => {
      const openCount = countCharacter(output, open);
      const closeCount = countCharacter(output, close);
      return { openCount, closeCount };
    })
    .filter(pair => pair.openCount + pair.closeCount > 0);

  if (activePairs.length === 0) return 8;

  return round(
    average(activePairs, pair => {
      const total = pair.openCount + pair.closeCount;
      const imbalance = Math.abs(pair.openCount - pair.closeCount) / total;
      return clampScore(10 - imbalance * 10);
    })
  );
}

function outputHealthScore(sourceBytes, outputBytes, failed, output) {
  let score = 0;
  if (output.trim().length > 0) score += 3;
  if (outputBytes <= sourceBytes) score += 2.5;
  if (!failed) score += 2;
  if (!/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(output)) score += 1.5;
  if (!/\b(?:undefined|null|\[object Object])\b/.test(output)) score += 1;
  return clampScore(score);
}

function contextBudgetScore(cutPercent) {
  if (cutPercent >= 80) return 8;
  if (cutPercent >= 45) return 10;
  if (cutPercent >= 25) return 9;
  if (cutPercent >= 10) return 8;
  if (cutPercent >= 3) return 7;
  if (cutPercent > 0) return 6;
  return 5;
}

function symbolContextScore(symbols) {
  if (!symbols.supported) return 7;
  return symbols.returned ? 10 : 4;
}

function agentOutputObservation({
  sample,
  metric,
  output,
  outputName,
  outputBytes,
  cutPercent: outputCutPercent,
  failed = false,
  symbolScore: outputSymbolScore = symbolContextScore(metric.symbols),
}) {
  const syntaxAnchors = syntaxAnchorRating(sample.ext, output);
  const structure = delimiterStructureScore(output);
  const outputHealth = outputHealthScore(
    metric.sourceBytes,
    outputBytes,
    failed,
    output
  );
  const contextBudget = contextBudgetScore(outputCutPercent);
  const symbols = outputSymbolScore;
  const score = round(
    syntaxAnchors.score * 0.4 +
      structure * 0.2 +
      outputHealth * 0.2 +
      contextBudget * 0.1 +
      symbols * 0.1
  );
  const signals = [
    {
      name: `${outputName} output is non-empty`,
      passed: output.trim().length > 0,
    },
    {
      name: `${outputName} did not grow`,
      passed: outputBytes <= metric.sourceBytes,
    },
    {
      name: `${outputName} completed`,
      passed: !failed,
    },
    {
      name: 'language syntax anchors present',
      passed: syntaxAnchors.score >= 6.7,
    },
    {
      name: 'delimiter structure is balanced enough',
      passed: structure >= 7,
    },
    {
      name: 'symbol skeleton returned or not configured',
      passed: !metric.symbols.supported || metric.symbols.returned,
    },
  ];

  return {
    output: outputName,
    bytes: outputBytes,
    cutPercent: outputCutPercent,
    score,
    label: ratingLabel(score),
    syntaxAnchors,
    structure,
    outputHealth,
    contextBudget,
    symbols,
    signals,
  };
}

function rawAgentObservation(sample, metric) {
  const output = sample.content;
  const syntaxAnchors = syntaxAnchorRating(sample.ext, output);
  const structure = delimiterStructureScore(output);
  const outputHealth = outputHealthScore(
    metric.sourceBytes,
    metric.sourceBytes,
    false,
    output
  );

  return {
    output: 'none',
    bytes: metric.sourceBytes,
    cutPercent: 0,
    score: 10,
    label: 'excellent',
    syntaxAnchors,
    structure,
    outputHealth,
    contextBudget: 5,
    symbols: symbolContextScore(metric.symbols),
    signals: [
      { name: 'none output is exact source', passed: true },
      { name: 'none output is non-empty', passed: output.trim().length > 0 },
      { name: 'none preserves source fidelity', passed: true },
    ],
  };
}

function agentObservationsForSample({
  sample,
  metric,
  contentView,
  asyncContent,
  symbolContent,
}) {
  return {
    none: rawAgentObservation(sample, metric),
    standard: agentOutputObservation({
      sample,
      metric,
      output: contentView,
      outputName: 'standard',
      outputBytes: metric.contentView.bytes,
      cutPercent: metric.contentView.cutPercent,
    }),
    minify: agentOutputObservation({
      sample,
      metric,
      output: asyncContent,
      outputName: 'minify',
      outputBytes: metric.async.bytes,
      cutPercent: metric.async.cutPercent,
      failed: metric.async.failed,
    }),
    symbols:
      symbolContent === null || metric.symbols.bytes === null
        ? null
        : agentOutputObservation({
            sample,
            metric,
            output: symbolContent,
            outputName: 'symbols',
            outputBytes: metric.symbols.bytes,
            cutPercent: metric.symbols.cutPercent ?? 0,
            symbolScore: metric.symbols.returned ? 10 : 4,
          }),
  };
}

function minifyScore(metric) {
  const cuts = [
    metric.contentView.cutPercent,
    metric.applyMinification.cutPercent,
    metric.sync.cutPercent,
    metric.async.cutPercent,
  ];
  const bestCut = Math.max(...cuts);
  const noGrowth = cuts.every(value => value >= 0);
  const nonEmpty = [
    metric.contentView.bytes,
    metric.applyMinification.bytes,
    metric.sync.bytes,
    metric.async.bytes,
  ].every(value => value > 0);

  let score = 0;
  if (noGrowth) score += 3;
  if (!metric.async.failed) score += 1;
  if (nonEmpty) score += 1;

  if (bestCut >= 60) score += 4;
  else if (bestCut >= 40) score += 3.5;
  else if (bestCut >= 25) score += 3;
  else if (bestCut >= 15) score += 2.25;
  else if (bestCut >= 5) score += 1.5;
  else if (bestCut > 0) score += 0.75;

  if (ENGINE_BACKED_EXTENSIONS.has(metric.ext)) score += 1;
  else score += 0.5;

  return Math.min(10, round(score));
}

function symbolScore(metric) {
  if (!metric.symbols.supported) return null;
  if (!metric.symbols.returned) return 3;
  const cut = metric.symbols.cutPercent ?? 0;
  if (cut >= 80) return 10;
  if (cut >= 60) return 9;
  if (cut >= 40) return 8;
  if (cut > 0) return 6.5;
  return 5;
}

function agentScore(metric) {
  const minify = minifyScore(metric);
  const symbols = symbolScore(metric);
  if (symbols === null) return minify;
  return round(minify * 0.65 + symbols * 0.35);
}

function ratingLabel(score) {
  if (score >= 9) return 'excellent';
  if (score >= 8) return 'strong';
  if (score >= 7) return 'good';
  if (score >= 6) return 'fair';
  return 'needs work';
}

function readmeReductionScore(cutPercent) {
  if (cutPercent >= 35) return 10;
  if (cutPercent >= 20) return 8;
  if (cutPercent >= 10) return 6;
  if (cutPercent >= 3) return 4;
  if (cutPercent > 0) return 2;
  return 1;
}

function readmeMinificationRating(metrics) {
  const metric = metrics.find(candidate => candidate.ext === 'md');
  if (!metric) return null;

  const output = metric.excerpts.contentView;
  const signals = [
    {
      name: 'no growth',
      passed: metric.contentView.bytes <= metric.sourceBytes,
    },
    {
      name: 'non-empty output',
      passed: metric.contentView.bytes > 0,
    },
    {
      name: 'markdown strategy completed',
      passed: !metric.async.failed && metric.async.type === 'markdown',
    },
    {
      name: 'headings preserved',
      passed: /^#{1,6}\s+\S/m.test(output),
    },
    {
      name: 'links or references preserved',
      passed: /\[[^\]]+](?:\[[^\]]*]|\([^)]+\))|^\[[^\]]+]:\s+\S/m.test(output),
    },
    {
      name: 'lists preserved',
      passed: /^(?:[-+*]|\d+[.)])\s+\S/m.test(output),
    },
  ];
  const passedSignals = signals.filter(signal => signal.passed).length;
  const readabilityScore = round((passedSignals / signals.length) * 10, 1);
  const byteReductionScore = readmeReductionScore(
    metric.contentView.cutPercent
  );
  const score = round(readabilityScore * 0.7 + byteReductionScore * 0.3, 1);

  return {
    source: metric.source,
    inputBytes: metric.sourceBytes,
    outputBytes: metric.contentView.bytes,
    cutPercent: metric.contentView.cutPercent,
    asyncType: metric.async.type,
    readabilityScore,
    byteReductionScore,
    score,
    label: ratingLabel(score),
    signals,
    notes: [
      'README score weights semantic preservation at 70% and byte reduction at 30%.',
      'Low byte cuts are acceptable when the README is already dense and mostly semantic content.',
      'Use this score to track whether Markdown changes preserve rendered README logic while still removing redundant tokens.',
    ],
  };
}

function average(metrics, selector) {
  if (metrics.length === 0) return 0;
  return (
    metrics.reduce((sum, metric) => sum + selector(metric), 0) / metrics.length
  );
}

function qualitySummary(metrics) {
  const symbolMetrics = metrics.filter(metric => metric.symbols.supported);
  const buckets = metrics.reduce((accumulator, metric) => {
    const label = metric.ratings.label;
    accumulator[label] = (accumulator[label] ?? 0) + 1;
    return accumulator;
  }, {});
  const weakest = [...metrics]
    .sort((left, right) => left.ratings.agent - right.ratings.agent)
    .slice(0, 10)
    .map(metric => ({
      ext: metric.ext,
      language: metric.language,
      agent: metric.ratings.agent,
      label: metric.ratings.label,
      contentViewCut: metric.contentView.cutPercent,
      applyCut: metric.applyMinification.cutPercent,
      asyncCut: metric.async.cutPercent,
      symbolsReturned: metric.symbols.returned,
    }));

  return {
    averageAgent: round(
      average(metrics, metric => metric.ratings.agent),
      2
    ),
    averageMinify: round(
      average(metrics, metric => metric.ratings.minify),
      2
    ),
    averageSymbolsAllMeasured: round(
      average(metrics, metric => metric.ratings.symbols ?? 0),
      2
    ),
    averageSymbolsWhenSupported: round(
      average(symbolMetrics, metric => metric.ratings.symbols ?? 0),
      2
    ),
    averageContentViewCut: round(
      average(metrics, metric => metric.contentView.cutPercent)
    ),
    averageApplyCut: round(
      average(metrics, metric => metric.applyMinification.cutPercent)
    ),
    averageAsyncCut: round(average(metrics, metric => metric.async.cutPercent)),
    symbolsSupported: symbolMetrics.length,
    symbolsReturned: symbolMetrics.filter(metric => metric.symbols.returned)
      .length,
    buckets,
    weakest,
  };
}

function agentUnderstandingSummary(metrics) {
  const buckets = metrics.reduce((accumulator, metric) => {
    const label = metric.agentUnderstanding.label;
    accumulator[label] = (accumulator[label] ?? 0) + 1;
    return accumulator;
  }, {});
  const weakest = [...metrics]
    .sort(
      (left, right) =>
        left.agentUnderstanding.score - right.agentUnderstanding.score
    )
    .slice(0, 10)
    .map(metric => ({
      ext: metric.ext,
      language: metric.language,
      score: metric.agentUnderstanding.score,
      label: metric.agentUnderstanding.label,
      syntaxAnchors: metric.agentUnderstanding.syntaxAnchors.score,
      structure: metric.agentUnderstanding.structure,
      outputHealth: metric.agentUnderstanding.outputHealth,
      contextBudget: metric.agentUnderstanding.contextBudget,
      symbols: metric.agentUnderstanding.symbols,
    }));

  return {
    averageScore: round(
      average(metrics, metric => metric.agentUnderstanding.score),
      2
    ),
    buckets,
    weakest,
  };
}

function agentObservationLevelSummary(metrics, level) {
  const observations = metrics.flatMap(metric => {
    const observation = metric.agentObservations[level];
    return observation ? [{ metric, observation }] : [];
  });
  const buckets = observations.reduce((accumulator, entry) => {
    const label = entry.observation.label;
    accumulator[label] = (accumulator[label] ?? 0) + 1;
    return accumulator;
  }, {});
  const weakest = observations
    .sort((left, right) => left.observation.score - right.observation.score)
    .slice(0, 10)
    .map(entry => ({
      ext: entry.metric.ext,
      language: entry.metric.language,
      score: entry.observation.score,
      label: entry.observation.label,
      cutPercent: entry.observation.cutPercent,
      syntaxAnchors: entry.observation.syntaxAnchors.score,
      structure: entry.observation.structure,
      outputHealth: entry.observation.outputHealth,
    }));

  return {
    count: observations.length,
    averageScore: round(
      average(observations, entry => entry.observation.score),
      2
    ),
    averageCut: round(
      average(observations, entry => entry.observation.cutPercent),
      2
    ),
    buckets,
    weakest,
  };
}

function agentObservationSummary(metrics) {
  const levels = {
    none: agentObservationLevelSummary(metrics, 'none'),
    standard: agentObservationLevelSummary(metrics, 'standard'),
    minify: agentObservationLevelSummary(metrics, 'minify'),
    symbols: agentObservationLevelSummary(metrics, 'symbols'),
  };
  const bestNavigationScores = metrics.map(metric =>
    Math.max(
      metric.agentObservations.standard.score,
      metric.agentObservations.symbols?.score ?? 0
    )
  );

  return {
    overallAgentUsefulness: round(
      bestNavigationScores.reduce((sum, score) => sum + score, 0) /
        bestNavigationScores.length,
      2
    ),
    levels,
  };
}

function asyncTypeDistribution(metrics) {
  return metrics.reduce((accumulator, metric) => {
    const type = metric.async.type;
    accumulator[type] ??= {
      count: 0,
      extensions: [],
    };
    accumulator[type].count++;
    accumulator[type].extensions.push(metric.ext);
    accumulator[type].extensions.sort();
    return accumulator;
  }, {});
}

function commonTypeMetrics(metrics) {
  const metricsByExtension = new Map(
    metrics.map(metric => [metric.ext, metric])
  );
  return COMMON_TYPE_MATRIX_EXTENSIONS.flatMap(ext => {
    const metric = metricsByExtension.get(ext);
    return metric ? [metric] : [];
  });
}

function excerpt(content, maxChars = EXCERPT_CHARS) {
  if (content.length <= maxChars) return content;
  const headLength = Math.floor(maxChars * 0.68);
  const tailLength = maxChars - headLength;
  return `${content.slice(0, headLength)}

... [truncated ${content.length - maxChars} chars] ...

${content.slice(content.length - tailLength)}`;
}

function fenceLanguage(ext) {
  if (ext === 'yml') return 'yaml';
  if (ext === 'md') return 'markdown';
  if (ext === 'h') return 'c';
  if (ext === 'hpp' || ext === 'cc') return 'cpp';
  if (ext === 'mjs' || ext === 'cjs') return 'js';
  return ext;
}

function notesFor(metric) {
  const notes = [];
  if (ENGINE_BACKED_EXTENSIONS.has(metric.ext)) {
    notes.push('engine-backed or parser-backed path');
  } else {
    notes.push(`${metric.strategy} text strategy`);
  }
  if (metric.contentView.cutPercent === 0) {
    notes.push(
      'content-view kept original because the readable output was not shorter'
    );
  }
  if (metric.async.failed) {
    notes.push(
      `async minifier failed: ${metric.async.reason ?? 'unknown reason'}`
    );
  }
  if (!metric.symbols.supported) {
    notes.push('symbols are not implemented for this extension');
  } else if (!metric.symbols.returned) {
    notes.push(
      'symbols extension is registered but this sample produced no skeleton'
    );
  }
  return notes;
}

async function measureSample(sample) {
  const contentView = timed(() =>
    applyContentViewMinification(sample.content, sample.path)
  );
  const apply = timed(() => applyMinification(sample.content, sample.path));
  const sync = timed(() => minifyContentSync(sample.content, sample.path));
  const asyncResult = await timedAsync(() =>
    minifyContent(sample.content, sample.path)
  );
  const symbols = timed(() => extractSignatures(sample.content, sample.path));
  const asyncContent = asyncResult.value.content;
  const symbolContent = symbols.value;
  const symbolBytes = symbolContent === null ? null : byteLength(symbolContent);

  const metric = {
    ext: sample.ext,
    language: LANGUAGE_NAMES[sample.ext] ?? sample.ext,
    source: sample.relativePath,
    sourceBytes: sample.bytes,
    strategy: MINIFY_CONFIG.fileTypes[sample.ext]?.strategy ?? 'fallback',
    commentModel: MINIFY_CONFIG.fileTypes[sample.ext]?.comments ?? null,
    sha256: hashContent(sample.content),
    contentView: {
      bytes: byteLength(contentView.value),
      cutPercent: round(
        cutPercent(sample.bytes, byteLength(contentView.value))
      ),
      durationMs: round(contentView.durationMs, 3),
    },
    applyMinification: {
      bytes: byteLength(apply.value),
      cutPercent: round(cutPercent(sample.bytes, byteLength(apply.value))),
      durationMs: round(apply.durationMs, 3),
    },
    sync: {
      bytes: byteLength(sync.value),
      cutPercent: round(cutPercent(sample.bytes, byteLength(sync.value))),
      durationMs: round(sync.durationMs, 3),
    },
    async: {
      bytes: byteLength(asyncContent),
      cutPercent: round(cutPercent(sample.bytes, byteLength(asyncContent))),
      durationMs: round(asyncResult.durationMs, 3),
      failed: asyncResult.value.failed,
      type: asyncResult.value.type,
      reason: asyncResult.value.reason ?? null,
    },
    symbols: {
      supported: SUPPORTED_SIGNATURE_EXTENSIONS.includes(sample.ext),
      returned: symbolContent !== null,
      bytes: symbolBytes,
      cutPercent:
        symbolBytes === null
          ? null
          : round(cutPercent(sample.bytes, symbolBytes)),
      durationMs: round(symbols.durationMs, 3),
    },
    excerpts: {
      before: excerpt(sample.content),
      contentView: excerpt(contentView.value),
      applyMinification: excerpt(apply.value),
      sync: excerpt(sync.value),
      async: excerpt(asyncContent),
      symbols:
        symbolContent === null
          ? 'No symbols returned for this sample.'
          : excerpt(symbolContent, SYMBOL_EXCERPT_CHARS),
    },
  };

  const minify = minifyScore(metric);
  const symbolsRating = symbolScore(metric);
  const agent = agentScore(metric);
  const agentObservations = agentObservationsForSample({
    sample,
    metric,
    contentView: contentView.value,
    asyncContent,
    symbolContent,
  });
  const agentUnderstanding = agentObservations.standard;

  return {
    ...metric,
    agentUnderstanding,
    agentObservations,
    ratings: {
      minify,
      symbols: symbolsRating,
      agent,
      label: ratingLabel(agent),
    },
    notes: notesFor(metric),
  };
}

function metricTable(metric) {
  const symbolCut =
    metric.symbols.cutPercent === null
      ? 'n/a'
      : `${metric.symbols.cutPercent}%`;

  return [
    '| Tool | Bytes | Cut | Time |',
    '| --- | ---: | ---: | ---: |',
    `| input | ${metric.sourceBytes} | - | - |`,
    `| content-view | ${metric.contentView.bytes} | ${metric.contentView.cutPercent}% | ${metric.contentView.durationMs} ms |`,
    `| applyMinification | ${metric.applyMinification.bytes} | ${metric.applyMinification.cutPercent}% | ${metric.applyMinification.durationMs} ms |`,
    `| sync minify | ${metric.sync.bytes} | ${metric.sync.cutPercent}% | ${metric.sync.durationMs} ms |`,
    `| async minify | ${metric.async.bytes} | ${metric.async.cutPercent}% | ${metric.async.durationMs} ms |`,
    `| symbols | ${metric.symbols.bytes ?? 'n/a'} | ${symbolCut} | ${metric.symbols.durationMs} ms |`,
  ].join('\n');
}

function agentUnderstandingTable(metric) {
  const understanding = metric.agentUnderstanding;
  const signalsPassed = understanding.signals.filter(
    signal => signal.passed
  ).length;

  return [
    '| Component | Score |',
    '| --- | ---: |',
    `| syntax anchors | ${understanding.syntaxAnchors.score}/10 (${understanding.syntaxAnchors.hits}/${understanding.syntaxAnchors.total}) |`,
    `| delimiter structure | ${understanding.structure}/10 |`,
    `| output health | ${understanding.outputHealth}/10 |`,
    `| context budget | ${understanding.contextBudget}/10 |`,
    `| symbol context | ${understanding.symbols}/10 |`,
    `| signals passed | ${signalsPassed}/${understanding.signals.length} |`,
  ].join('\n');
}

function agentObservationTable(metric) {
  const rows = Object.entries(metric.agentObservations).map(
    ([level, observation]) => {
      if (observation === null) {
        return `| ${level} | n/a | n/a | n/a | n/a | n/a |`;
      }

      return `| ${level} | ${observation.bytes} | ${observation.cutPercent}% | ${observation.score}/10 ${observation.label} | ${observation.syntaxAnchors.score}/10 | ${observation.structure}/10 |`;
    }
  );

  return [
    '| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |',
    '| --- | ---: | ---: | ---: | ---: | ---: |',
    ...rows,
  ].join('\n');
}

function renderLanguageReport(metric) {
  const lang = fenceLanguage(metric.ext);
  return `# ${metric.language} (.${metric.ext})

Source sample: \`${metric.source}\`

Strategy: \`${metric.strategy}\`

${metricTable(metric)}

## Notes

${metric.notes.map(note => `- ${note}.`).join('\n')}

## Before Excerpt

\`\`\`${lang}
${metric.excerpts.before}
\`\`\`

## Content-View Excerpt

\`\`\`${lang}
${metric.excerpts.contentView}
\`\`\`

## Apply Minification Excerpt

\`\`\`${lang}
${metric.excerpts.applyMinification}
\`\`\`

## Sync Minify Excerpt

\`\`\`${lang}
${metric.excerpts.sync}
\`\`\`

## Async Minify Excerpt

\`\`\`${lang}
${metric.excerpts.async}
\`\`\`

## Symbols

\`\`\`txt
${metric.excerpts.symbols}
\`\`\`
`;
}

function writeJson(filePath, value) {
  writeFileSync(`${filePath}`, `${JSON.stringify(value, null, 2)}\n`);
}

function sanitizeReportText(content) {
  return content.replace(/\r\n?/g, '\n').replace(/[ \t]+$/gm, '');
}

function writeReportText(filePath, content) {
  writeFileSync(filePath, sanitizeReportText(content));
}

function writeBenchmarkArtifacts(languageDir, metric) {
  const rawDir = join(languageDir, 'raw');
  const minifiedDir = join(languageDir, 'minified');
  const symbolDir = join(languageDir, 'symbol');
  mkdirSync(rawDir, { recursive: true });
  mkdirSync(minifiedDir, { recursive: true });
  mkdirSync(symbolDir, { recursive: true });

  writeReportText(join(rawDir, 'source.excerpt.txt'), metric.excerpts.before);
  writeJson(join(rawDir, 'metadata.json'), {
    source: metric.source,
    sourceBytes: metric.sourceBytes,
    sha256: metric.sha256,
    excerptBytes: byteLength(metric.excerpts.before),
    truncated: metric.excerpts.before.includes('... [truncated '),
  });
  writeReportText(
    join(rawDir, 'README.md'),
    `# Raw Input

Source sample: \`${metric.source}\`

This folder stores an excerpt of the original real-code sample.
Full third-party source files are intentionally not vendored.
`
  );

  const minifiedArtifacts = {
    'content-view': {
      fileName: 'content-view.excerpt.txt',
      metric: metric.contentView,
      content: metric.excerpts.contentView,
      api: 'applyContentViewMinification',
    },
    'apply-minification': {
      fileName: 'apply-minification.excerpt.txt',
      metric: metric.applyMinification,
      content: metric.excerpts.applyMinification,
      api: 'applyMinification',
    },
    'minify-content-sync': {
      fileName: 'minify-content-sync.excerpt.txt',
      metric: metric.sync,
      content: metric.excerpts.sync,
      api: 'minifyContentSync',
    },
    'minify-content-async': {
      fileName: 'minify-content-async.excerpt.txt',
      metric: metric.async,
      content: metric.excerpts.async,
      api: 'minifyContent',
    },
  };

  for (const artifact of Object.values(minifiedArtifacts)) {
    writeReportText(join(minifiedDir, artifact.fileName), artifact.content);
  }

  writeJson(join(minifiedDir, 'metrics.json'), minifiedArtifacts);
  writeReportText(
    join(minifiedDir, 'README.md'),
    `# Minified Outputs

All minification permutations for this sample:

- \`content-view.excerpt.txt\` from \`applyContentViewMinification\`
- \`apply-minification.excerpt.txt\` from \`applyMinification\`
- \`minify-content-sync.excerpt.txt\` from \`minifyContentSync\`
- \`minify-content-async.excerpt.txt\` from \`minifyContent\`

Each file is an excerpt, not the complete third-party source output.
`
  );

  writeReportText(join(symbolDir, 'signatures.txt'), metric.excerpts.symbols);
  writeJson(join(symbolDir, 'metrics.json'), metric.symbols);
  writeReportText(
    join(symbolDir, 'README.md'),
    `# Symbol Output

Generated by \`extractSignatures\`.

Supported for this extension: \`${metric.symbols.supported}\`

Returned symbols for this sample: \`${metric.symbols.returned}\`
`
  );
}

function rootSummary(metrics) {
  const quality = qualitySummary(metrics);
  const typeDistribution = asyncTypeDistribution(metrics);
  const commonTypeRows = commonTypeMetrics(metrics).map(metric => {
    const symbolCut =
      metric.symbols.cutPercent === null
        ? 'n/a'
        : `${metric.symbols.cutPercent}%`;
    return `| \`${metric.ext}\` | ${metric.language} | \`${metric.strategy}\` | \`${metric.async.type}\` | ${metric.sourceBytes} | ${metric.contentView.cutPercent}% | ${metric.applyMinification.cutPercent}% | ${metric.sync.cutPercent}% | ${metric.async.cutPercent}% | ${symbolCut} | \`${metric.source}\` |`;
  });
  const typeDistributionText = Object.entries(typeDistribution)
    .map(([type, info]) => `${type} ${info.count}`)
    .join(', ');
  return `# Real-Code Minifier Benchmark

This directory records before/after excerpts and metrics for one real sample per
discovered extension. Full third-party source files are not vendored here; use
the generator to recreate reports from a local corpus.

## Summary

- Samples covered: ${metrics.length}
- Symbol skeletons returned: ${quality.symbolsReturned}/${quality.symbolsSupported}
- Average cuts: content-view ${quality.averageContentViewCut}%, apply ${quality.averageApplyCut}%, async ${quality.averageAsyncCut}%

## Competitor Baseline

This benchmark rates Octocode as an agent-context compressor. Production
compiler and bundler minifiers are the right baseline for deployable output:

| Competitor | Best At | Octocode Position |
| --- | --- | --- |
| [Terser](https://www.npmjs.com/package/terser) | Production JavaScript parsing, compression, mangling, and formatting. | Used for JS/CJS/MJS and stronger JS-family paths where safe. |
| [esbuild](https://www.npmjs.com/package/esbuild) | Very fast JS/TS/CSS bundling and minification. | Better for production builds; Octocode avoids adding it as a runtime dependency. |
| [SWC](https://www.npmjs.com/package/@swc/core) | Rust-backed JS/TS compilation transforms. | Better compiler-grade path; Octocode uses TypeScript transform plus guarded minification. |
| [Lightning CSS](https://www.npmjs.com/package/lightningcss) | Parser-grade CSS transforms and minification. | Better production CSS optimizer; Octocode uses CleanCSS async and lightweight sync cleanup. |
| [html-minifier-terser](https://www.npmjs.com/package/html-minifier-terser) | HTML minification with embedded asset options. | Used for async HTML; content-view still prioritizes readable agent context. |

## Real Minification Type Matrix

Measured async result types across the real corpus: ${typeDistributionText}.

| Ext | Format | Configured strategy | Async type | Input bytes | Content-view cut | Apply cut | Sync cut | Async cut | Symbols cut | Source |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
${commonTypeRows.join('\n')}

## Regenerate

\`\`\`bash
yarn build
node benchmark/minify/generate-real-code-report.mjs /path/to/real/corpus
\`\`\`
`;
}

function missingReport(missingExtensions) {
  return `# Missing Real Samples

These extensions are configured in \`MINIFY_CONFIG\`, but the local corpus did
not contain a real sample for them.

\`\`\`txt
${missingExtensions.join(', ')}
\`\`\`
`;
}

async function main() {
  // Flag-safe corpus path + an explicit destructive-write gate. This script
  // regenerates minify/<lang>/ by WIPING the committed samples that
  // check-minify.mjs depends on, so it refuses to run without --write.
  const positional = process.argv.slice(2).filter((a) => !a.startsWith('--'));
  const corpusRoot = resolve(positional[0] ?? DEFAULT_CORPUS_ROOT);
  const outputRoot = resolve(new URL('.', import.meta.url).pathname);

  if (!process.argv.includes('--write')) {
    throw new Error(
      `Refusing to regenerate: this WIPES the committed minify/ samples that check-minify.mjs uses. ` +
        `Re-run with --write to regenerate from corpus ${corpusRoot}.`
    );
  }

  if (!existsSync(corpusRoot)) {
    throw new Error(`Corpus root does not exist: ${corpusRoot}`);
  }

  const samples = collectSamples(corpusRoot);
  if (samples.length === 0) {
    throw new Error(`No supported samples found in: ${corpusRoot}`);
  }

  const preserve = new Set(['generate-real-code-report.mjs', 'check-minify.mjs']);
  for (const entry of readdirSync(outputRoot)) {
    if (preserve.has(entry)) continue;
    rmSync(join(outputRoot, entry), { recursive: true, force: true });
  }

  const metrics = [];
  for (const sample of samples) {
    const metric = await measureSample(sample);
    metrics.push(metric);

    const languageDir = join(outputRoot, metric.ext);
    mkdirSync(languageDir, { recursive: true });
    writeBenchmarkArtifacts(languageDir, metric);
    writeReportText(join(languageDir, 'README.md'), renderLanguageReport(metric));
    writeFileSync(
      join(languageDir, 'metrics.json'),
      `${JSON.stringify(
        {
          ...metric,
          excerpts: undefined,
        },
        null,
        2
      )}\n`
    );
  }

  const discoveredExtensions = new Set(metrics.map(metric => metric.ext));
  const missingExtensions = Object.keys(MINIFY_CONFIG.fileTypes)
    .filter(ext => !discoveredExtensions.has(ext))
    .sort();

  writeFileSync(
    join(outputRoot, 'summary.json'),
    `${JSON.stringify(
      {
        corpusRoot,
        generatedAt: new Date().toISOString(),
        configuredExtensions: Object.keys(MINIFY_CONFIG.fileTypes).length,
        coveredExtensions: metrics.length,
        missingExtensions,
        quality: qualitySummary(metrics),
        agentUnderstanding: agentUnderstandingSummary(metrics),
        agentObservations: agentObservationSummary(metrics),
        readmeMinification: readmeMinificationRating(metrics),
        asyncTypeDistribution: asyncTypeDistribution(metrics),
        commonLanguageTypes: commonTypeMetrics(metrics).map(metric => ({
          ext: metric.ext,
          language: metric.language,
          source: metric.source,
          strategy: metric.strategy,
          asyncType: metric.async.type,
          contentViewCut: metric.contentView.cutPercent,
          applyCut: metric.applyMinification.cutPercent,
          syncCut: metric.sync.cutPercent,
          asyncCut: metric.async.cutPercent,
          symbolsCut: metric.symbols.cutPercent,
          rating: metric.ratings.agent,
        })),
        metrics: metrics.map(metric => ({
          ext: metric.ext,
          language: metric.language,
          source: metric.source,
          sourceBytes: metric.sourceBytes,
          strategy: metric.strategy,
          contentView: metric.contentView,
          applyMinification: metric.applyMinification,
          sync: metric.sync,
          async: metric.async,
          symbols: metric.symbols,
          agentUnderstanding: metric.agentUnderstanding,
          agentObservations: metric.agentObservations,
          ratings: metric.ratings,
          notes: metric.notes,
        })),
      },
      null,
      2
    )}\n`
  );
  writeReportText(join(outputRoot, 'README.md'), rootSummary(metrics));
  writeReportText(
    join(outputRoot, 'missing-real-samples.md'),
    missingReport(missingExtensions)
  );

  console.log(
    `Generated ${metrics.length} benchmark reports in ${relative(
      process.cwd(),
      outputRoot
    )}`
  );
}

await main();
