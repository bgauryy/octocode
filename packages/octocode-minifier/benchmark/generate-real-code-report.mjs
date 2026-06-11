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
import {
  applyContentViewMinification,
  applyMinification,
  extractSignatures,
  MINIFY_CONFIG,
  minifyContent,
  minifyContentSync,
  SUPPORTED_SIGNATURE_EXTENSIONS,
} from '../dist/index.js';

const DEFAULT_CORPUS_ROOT = '/tmp/octocode-minifier-real-corpus';
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

function average(metrics, selector) {
  if (metrics.length === 0) return 0;
  return metrics.reduce((sum, metric) => sum + selector(metric), 0) / metrics.length;
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
    averageAgent: round(average(metrics, metric => metric.ratings.agent), 2),
    averageMinify: round(average(metrics, metric => metric.ratings.minify), 2),
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
    symbolsReturned: symbolMetrics.filter(metric => metric.symbols.returned).length,
    buckets,
    weakest,
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
  const metricsByExtension = new Map(metrics.map(metric => [metric.ext, metric]));
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
    notes.push('content-view kept original because the readable output was not shorter');
  }
  if (metric.async.failed) {
    notes.push(`async minifier failed: ${metric.async.reason ?? 'unknown reason'}`);
  }
  if (!metric.symbols.supported) {
    notes.push('symbols are not implemented for this extension');
  } else if (!metric.symbols.returned) {
    notes.push('symbols extension is registered but this sample produced no skeleton');
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
      cutPercent: round(cutPercent(sample.bytes, byteLength(contentView.value))),
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
        symbolBytes === null ? null : round(cutPercent(sample.bytes, symbolBytes)),
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

  return {
    ...metric,
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
    metric.symbols.cutPercent === null ? 'n/a' : `${metric.symbols.cutPercent}%`;
  const symbolRating =
    metric.ratings.symbols === null ? 'n/a' : `${metric.ratings.symbols}/10`;

  return [
    '| Tool | Bytes | Cut | Time | Rating |',
    '| --- | ---: | ---: | ---: | ---: |',
    `| input | ${metric.sourceBytes} | - | - | - |`,
    `| content-view | ${metric.contentView.bytes} | ${metric.contentView.cutPercent}% | ${metric.contentView.durationMs} ms | ${metric.ratings.minify}/10 |`,
    `| applyMinification | ${metric.applyMinification.bytes} | ${metric.applyMinification.cutPercent}% | ${metric.applyMinification.durationMs} ms | ${metric.ratings.minify}/10 |`,
    `| sync minify | ${metric.sync.bytes} | ${metric.sync.cutPercent}% | ${metric.sync.durationMs} ms | ${metric.ratings.minify}/10 |`,
    `| async minify | ${metric.async.bytes} | ${metric.async.cutPercent}% | ${metric.async.durationMs} ms | ${metric.ratings.minify}/10 |`,
    `| symbols | ${metric.symbols.bytes ?? 'n/a'} | ${symbolCut} | ${metric.symbols.durationMs} ms | ${symbolRating} |`,
  ].join('\n');
}

function renderLanguageReport(metric) {
  const lang = fenceLanguage(metric.ext);
  return `# ${metric.language} (.${metric.ext})

Source sample: \`${metric.source}\`

Strategy: \`${metric.strategy}\`

Agent rating: **${metric.ratings.agent}/10 (${metric.ratings.label})**

Artifacts:

- \`raw/source.excerpt.txt\`
- \`minified/content-view.excerpt.txt\`
- \`minified/apply-minification.excerpt.txt\`
- \`minified/minify-content-sync.excerpt.txt\`
- \`minified/minify-content-async.excerpt.txt\`
- \`symbol/signatures.txt\`

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

function writeBenchmarkArtifacts(languageDir, metric) {
  const rawDir = join(languageDir, 'raw');
  const minifiedDir = join(languageDir, 'minified');
  const symbolDir = join(languageDir, 'symbol');
  mkdirSync(rawDir, { recursive: true });
  mkdirSync(minifiedDir, { recursive: true });
  mkdirSync(symbolDir, { recursive: true });

  writeFileSync(
    join(rawDir, 'source.excerpt.txt'),
    metric.excerpts.before
  );
  writeJson(join(rawDir, 'metadata.json'), {
    source: metric.source,
    sourceBytes: metric.sourceBytes,
    sha256: metric.sha256,
    excerptBytes: byteLength(metric.excerpts.before),
    truncated: metric.excerpts.before.includes('... [truncated '),
  });
  writeFileSync(
    join(rawDir, 'README.md'),
    `# Raw Input

Source sample: \`${metric.source}\`

This folder stores an excerpt and metadata for the original real-code sample.
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
    writeFileSync(join(minifiedDir, artifact.fileName), artifact.content);
  }

  writeJson(join(minifiedDir, 'metrics.json'), minifiedArtifacts);
  writeFileSync(
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

  writeFileSync(join(symbolDir, 'signatures.txt'), metric.excerpts.symbols);
  writeJson(join(symbolDir, 'metrics.json'), metric.symbols);
  writeFileSync(
    join(symbolDir, 'README.md'),
    `# Symbol Output

Generated by \`extractSignatures\`.

Supported for this extension: \`${metric.symbols.supported}\`

Returned symbols for this sample: \`${metric.symbols.returned}\`
`
  );
}

