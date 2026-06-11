export type CommentPatternGroup =
  | 'c-style'
  | 'hash'
  | 'html'
  | 'sql'
  | 'lua'
  | 'template'
  | 'haskell'
  | 'semicolon'
  | 'wasm-text'
  | 'percent'
  | 'haml'
  | 'slim'
  | 'powershell'
  | 'bang'
  | 'apostrophe'
  | 'double-dash'
  | 'fsharp-block'
  | 'pascal';

export type Strategy =
  | 'terser'
  | 'conservative'
  | 'aggressive'
  | 'json'
  | 'general'
  | 'markdown';

export type MinificationMode = 'content-view' | 'minify' | 'symbols';

export type MinificationModeInfo = {
  readonly mode: MinificationMode;
  readonly purpose: string;
  readonly outputContract: string;
};

export type FileTypeMinifyConfig = {
  strategy: Strategy;
  comments?: CommentPatternGroup | CommentPatternGroup[];
};

type MinifyConfig = {
  commentPatterns: {
    [key in CommentPatternGroup]: RegExp[];
  };
  fileTypes: {
    [extension: string]: FileTypeMinifyConfig;
  };
};

export type MinifyResult = {
  content: string;
  failed: boolean;
  type: Strategy | 'failed';
  reason?: string;
};

export const MINIFICATION_MODES = {
  contentView: {
    mode: 'content-view',
    purpose: 'Readable agent context with source-like structure preserved',
    outputContract: 'Best-effort token reduction; not guaranteed executable',
  },
  minify: {
    mode: 'minify',
    purpose: 'Stronger parser-backed or strategy-backed reduction',
    outputContract:
      'Must not grow content in guarded paths; parser output when available',
  },
  symbols: {
    mode: 'symbols',
    purpose: 'Whole-file skeleton with original line gutters',
    outputContract: 'Navigation summary only; not executable source',
  },
} as const satisfies Record<string, MinificationModeInfo>;

