export type CommentPatternGroup =
  | 'c-style'
  | 'hash'
  | 'html'
  | 'sql'
  | 'lua'
  | 'template'
  | 'haskell';

export type Strategy =
  | 'terser'
  | 'conservative'
  | 'aggressive'
  | 'json'
  | 'general'
  | 'markdown';

export interface FileTypeMinifyConfig {
  strategy: Strategy;
  comments?: CommentPatternGroup | CommentPatternGroup[];
}

interface MinifyConfig {
  commentPatterns: {
    [key in CommentPatternGroup]: RegExp[];
  };
  fileTypes: {
    [extension: string]: FileTypeMinifyConfig;
  };
}

export interface MinifyResult {
  content: string;
  failed: boolean;
  type: Strategy | 'failed';
  reason?: string;
}

export const MINIFY_CONFIG: MinifyConfig = {
  commentPatterns: {
    'c-style': [
      /\/\*[\s\S]*?\*\//g, // /* block comments */
      /^\s*\/\/.*$/gm, // // line comments at start of line
      /\s+\/\/.*$/gm, // // inline comments with space before
    ],
    hash: [
      /^\s*#(?!!).*$/gm, // # comments (but not shebangs #!)
      /\s+#.*$/gm, // # inline comments
    ],
    html: [
      /<!--[\s\S]*?-->/g, // <!-- HTML comments -->
    ],
    sql: [
      /--.*$/gm, // -- SQL comments
      /\/\*[\s\S]*?\*\//g, // /* SQL block comments */
    ],
    lua: [
      /^\s*--.*$/gm, // -- line comments
      /--\[\[[\s\S]*?\]\]/g, // --[[ block comments ]]
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
  },

  fileTypes: {
    js: { strategy: 'terser' },
    jsx: { strategy: 'terser' },
    mjs: { strategy: 'terser' },
    cjs: { strategy: 'terser' },

    ts: { strategy: 'conservative', comments: 'c-style' },
    tsx: { strategy: 'conservative', comments: 'c-style' },

    py: { strategy: 'conservative', comments: 'hash' },
    yaml: { strategy: 'conservative', comments: 'hash' },
    yml: { strategy: 'conservative', comments: 'hash' },
    coffee: { strategy: 'conservative', comments: 'hash' },
    nim: { strategy: 'conservative', comments: 'hash' },
    haml: { strategy: 'conservative', comments: 'hash' },
    slim: { strategy: 'conservative', comments: 'hash' },
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

    go: { strategy: 'conservative', comments: 'c-style' },
    java: { strategy: 'conservative', comments: 'c-style' },
    c: { strategy: 'conservative', comments: 'c-style' },
    cpp: { strategy: 'conservative', comments: 'c-style' },
    cs: { strategy: 'conservative', comments: 'c-style' },
    rust: { strategy: 'conservative', comments: 'c-style' },
    rs: { strategy: 'conservative', comments: 'c-style' },
    swift: { strategy: 'conservative', comments: 'c-style' },
    kotlin: { strategy: 'conservative', comments: 'c-style' },
    scala: { strategy: 'conservative', comments: 'c-style' },
    dart: { strategy: 'conservative', comments: 'c-style' },

    php: { strategy: 'conservative', comments: ['c-style', 'hash'] },
    rb: { strategy: 'conservative', comments: 'hash' },
    perl: { strategy: 'conservative', comments: 'hash' },
    sh: { strategy: 'conservative', comments: 'hash' },
    bash: { strategy: 'conservative', comments: 'hash' },

    sql: { strategy: 'aggressive', comments: 'sql' },

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

    graphql: { strategy: 'aggressive', comments: 'hash' },
    gql: { strategy: 'aggressive', comments: 'hash' },
    proto: { strategy: 'aggressive', comments: 'c-style' },
    csv: { strategy: 'conservative' },
    toml: { strategy: 'aggressive', comments: 'hash' },
    ini: { strategy: 'aggressive', comments: 'hash' },
    conf: { strategy: 'aggressive', comments: 'hash' },
    config: { strategy: 'aggressive', comments: 'hash' },
    env: { strategy: 'aggressive', comments: 'hash' },
    properties: { strategy: 'aggressive', comments: 'hash' },

    tf: { strategy: 'aggressive', comments: ['hash', 'c-style'] },
    tfvars: { strategy: 'aggressive', comments: ['hash', 'c-style'] },
    pp: { strategy: 'aggressive', comments: 'hash' },

    md: { strategy: 'markdown' },
    markdown: { strategy: 'markdown' },
    rst: { strategy: 'conservative', comments: 'hash' },

    star: { strategy: 'conservative', comments: 'hash' },
    bzl: { strategy: 'conservative', comments: 'hash' },
    cmake: { strategy: 'conservative', comments: 'hash' },

    pl: { strategy: 'aggressive', comments: 'hash' },
    pm: { strategy: 'aggressive', comments: 'hash' },
    fs: { strategy: 'conservative', comments: 'c-style' },
    fsx: { strategy: 'conservative', comments: 'c-style' },
    hs: { strategy: 'conservative', comments: 'haskell' },
    lhs: { strategy: 'conservative', comments: 'haskell' },
    elm: { strategy: 'conservative', comments: 'c-style' },
    clj: { strategy: 'aggressive', comments: 'hash' },
    cljs: { strategy: 'aggressive', comments: 'hash' },
    ex: { strategy: 'aggressive', comments: 'hash' },
    exs: { strategy: 'aggressive', comments: 'hash' },
    erl: { strategy: 'aggressive', comments: 'hash' },
    hrl: { strategy: 'aggressive', comments: 'hash' },

    txt: { strategy: 'general' },
    log: { strategy: 'general' },
    cfg: { strategy: 'aggressive', comments: 'hash' },
    gitignore: { strategy: 'aggressive', comments: 'hash' },
    dockerignore: { strategy: 'aggressive', comments: 'hash' },
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