function rootSummary(metrics, corpusRoot, missingExtensions) {
  const quality = qualitySummary(metrics);
  const typeDistribution = asyncTypeDistribution(metrics);
  const commonTypeRows = commonTypeMetrics(metrics).map(metric => {
    const symbolCut =
      metric.symbols.cutPercent === null ? 'n/a' : `${metric.symbols.cutPercent}%`;
    return `| \`${metric.ext}\` | ${metric.language} | \`${metric.strategy}\` | \`${metric.async.type}\` | ${metric.contentView.cutPercent}% | ${metric.applyMinification.cutPercent}% | ${metric.async.cutPercent}% | ${symbolCut} | \`${metric.source}\` |`;
  });
  const typeDistributionText = Object.entries(typeDistribution)
    .map(([type, info]) => `${type} ${info.count}`)
    .join(', ');
  const rows = metrics.map(metric => {
    const symbolCut =
      metric.symbols.cutPercent === null ? 'n/a' : `${metric.symbols.cutPercent}%`;
    return `| \`${metric.ext}\` | ${metric.language} | \`${metric.ext}/README.md\` | ${metric.sourceBytes} | ${metric.contentView.cutPercent}% | ${metric.applyMinification.cutPercent}% | ${metric.sync.cutPercent}% | ${metric.async.cutPercent}% | ${symbolCut} | ${metric.ratings.agent}/10 ${metric.ratings.label} |`;
  });

  return `# Real-Code Minifier Benchmark

Generated from local corpus: \`${corpusRoot}\`

This directory records before/after excerpts and metrics for one real sample per
discovered extension. Full third-party source files are not vendored here; use
the generator to recreate reports from a local corpus.

## Summary

- Agent-context minifier rating: **${quality.averageAgent}/10**
- Minify rating: **${quality.averageMinify}/10**
- Symbols rating where supported: **${quality.averageSymbolsWhenSupported}/10** (${quality.symbolsReturned}/${quality.symbolsSupported} returned)
- Average cuts: content-view ${quality.averageContentViewCut}%, apply ${quality.averageApplyCut}%, async ${quality.averageAsyncCut}%
- Rating buckets: ${Object.entries(quality.buckets)
    .map(([label, count]) => `${label} ${count}`)
    .join(', ')}

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

| Ext | Format | Configured strategy | Async type | Content-view cut | Apply cut | Async cut | Symbols cut | Source |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- |
${commonTypeRows.join('\n')}

| Ext | Format | Report | Input bytes | Content-view cut | Apply cut | Sync cut | Async cut | Symbols cut | Agent rating |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
${rows.join('\n')}

## Coverage

- Configured extensions: ${Object.keys(MINIFY_CONFIG.fileTypes).length}
- Real corpus extensions covered here: ${metrics.length}
- Configured extensions missing from this corpus: ${missingExtensions.length}

See \`missing-real-samples.md\` for formats that are
supported by the package but not present in this local corpus.

## Weakest Measured Formats

${quality.weakest
  .map(
    metric =>
      `- \`.${metric.ext}\` ${metric.language}: ${metric.agent}/10 ${metric.label}; content-view cut ${metric.contentViewCut}%, async cut ${metric.asyncCut}%, symbols ${metric.symbolsReturned ? 'returned' : 'not returned'}.`
  )
  .join('\n')}

## Regenerate

\`\`\`bash
yarn build
node benchmark/generate-real-code-report.mjs /path/to/real/corpus
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
  const corpusRoot = resolve(process.argv[2] ?? DEFAULT_CORPUS_ROOT);
  const outputRoot = resolve(new URL('.', import.meta.url).pathname);

  if (!existsSync(corpusRoot)) {
    throw new Error(`Corpus root does not exist: ${corpusRoot}`);
  }

  const samples = collectSamples(corpusRoot);
  if (samples.length === 0) {
    throw new Error(`No supported samples found in: ${corpusRoot}`);
  }

  for (const entry of readdirSync(outputRoot)) {
    if (entry === 'generate-real-code-report.mjs') continue;
    rmSync(join(outputRoot, entry), { recursive: true, force: true });
  }

  const metrics = [];
  for (const sample of samples) {
    const metric = await measureSample(sample);
    metrics.push(metric);

    const languageDir = join(outputRoot, metric.ext);
    mkdirSync(languageDir, { recursive: true });
    writeBenchmarkArtifacts(languageDir, metric);
    writeFileSync(join(languageDir, 'README.md'), renderLanguageReport(metric));
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
          ratings: metric.ratings,
          notes: metric.notes,
        })),
      },
      null,
      2
    )}\n`
  );
  writeFileSync(
    join(outputRoot, 'README.md'),
    rootSummary(metrics, corpusRoot, missingExtensions)
  );
  writeFileSync(
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
