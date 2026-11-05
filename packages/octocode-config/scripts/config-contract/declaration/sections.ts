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
        description: 'Expose securityCheck on MCP and the CLI: it asks OSV (api.osv.dev, operated by Google) about packages, sending each public package name and version it checks. Off, the tool is absent from both surfaces and sends nothing.',
        notes: 'Home-only, so a repository can turn it off but cannot turn it on.',
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
      prewarm: {
        type: 'enum',
        values: ['targeted', 'all', 'off'],
        description: 'Language-server prewarm: targeted starts the server an offered lspSearch call names once the session has run an lspSearch; all warms it from the start, also warms the file a read or search names and, at MCP initialize, the workspace\'s top three languages; off disables it. Each start indexes a whole project, so a first lspSearch under targeted can take seconds in exchange for no server in a session that never calls one.',
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
    },
  },
  security: {
    title: 'Security',
    fields: {
      credentialFiles: {
        type: 'enum',
        typeName: 'CredentialFilesPolicy',
        values: ['withhold', 'allow'],
        description: 'Whether local read and search tools may read files named like credentials (.env, credentials.json, *.pem): withhold them, or allow them in the workspace and allowed paths so results match ripgrep. Secret values stay redacted. Sensitive directories such as .ssh and .git, the Octocode home, and files and dot directories directly in your home stay withheld. Graph snapshots, clasify and classification never read these files, and allow keeps saved local results in memory, so a cursor ends with its process.',
        notes: 'Security-sensitive: redaction recognizes secret formats, so a secret in no known format can appear in results. Home-only, so a repository cannot turn it on.',
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
        description: 'Persistent saves continuation state and provider answers to disk; memory retains them only in the current runtime. Fresh local requests recompute results; GitHub and registry answers repeat as their origin allows. Explicit graph snapshots are outputs; interrupted extraction checkpoints follow this setting.',
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
        env: { name: 'OCTOCODE_CLASSIFICATION_API', dotenv: 'all', normalize: 'trim' },
        displayDefault: 'unset (classification off)',
        group: 'Classification',
        docsUrl: 'https://docs.typesafe.ai/introduction',
        notes: 'Credential values are omitted from resolved configuration and diagnostics.',
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
