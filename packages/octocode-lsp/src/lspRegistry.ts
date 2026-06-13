import type { LanguageServerCommand } from './types.js';

export const LANGUAGE_SERVER_COMMANDS: Record<string, LanguageServerCommand> = {
  '.ts': {
    command: 'typescript-language-server',
    args: ['--stdio'],
    languageId: 'typescript',
    envVar: 'OCTOCODE_TS_SERVER_PATH',
  },
  '.tsx': {
    command: 'typescript-language-server',
    args: ['--stdio'],
    languageId: 'typescriptreact',
    envVar: 'OCTOCODE_TS_SERVER_PATH',
  },
  '.js': {
    command: 'typescript-language-server',
    args: ['--stdio'],
    languageId: 'javascript',
    envVar: 'OCTOCODE_TS_SERVER_PATH',
  },
  '.jsx': {
    command: 'typescript-language-server',
    args: ['--stdio'],
    languageId: 'javascriptreact',
    envVar: 'OCTOCODE_TS_SERVER_PATH',
  },
  '.mjs': {
    command: 'typescript-language-server',
    args: ['--stdio'],
    languageId: 'javascript',
    envVar: 'OCTOCODE_TS_SERVER_PATH',
  },
  '.cjs': {
    command: 'typescript-language-server',
    args: ['--stdio'],
    languageId: 'javascript',
    envVar: 'OCTOCODE_TS_SERVER_PATH',
  },

  '.py': {
    command: 'pylsp',
    args: [],
    languageId: 'python',
    envVar: 'OCTOCODE_PYTHON_SERVER_PATH',
  },
  '.pyi': {
    command: 'pylsp',
    args: [],
    languageId: 'python',
    envVar: 'OCTOCODE_PYTHON_SERVER_PATH',
  },

  '.go': {
    command: 'gopls',
    args: ['serve'],
    languageId: 'go',
    envVar: 'OCTOCODE_GO_SERVER_PATH',
  },

  '.rs': {
    command: 'rust-analyzer',
    args: [],
    languageId: 'rust',
    envVar: 'OCTOCODE_RUST_SERVER_PATH',
  },

  '.java': {
    command: 'jdtls',
    args: [],
    languageId: 'java',
    envVar: 'OCTOCODE_JAVA_SERVER_PATH',
  },

  '.kt': {
    command: 'kotlin-language-server',
    args: [],
    languageId: 'kotlin',
    envVar: 'OCTOCODE_KOTLIN_SERVER_PATH',
  },
  '.kts': {
    command: 'kotlin-language-server',
    args: [],
    languageId: 'kotlin',
    envVar: 'OCTOCODE_KOTLIN_SERVER_PATH',
  },

  '.c': {
    command: 'clangd',
    args: [],
    languageId: 'c',
    envVar: 'OCTOCODE_CLANGD_SERVER_PATH',
  },
  '.h': {
    command: 'clangd',
    args: [],
    languageId: 'c',
    envVar: 'OCTOCODE_CLANGD_SERVER_PATH',
  },
  '.cpp': {
    command: 'clangd',
    args: [],
    languageId: 'cpp',
    envVar: 'OCTOCODE_CLANGD_SERVER_PATH',
  },
  '.hpp': {
    command: 'clangd',
    args: [],
    languageId: 'cpp',
    envVar: 'OCTOCODE_CLANGD_SERVER_PATH',
  },
  '.cc': {
    command: 'clangd',
    args: [],
    languageId: 'cpp',
    envVar: 'OCTOCODE_CLANGD_SERVER_PATH',
  },
  '.cxx': {
    command: 'clangd',
    args: [],
    languageId: 'cpp',
    envVar: 'OCTOCODE_CLANGD_SERVER_PATH',
  },

  '.cs': {
    command: 'csharp-ls',
    args: [],
    languageId: 'csharp',
    envVar: 'OCTOCODE_CSHARP_SERVER_PATH',
  },

  '.rb': {
    command: 'solargraph',
    args: ['stdio'],
    languageId: 'ruby',
    envVar: 'OCTOCODE_RUBY_SERVER_PATH',
  },

  '.php': {
    command: 'intelephense',
    args: ['--stdio'],
    languageId: 'php',
    envVar: 'OCTOCODE_PHP_SERVER_PATH',
  },

  '.swift': {
    command: 'sourcekit-lsp',
    args: [],
    languageId: 'swift',
    envVar: 'OCTOCODE_SWIFT_SERVER_PATH',
  },

  '.dart': {
    command: 'dart',
    args: ['language-server', '--client-id=octocode'],
    languageId: 'dart',
    envVar: 'OCTOCODE_DART_SERVER_PATH',
  },

  '.lua': {
    command: 'lua-language-server',
    args: [],
    languageId: 'lua',
    envVar: 'OCTOCODE_LUA_SERVER_PATH',
  },

  '.zig': {
    command: 'zls',
    args: [],
    languageId: 'zig',
    envVar: 'OCTOCODE_ZIG_SERVER_PATH',
  },

  '.ex': {
    command: 'elixir-ls',
    args: [],
    languageId: 'elixir',
    envVar: 'OCTOCODE_ELIXIR_SERVER_PATH',
  },
  '.exs': {
    command: 'elixir-ls',
    args: [],
    languageId: 'elixir',
    envVar: 'OCTOCODE_ELIXIR_SERVER_PATH',
  },

  '.scala': {
    command: 'metals',
    args: [],
    languageId: 'scala',
    envVar: 'OCTOCODE_SCALA_SERVER_PATH',
  },
  '.sc': {
    command: 'metals',
    args: [],
    languageId: 'scala',
    envVar: 'OCTOCODE_SCALA_SERVER_PATH',
  },

  '.hs': {
    command: 'haskell-language-server-wrapper',
    args: ['--lsp'],
    languageId: 'haskell',
    envVar: 'OCTOCODE_HASKELL_SERVER_PATH',
  },

  '.ml': {
    command: 'ocamllsp',
    args: [],
    languageId: 'ocaml',
    envVar: 'OCTOCODE_OCAML_SERVER_PATH',
  },
  '.mli': {
    command: 'ocamllsp',
    args: [],
    languageId: 'ocaml',
    envVar: 'OCTOCODE_OCAML_SERVER_PATH',
  },

  '.clj': {
    command: 'clojure-lsp',
    args: [],
    languageId: 'clojure',
    envVar: 'OCTOCODE_CLOJURE_SERVER_PATH',
  },
  '.cljs': {
    command: 'clojure-lsp',
    args: [],
    languageId: 'clojure',
    envVar: 'OCTOCODE_CLOJURE_SERVER_PATH',
  },
  '.cljc': {
    command: 'clojure-lsp',
    args: [],
    languageId: 'clojure',
    envVar: 'OCTOCODE_CLOJURE_SERVER_PATH',
  },

  '.vue': {
    command: 'vue-language-server',
    args: ['--stdio'],
    languageId: 'vue',
    envVar: 'OCTOCODE_VUE_SERVER_PATH',
  },

  '.svelte': {
    command: 'svelteserver',
    args: ['--stdio'],
    languageId: 'svelte',
    envVar: 'OCTOCODE_SVELTE_SERVER_PATH',
  },

  '.yaml': {
    command: 'yaml-language-server',
    args: ['--stdio'],
    languageId: 'yaml',
    envVar: 'OCTOCODE_YAML_SERVER_PATH',
  },
  '.yml': {
    command: 'yaml-language-server',
    args: ['--stdio'],
    languageId: 'yaml',
    envVar: 'OCTOCODE_YAML_SERVER_PATH',
  },

  '.toml': {
    command: 'taplo',
    args: ['lsp', 'stdio'],
    languageId: 'toml',
    envVar: 'OCTOCODE_TOML_SERVER_PATH',
  },

  '.json': {
    command: 'vscode-json-language-server',
    args: ['--stdio'],
    languageId: 'json',
    envVar: 'OCTOCODE_JSON_SERVER_PATH',
  },
  '.jsonc': {
    command: 'vscode-json-language-server',
    args: ['--stdio'],
    languageId: 'jsonc',
    envVar: 'OCTOCODE_JSON_SERVER_PATH',
  },

  '.html': {
    command: 'vscode-html-language-server',
    args: ['--stdio'],
    languageId: 'html',
    envVar: 'OCTOCODE_HTML_SERVER_PATH',
  },
  '.css': {
    command: 'vscode-css-language-server',
    args: ['--stdio'],
    languageId: 'css',
    envVar: 'OCTOCODE_CSS_SERVER_PATH',
  },
  '.scss': {
    command: 'vscode-css-language-server',
    args: ['--stdio'],
    languageId: 'scss',
    envVar: 'OCTOCODE_CSS_SERVER_PATH',
  },
  '.less': {
    command: 'vscode-css-language-server',
    args: ['--stdio'],
    languageId: 'less',
    envVar: 'OCTOCODE_CSS_SERVER_PATH',
  },

  '.sh': {
    command: 'bash-language-server',
    args: ['start'],
    languageId: 'shellscript',
    envVar: 'OCTOCODE_BASH_SERVER_PATH',
  },
  '.bash': {
    command: 'bash-language-server',
    args: ['start'],
    languageId: 'shellscript',
    envVar: 'OCTOCODE_BASH_SERVER_PATH',
  },
  '.zsh': {
    command: 'bash-language-server',
    args: ['start'],
    languageId: 'shellscript',
    envVar: 'OCTOCODE_BASH_SERVER_PATH',
  },

  '.sql': {
    command: 'sql-language-server',
    args: ['up', '--method', 'stdio'],
    languageId: 'sql',
    envVar: 'OCTOCODE_SQL_SERVER_PATH',
  },

  '.graphql': {
    command: 'graphql-lsp',
    args: ['server', '-m', 'stream'],
    languageId: 'graphql',
    envVar: 'OCTOCODE_GRAPHQL_SERVER_PATH',
  },
  '.gql': {
    command: 'graphql-lsp',
    args: ['server', '-m', 'stream'],
    languageId: 'graphql',
    envVar: 'OCTOCODE_GRAPHQL_SERVER_PATH',
  },

  '.tf': {
    command: 'terraform-ls',
    args: ['serve'],
    languageId: 'terraform',
    envVar: 'OCTOCODE_TERRAFORM_SERVER_PATH',
  },
};