export const MINIFY_CONFIG: MinifyConfig = {
  commentPatterns: {
    'c-style': [
      /\/\*[\s\S]*?\*\//g, // /* block comments */
      /^\s*\/\/.*$/gm, // // line comments at start of line
      /\s+\/\/.*$/gm, // // inline comments with space before
    ],
    hash: [
      /^\s*#(?!!).*$/gm, // # comments (but not shebangs #!)
      /\s+#(?!!).*$/gm, // # inline comments (but not shebangs, e.g. skeleton gutters)
    ],
    html: [
      /<!--[\s\S]*?-->/g, // <!-- HTML comments -->
    ],
    sql: [
      /--.*$/gm, // -- SQL comments
      /\/\*[\s\S]*?\*\//g, // /* SQL block comments */
    ],
    lua: [
      /--\[\[[\s\S]*?\]\]/g, // --[[ block comments ]]
      /^\s*--(?!\[\[).*$/gm, // -- line comments (but not block starts)
      /\s+--(?!\[\[).*$/gm, // -- inline comments
    ],
    template: [
      /\{\{!--[\s\S]*?--\}\}/g, // {{!-- Handlebars --}}
      /\{\{![\s\S]*?\}\}/g, // {{! Handlebars }}
      /<%#[\s\S]*?%>/g, // <%# EJS %>
      /\{#[\s\S]*?#\}/g, // {# Twig/Jinja #}
    ],
    haskell: [
      /^\s*--.*$/gm, // -- line comments
      /\s+--.*$/gm, // -- inline comments
      /\{-[\s\S]*?-\}/g, // {- block comments -}
    ],
    semicolon: [
      /^\s*;.*$/gm, // ; line comments (INI, Clojure)
      /\s+;.*$/gm, // ; inline comments
    ],
    'wasm-text': [
      /\(;[\s\S]*?;\)/g, // (; WebAssembly text block comments ;)
      /^\s*;;.*$/gm, // ;; WebAssembly text line comments
      /\s+;;.*$/gm, // ;; inline comments
    ],
    percent: [
      /^\s*%.*$/gm, // % line comments (Erlang)
      /\s+%.*$/gm, // % inline comments
    ],
    haml: [
      /^\s*-#.*$/gm, // -# HAML silent comments
    ],
    slim: [
      /^\s*\/.*$/gm, // / Slim comments
    ],
    powershell: [
      /<#[\s\S]*?#>/g, // <# PowerShell block comments #>
      /^\s*#(?!!).*$/gm, // # comments (but not shebangs)
      /\s+#(?!!).*$/gm, // # inline comments
    ],
    bang: [
      /^\s*!.*$/gm, // ! line comments (Fortran)
      /\s+!.*$/gm, // ! inline comments
    ],
    apostrophe: [
      /^\s*'.*$/gm, // ' line comments (Visual Basic)
      /\s+'.*$/gm, // ' inline comments
    ],
    'double-dash': [
      /^\s*--.*$/gm, // -- line comments (Ada, VHDL)
      /\s+--.*$/gm, // -- inline comments
    ],
    'fsharp-block': [
      /\(\*[\s\S]*?\*\)/g, // (* F# block comments *)
    ],
    pascal: [
      /\(\*[\s\S]*?\*\)/g, // (* Pascal block comments *)
      /\{[\s\S]*?\}/g, // { Pascal block comments }
      /^\s*\/\/.*$/gm, // // Delphi line comments
      /\s+\/\/.*$/gm, // // inline comments
    ],
  },

  fileTypes: {
    js: { strategy: 'terser', comments: 'c-style' },
    jsx: { strategy: 'terser', comments: 'c-style' },
    mjs: { strategy: 'terser', comments: 'c-style' },
    cjs: { strategy: 'terser', comments: 'c-style' },

    ts: { strategy: 'conservative', comments: 'c-style' },
    tsx: { strategy: 'conservative', comments: 'c-style' },

    py: { strategy: 'conservative', comments: 'hash' },
    yaml: { strategy: 'conservative', comments: 'hash' },
    yml: { strategy: 'conservative', comments: 'hash' },
    coffee: { strategy: 'conservative', comments: 'hash' },
    nim: { strategy: 'conservative', comments: 'hash' },
    haml: { strategy: 'conservative', comments: ['hash', 'haml'] },
    slim: { strategy: 'conservative', comments: ['hash', 'slim'] },
    sass: { strategy: 'conservative', comments: 'c-style' },
    styl: { strategy: 'conservative', comments: 'c-style' },

    html: { strategy: 'aggressive', comments: 'html' },
    htm: { strategy: 'aggressive', comments: 'html' },
    xml: { strategy: 'aggressive', comments: 'html' },
    svg: { strategy: 'aggressive', comments: 'html' },

    css: { strategy: 'aggressive', comments: 'c-style' },
    less: { strategy: 'aggressive', comments: 'c-style' },
    scss: { strategy: 'aggressive', comments: 'c-style' },

    json: { strategy: 'json' },
    jsonc: { strategy: 'json' },
    json5: { strategy: 'json' },

    go: { strategy: 'conservative', comments: 'c-style' },
    java: { strategy: 'conservative', comments: 'c-style' },
    c: { strategy: 'conservative', comments: 'c-style' },
    h: { strategy: 'conservative', comments: 'c-style' },
    cpp: { strategy: 'conservative', comments: 'c-style' },
    hpp: { strategy: 'conservative', comments: 'c-style' },
    cc: { strategy: 'conservative', comments: 'c-style' },
    cs: { strategy: 'conservative', comments: 'c-style' },
    vb: { strategy: 'conservative', comments: 'apostrophe' },
    vbs: { strategy: 'conservative', comments: 'apostrophe' },
    rust: { strategy: 'conservative', comments: 'c-style' },
    rs: { strategy: 'conservative', comments: 'c-style' },
    swift: { strategy: 'conservative', comments: 'c-style' },
    kt: { strategy: 'conservative', comments: 'c-style' },
    kotlin: { strategy: 'conservative', comments: 'c-style' },
    scala: { strategy: 'conservative', comments: 'c-style' },
    dart: { strategy: 'conservative', comments: 'c-style' },
    groovy: { strategy: 'conservative', comments: 'c-style' },
    gradle: { strategy: 'conservative', comments: 'c-style' },
    mm: { strategy: 'conservative', comments: 'c-style' },
    pas: { strategy: 'conservative', comments: 'pascal' },
    adb: { strategy: 'conservative', comments: 'double-dash' },
    ads: { strategy: 'conservative', comments: 'double-dash' },
    f: { strategy: 'conservative', comments: 'bang' },
    for: { strategy: 'conservative', comments: 'bang' },
    f90: { strategy: 'conservative', comments: 'bang' },
    f95: { strategy: 'conservative', comments: 'bang' },
    f03: { strategy: 'conservative', comments: 'bang' },
    f08: { strategy: 'conservative', comments: 'bang' },
    zig: { strategy: 'conservative', comments: 'c-style' },
    v: { strategy: 'conservative', comments: 'c-style' },
    jl: { strategy: 'conservative', comments: 'hash' },
    nix: { strategy: 'conservative', comments: ['hash', 'c-style'] },

    php: { strategy: 'conservative', comments: ['c-style', 'hash'] },
    rb: { strategy: 'conservative', comments: 'hash' },
    perl: { strategy: 'conservative', comments: 'hash' },
    sh: { strategy: 'conservative', comments: 'hash' },
    bash: { strategy: 'conservative', comments: 'hash' },
    zsh: { strategy: 'conservative', comments: 'hash' },
    fish: { strategy: 'conservative', comments: 'hash' },
    ps1: { strategy: 'conservative', comments: 'powershell' },
    psm1: { strategy: 'conservative', comments: 'powershell' },
    psd1: { strategy: 'conservative', comments: 'powershell' },

    sql: { strategy: 'conservative', comments: 'sql' },
    tsql: { strategy: 'conservative', comments: 'sql' },
    plsql: { strategy: 'conservative', comments: 'sql' },
    pls: { strategy: 'conservative', comments: 'sql' },
    pks: { strategy: 'conservative', comments: 'sql' },
    pkb: { strategy: 'conservative', comments: 'sql' },

    lua: { strategy: 'aggressive', comments: 'lua' },
    r: { strategy: 'aggressive', comments: 'hash' },

    hbs: { strategy: 'aggressive', comments: 'template' },
    handlebars: { strategy: 'aggressive', comments: 'template' },
    ejs: { strategy: 'aggressive', comments: 'template' },
    pug: { strategy: 'conservative', comments: 'c-style' },
    jade: { strategy: 'conservative', comments: 'c-style' },
    mustache: { strategy: 'aggressive', comments: 'template' },
    twig: { strategy: 'aggressive', comments: 'template' },
    jinja: { strategy: 'aggressive', comments: 'template' },
    jinja2: { strategy: 'aggressive', comments: 'template' },
    erb: { strategy: 'aggressive', comments: 'template' },

    vue: { strategy: 'aggressive', comments: 'html' },
    svelte: { strategy: 'aggressive', comments: 'html' },
    xsl: { strategy: 'aggressive', comments: 'html' },
    xslt: { strategy: 'aggressive', comments: 'html' },

    graphql: { strategy: 'conservative', comments: 'hash' },
    gql: { strategy: 'conservative', comments: 'hash' },
    proto: { strategy: 'conservative', comments: 'c-style' },
    csv: { strategy: 'conservative' },
    toml: { strategy: 'conservative', comments: 'hash' },
    ini: { strategy: 'conservative', comments: ['hash', 'semicolon'] },
    conf: { strategy: 'conservative', comments: 'hash' },
    config: { strategy: 'conservative', comments: 'hash' },
    env: { strategy: 'conservative', comments: 'hash' },
    properties: { strategy: 'conservative', comments: 'hash' },

    tf: { strategy: 'conservative', comments: ['hash', 'c-style'] },
    tfvars: { strategy: 'conservative', comments: ['hash', 'c-style'] },
    pp: { strategy: 'conservative', comments: 'hash' },

    md: { strategy: 'markdown' },
    markdown: { strategy: 'markdown' },
    rst: { strategy: 'conservative', comments: 'hash' },

    star: { strategy: 'conservative', comments: 'hash' },
    bzl: { strategy: 'conservative', comments: 'hash' },
    cmake: { strategy: 'conservative', comments: 'hash' },
    awk: { strategy: 'conservative', comments: 'hash' },

    pl: { strategy: 'aggressive', comments: 'hash' },
    pm: { strategy: 'aggressive', comments: 'hash' },
    fs: { strategy: 'conservative', comments: ['c-style', 'fsharp-block'] },
    fsx: { strategy: 'conservative', comments: ['c-style', 'fsharp-block'] },
    hs: { strategy: 'conservative', comments: 'haskell' },
    lhs: { strategy: 'conservative', comments: 'haskell' },
    elm: { strategy: 'conservative', comments: 'c-style' },
    lisp: { strategy: 'conservative', comments: 'semicolon' },
    lsp: { strategy: 'conservative', comments: 'semicolon' },
    scm: { strategy: 'conservative', comments: 'semicolon' },
    rkt: { strategy: 'conservative', comments: 'semicolon' },
    clj: { strategy: 'aggressive', comments: 'semicolon' },
    cljs: { strategy: 'aggressive', comments: 'semicolon' },
    ex: { strategy: 'aggressive', comments: 'hash' },
    exs: { strategy: 'aggressive', comments: 'hash' },
    erl: { strategy: 'aggressive', comments: 'percent' },
    hrl: { strategy: 'aggressive', comments: 'percent' },
    vhd: { strategy: 'conservative', comments: 'double-dash' },
    vhdl: { strategy: 'conservative', comments: 'double-dash' },
    asm: { strategy: 'conservative', comments: 'semicolon' },
    nasm: { strategy: 'conservative', comments: 'semicolon' },
    wat: { strategy: 'conservative', comments: 'wasm-text' },
    wast: { strategy: 'conservative', comments: 'wasm-text' },

    txt: { strategy: 'general' },
    log: { strategy: 'general' },
    cfg: { strategy: 'conservative', comments: 'hash' },
    gitignore: { strategy: 'conservative', comments: 'hash' },
    dockerignore: { strategy: 'conservative', comments: 'hash' },
  },
};

export const INDENTATION_SENSITIVE_NAMES = new Set([
  'makefile',
  'dockerfile',
  'procfile',
  'justfile',
  'rakefile',
  'gemfile',
  'podfile',
  'fastfile',
  'vagrantfile',
  'jenkinsfile',
  'cakefile',
  'pipfile',
  'buildfile',
  'capfile',
  'brewfile',
]);
