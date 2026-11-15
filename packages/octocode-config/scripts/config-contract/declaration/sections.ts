// Every setting once: type, default, description, environment binding and dotenv trust.
// Section path "" is the root of .octocoderc; dotted paths nest. Field order is document order.
import type { ContractInput } from '../schema.ts';

export const sections: ContractInput['sections'] = {
  '': { title: 'Configuration', fields: {} },
  github: {
    title: 'GitHub API',
    typeName: 'GitHub',
    fields: {
      apiUrl: {
        type: 'url',
        default: 'https://api.github.com',
        description: 'GitHub REST API root. GitHub Enterprise commonly uses /api/v3.',
        env: { name: 'GITHUB_API_URL', dotenv: 'home', normalize: 'trim' },
      },
    },
  },
  local: {
    title: 'Local tools',
    fields: {
      enabled: {
        type: 'boolean',
        default: true,
        description: 'Expose local filesystem tools through MCP. CLI tools remain available; path and content security apply on both surfaces.',
        env: { name: 'OCTOCODE_ENABLE_LOCAL', dotenv: 'home', workspaceNarrowsTo: 'false' },
      },
      beta: {
        type: 'boolean',
        default: false,
        description: 'Expose graphBuild and graphQuery through MCP. CLI tools remain available without this opt-in. Restart MCP after changing this setting.',
        env: { name: 'OCTOCODE_BETA', dotenv: 'home' },
      },
      allowedPaths: {
        type: 'stringArray',
        default: [],
        itemFormat: 'path',
        description: 'Extra absolute or home-relative roots added to the allowed roots (workspace root and OCTOCODE_HOME).',
        env: { name: 'ALLOWED_PATHS', dotenv: 'home' },
        notes: 'The environment value is a comma-separated list.',
      },
      workspaceRoot: {
        type: 'path',
        default: null,
        displayDefault: 'the working directory',
        description: 'Optional absolute or home-relative workspace root.',
        env: { name: 'WORKSPACE_ROOT', dotenv: 'home', normalize: 'trim' },
      },
      graphHome: {
        type: 'path',
        default: null,
        displayDefault: '<workspace>/.octocode/graph',
        description: 'Directory in which graphBuild keeps its snapshots, in place of the workspace\'s .octocode/graph, shared by every workspace: a build then writes nothing in the repository and may scan any allowed directory, such as a ghCloneRepo checkout. Octocode creates the directory if needed and writes only in its own octocode-graph folder there, never reading or removing anything else; the setting grants local tools no read access to it. A build of a path that contains it is refused.',
        notes: 'Home-only, so a repository cannot choose where Octocode writes.',
        env: { name: 'OCTOCODE_GRAPH_HOME', dotenv: 'home', normalize: 'trim' },
      },
    },
  },
  tools: {
    title: 'Tool availability',
    fields: {
      enabled: {
        type: 'stringArray',
        default: null,
        displayDefault: 'unset (every tool)',
        description: 'Strict MCP tool allowlist; other MCP prerequisites still apply. The CLI ignores this list.',
        env: { name: 'TOOLS_TO_RUN' },
      },
      disabled: {
        type: 'stringArray',
        default: null,
        displayDefault: 'unset (none)',
        description: 'Tools removed from MCP discovery and admission. The CLI ignores this list.',
        env: { name: 'DISABLE_TOOLS' },
      },
      enableClone: {
        type: 'boolean',
        default: false,
        description: 'Expose ghCloneRepo through MCP. CLI remains available. Clone execution requires persistent storage on both surfaces. Restart MCP after changing this setting.',
        env: { name: 'OCTOCODE_ENABLE_CLONE', dotenv: 'home', workspaceNarrowsTo: 'false' },
      },
      enableSecurityCheck: {
        type: 'boolean',
        default: false,
        description: 'Expose securityCheck through MCP: it asks OSV (api.osv.dev, operated by Google) about packages, sending each public package name and version it checks. Off, MCP leaves it out and sends nothing. The CLI always offers it, and a CLI call sends the package name to OSV only when you run it. Restart MCP after changing this setting.',
        notes: 'Home-only, so a repository can turn it off for MCP but cannot turn it on.',
        env: { name: 'OCTOCODE_SECURITY_CHECKS', dotenv: 'home', workspaceNarrowsTo: 'false' },
      },
    },
  },
  network: {
    title: 'Network',
    fields: {
      allowPrivateRegistry: {
        type: 'boolean',
        default: false,
        description: 'Allow private, loopback, or link-local artifact registries.',
        env: { name: 'OCTOCODE_ALLOW_PRIVATE_REGISTRY', dotenv: 'home' },
        notes: 'Security-sensitive SSRF escape hatch; opt in only for trusted enterprise registries.',
      },
    },
  },
  lsp: {
    title: 'LSP',
    fields: {
      autoInstall: {
        type: 'enum',
        values: ['prompt', 'off', 'auto'],
        description: 'Whether `octocode lsp-server install` may download a managed language server: prompt (ask), off, or auto.',
        env: { name: 'OCTOCODE_LSP_AUTO_INSTALL', dotenv: 'home', normalize: 'lower' },
      },
      trustProjectConfig: {
        type: 'boolean',
        default: false,
        description: 'Trust a project .octocode/lsp-servers.json. Security-sensitive: a trusted project config can launch server binaries.',
        env: { name: 'OCTOCODE_TRUST_PROJECT_LSP_CONFIG', dotenv: 'home' },
      },
      trustFetchedCode: {
        type: 'boolean',
        default: false,
        description: 'Let lspSearch start language servers on code Octocode fetched: ghCloneRepo and ghStructure materialize checkouts and URL captures under its home. Off by default, because a language server can run code from the repository it opens; text, AST and graph tools read that code without running anything.',
        notes: 'Security-sensitive: turning it on lets a repository you fetched run code on this machine when lspSearch opens it. Home-only, so a repository cannot turn it on.',
        env: { name: 'OCTOCODE_LSP_TRUST_FETCHED_CODE', dotenv: 'home', workspaceNarrowsTo: 'false' },
      },
      allowBuildScripts: {
        type: 'boolean',
        default: false,
        description: 'Let lspSearch run a Rust workspace\'s Cargo build scripts and procedural macros when a query\'s rustContext sets buildScripts or procMacros. Off by default: they are repository code, so such a query is refused (errorCode policy) and the default rust-analyzer, which runs none, answers every other query.',
        notes: 'Security-sensitive: turning it on lets any lspSearch call, in any client mode, run the build scripts and proc macros of the workspace it names. Home-only, so a repository cannot turn it on.',
        env: { name: 'OCTOCODE_LSP_ALLOW_BUILD_SCRIPTS', dotenv: 'home', workspaceNarrowsTo: 'false' },
      },
      prewarm: {
        type: 'enum',
        values: ['targeted', 'all', 'off'],
        description: 'Language-server prewarm: targeted warms concrete LSP lookup candidates found by tools; all also warms files named by reads or searches and, at MCP initialize, the workspace\'s top three languages; off disables prewarm. Starts index a whole project: targeted avoids unrelated server starts, but a lookup without an earlier candidate can take seconds.',
        env: { name: 'OCTOCODE_LSP_PREWARM', normalize: 'lower' },
      },
    },
  },
  output: {
    title: 'Output',
    fields: {
      redactEmails: {
        type: 'boolean',
        default: false,
        description: 'Mask email addresses such as GitHub commit authors.',
        env: { name: 'OCTOCODE_REDACT_EMAILS', dotenv: 'home', workspaceNarrowsTo: 'true' },
      },
      redactUuid: {
        type: 'boolean',
        default: false,
        description: 'Mask UUIDs (8-4-4-4-12 hexadecimal) in every tool result. Off by default: a UUID is an identifier, not a secret.',
        env: { name: 'OCTOCODE_REDACT_UUID', dotenv: 'home', workspaceNarrowsTo: 'true' },
      },
    },
  },
  security: {
    title: 'Security',
    fields: {
      credentialFiles: {
        type: 'enum',
        typeName: 'CredentialFilesPolicy',
        values: ['withhold', 'allow'],
        description: 'Whether local read and search tools may read the known credential files (.env and .env.*, credentials.json, .netrc, .npmrc, id_rsa*, *.pem and the like): withhold them, or allow them in the workspace and allowed paths so results match ripgrep. Known secret formats stay redacted. Credential directories such as .ssh, .aws and .git, the Octocode home, and known credential files directly in your home or in its dot directories (~/.npmrc, ~/.codex/auth.json) stay withheld; every other file, such as ~/.zshrc or a shell history, follows the allowed roots like any file. Graph snapshots, clasify and classification never read these files, and allow keeps saved local results in memory, so a cursor ends with its process.',
        notes: 'Security-sensitive: redaction recognizes known secret formats only, so a secret in no known format can appear in results. Home-only, so a repository cannot turn it on.',
        env: {
          name: 'OCTOCODE_CREDENTIAL_FILES',
          dotenv: 'home',
          normalize: 'lower',
          workspaceNarrowsTo: 'withhold',
        },
      },
    },
  },
  storage: {
    title: 'Runtime storage',
    fields: {
      mode: {
        type: 'enum',
        typeName: 'StorageMode',
        values: ['persistent', 'memory'],
        description: 'Persistent saves continuation state and provider answers to disk; memory retains them only in the current runtime. Fresh local requests reuse only in-memory results keyed by the content they read; GitHub and registry answers repeat as their origin allows. Explicit graph snapshots are outputs; interrupted extraction checkpoints follow this setting.',
        env: {
          name: 'OCTOCODE_STORAGE_MODE',
          dotenv: 'home',
          normalize: 'lower',
          workspaceNarrowsTo: 'memory',
        },
      },
    },
  },
  classification: {
    title: 'Classification provider',
    fields: {
      api: {
        type: 'string',
        default: null,
        credential: true,
        description: 'Classification provider API key. A nonblank value exposes clasify and read-tool classifiers in MCP and the CLI; without one, a CLI clasify call answers where to get a key. Discovery makes no provider request.',
        env: { name: 'OCTOCODE_CLASSIFICATION_API', dotenv: 'home', normalize: 'trim' },
        displayDefault: 'unset (classification off)',
        group: 'Classification',
        docsUrl: 'https://docs.typesafe.ai/introduction',
        notes: 'Credential values are omitted from resolved configuration and diagnostics. Home-only, so a repository cannot turn classification on or send your evidence under its own provider account.',
      },
      apiHost: {
        type: 'url',
        default: null,
        description: 'Classification provider API root, for a proxy, gateway or compatible provider that serves the same API.',
        env: { name: 'OCTOCODE_CLASSIFICATION_API_HOST', dotenv: 'home', normalize: 'trim' },
        displayDefault: 'https://api.typesafe.ai',
        group: 'Classification',
        notes: 'Calls require HTTPS except on loopback, and an API root with no path. Home-only, so a repository cannot redirect your key.',
      },
      model: {
        type: 'string',
        default: null,
        description: 'Classification model for read-tool classifiers and clasify; a clasify row\'s model overrides it.',
        env: { name: 'OCTOCODE_CLASSIFICATION_MODEL', dotenv: 'all', normalize: 'trim' },
        displayDefault: 'jev-latest',
        group: 'Classification',
        notes: 'Pin an exact model (for example jev-1.13.0) for reproducible results; an alias can change between fresh evaluations.',
      },
    },
  },
};
