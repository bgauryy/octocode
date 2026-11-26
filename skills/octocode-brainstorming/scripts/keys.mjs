#!/usr/bin/env node
// Generated from src/keys.mjs by skills/common/bundle-skill.mjs: edit the source, then run `yarn build` in this skill.

// ../../packages/octocode-config/dist/index.js
import { homedir } from "node:os";
import path from "node:path";
import fs from "node:fs";
import path2 from "node:path";
import fs2 from "node:fs";
import path3 from "node:path";
var CONFIG_FIELDS = [
  {
    "path": "github.apiUrl",
    "section": "github",
    "key": "apiUrl",
    "type": "url",
    "resolved": true,
    "credential": false,
    "description": "GitHub REST API root. GitHub Enterprise commonly uses /api/v3.",
    "env": {
      "name": "GITHUB_API_URL",
      "dotenv": "home",
      "normalize": "trim"
    },
    "defaultValue": "https://api.github.com"
  },
  {
    "path": "local.enabled",
    "section": "local",
    "key": "enabled",
    "type": "boolean",
    "resolved": true,
    "credential": false,
    "description": "Expose the local section through MCP: the local filesystem tools, ghCloneRepo (with tools.enableClone) and the graph tools. Off, MCP lists the GitHub and package tools only. CLI tools remain available; path and content security apply on both surfaces. Restart MCP after changing this setting.",
    "notes": "Home-only, so a repository can turn it off for MCP but cannot turn it on.",
    "env": {
      "name": "OCTOCODE_ENABLE_LOCAL",
      "dotenv": "home",
      "workspaceNarrowsTo": "false"
    },
    "defaultValue": false
  },
  {
    "path": "local.beta",
    "section": "local",
    "key": "beta",
    "type": "boolean",
    "resolved": true,
    "credential": false,
    "description": "Enable beta tools on their owning MCP server. The octocode-graph launcher opts into its graph tools by default. Restart the server after changing its environment.",
    "env": {
      "name": "OCTOCODE_BETA",
      "dotenv": "home"
    },
    "defaultValue": false
  },
  {
    "path": "local.allowedPaths",
    "section": "local",
    "key": "allowedPaths",
    "type": "stringArray",
    "resolved": true,
    "credential": false,
    "description": "Extra absolute or home-relative roots added to the allowed roots (workspace root and OCTOCODE_HOME).",
    "notes": "The environment value is a comma-separated list.",
    "env": {
      "name": "ALLOWED_PATHS",
      "dotenv": "home"
    },
    "defaultValue": [],
    "itemFormat": "path"
  },
  {
    "path": "local.workspaceRoot",
    "section": "local",
    "key": "workspaceRoot",
    "type": "path",
    "resolved": true,
    "credential": false,
    "description": "Optional absolute or home-relative workspace root.",
    "env": {
      "name": "WORKSPACE_ROOT",
      "dotenv": "home",
      "normalize": "trim"
    },
    "defaultValue": null
  },
  {
    "path": "local.graphHome",
    "section": "local",
    "key": "graphHome",
    "type": "path",
    "resolved": true,
    "credential": false,
    "description": "Directory in which graphBuild keeps its snapshots, in place of the workspace's .octocode/graph, shared by every workspace: a build then writes nothing in the repository and may scan any allowed directory, such as a ghCloneRepo checkout. Octocode creates the directory if needed and writes only in its own octocode-graph folder there, never reading or removing anything else; the setting grants local tools no read access to it. A build of a path that contains it is refused.",
    "notes": "Home-only, so a repository cannot choose where Octocode writes.",
    "env": {
      "name": "OCTOCODE_GRAPH_HOME",
      "dotenv": "home",
      "normalize": "trim"
    },
    "defaultValue": null
  },
  {
    "path": "tools.enabled",
    "section": "tools",
    "key": "enabled",
    "type": "stringArray",
    "resolved": true,
    "credential": false,
    "description": "Strict MCP tool allowlist; other MCP prerequisites still apply. The CLI ignores this list.",
    "env": {
      "name": "TOOLS_TO_RUN"
    },
    "defaultValue": null
  },
  {
    "path": "tools.disabled",
    "section": "tools",
    "key": "disabled",
    "type": "stringArray",
    "resolved": true,
    "credential": false,
    "description": "Tools removed from MCP discovery and admission. The CLI ignores this list.",
    "env": {
      "name": "DISABLE_TOOLS"
    },
    "defaultValue": null
  },
  {
    "path": "tools.enableClone",
    "section": "tools",
    "key": "enableClone",
    "type": "boolean",
    "resolved": true,
    "credential": false,
    "description": "Expose ghCloneRepo through MCP, with the local tools that read its checkout (local.enabled). CLI remains available. Clone execution requires persistent storage on both surfaces. Restart MCP after changing this setting.",
    "env": {
      "name": "OCTOCODE_ENABLE_CLONE",
      "dotenv": "home",
      "workspaceNarrowsTo": "false"
    },
    "defaultValue": false
  },
  {
    "path": "tools.enableSecurity",
    "section": "tools",
    "key": "enableSecurity",
    "type": "boolean",
    "resolved": true,
    "credential": false,
    "description": "Expose securityCheck through MCP: it asks OSV (api.osv.dev, operated by Google) about packages, sending each public package name and version it checks. Off, MCP leaves it out and sends nothing. The CLI always offers it, and a CLI call sends the package name to OSV only when you run it. Restart MCP after changing this setting.",
    "notes": "Home-only, so a repository can turn it off for MCP but cannot turn it on.",
    "env": {
      "name": "OCTOCODE_ENABLE_SECURITY",
      "aliases": [
        "OCTOCODE_SECURITY_CHECKS"
      ],
      "dotenv": "home",
      "workspaceNarrowsTo": "false"
    },
    "defaultValue": false
  },
  {
    "path": "network.allowPrivateRegistry",
    "section": "network",
    "key": "allowPrivateRegistry",
    "type": "boolean",
    "resolved": true,
    "credential": false,
    "description": "Allow private, loopback, or link-local artifact registries.",
    "notes": "Security-sensitive SSRF escape hatch; opt in only for trusted enterprise registries.",
    "env": {
      "name": "OCTOCODE_ALLOW_PRIVATE_REGISTRY",
      "dotenv": "home"
    },
    "defaultValue": false
  },
  {
    "path": "lsp.autoInstall",
    "section": "lsp",
    "key": "autoInstall",
    "type": "enum",
    "resolved": true,
    "credential": false,
    "description": "Whether `octocode lsp-server install` may download a managed language server: prompt (ask), off, or auto.",
    "env": {
      "name": "OCTOCODE_LSP_AUTO_INSTALL",
      "dotenv": "home",
      "normalize": "lower"
    },
    "defaultValue": "prompt",
    "values": [
      "prompt",
      "off",
      "auto"
    ]
  },
  {
    "path": "lsp.trustProjectConfig",
    "section": "lsp",
    "key": "trustProjectConfig",
    "type": "boolean",
    "resolved": true,
    "credential": false,
    "description": "Trust a project .octocode/lsp-servers.json. Security-sensitive: a trusted project config can launch server binaries.",
    "env": {
      "name": "OCTOCODE_TRUST_PROJECT_LSP_CONFIG",
      "dotenv": "home"
    },
    "defaultValue": false
  },
  {
    "path": "lsp.trustFetchedCode",
    "section": "lsp",
    "key": "trustFetchedCode",
    "type": "boolean",
    "resolved": true,
    "credential": false,
    "description": "Let lspSearch start language servers on code Octocode fetched: ghCloneRepo and ghStructure materialize checkouts and URL captures under its home. Off by default, because a language server can run code from the repository it opens; text, AST and graph tools read that code without running anything.",
    "notes": "Security-sensitive: turning it on lets a repository you fetched run code on this machine when lspSearch opens it. Home-only, so a repository cannot turn it on.",
    "env": {
      "name": "OCTOCODE_LSP_TRUST_FETCHED_CODE",
      "dotenv": "home",
      "workspaceNarrowsTo": "false"
    },
    "defaultValue": false
  },
  {
    "path": "lsp.prewarm",
    "section": "lsp",
    "key": "prewarm",
    "type": "enum",
    "resolved": true,
    "credential": false,
    "description": "Language-server prewarm: targeted warms the server of the file a localSearch, localFetch or astSearch row names and of an lspSearch call the response offers; all also warms files named by other local tools and, at MCP initialize, the workspace's top three languages; off disables prewarm. Only languages with a configured server start, and each start indexes a whole project: targeted avoids unrelated server starts, but a lookup before any read or search can take seconds.",
    "env": {
      "name": "OCTOCODE_LSP_PREWARM",
      "normalize": "lower"
    },
    "defaultValue": "targeted",
    "values": [
      "targeted",
      "all",
      "off"
    ]
  },
  {
    "path": "output.redactEmails",
    "section": "output",
    "key": "redactEmails",
    "type": "boolean",
    "resolved": true,
    "credential": false,
    "description": "Mask email addresses such as GitHub commit authors.",
    "env": {
      "name": "OCTOCODE_REDACT_EMAILS",
      "dotenv": "home",
      "workspaceNarrowsTo": "true"
    },
    "defaultValue": false
  },
  {
    "path": "output.redactUuid",
    "section": "output",
    "key": "redactUuid",
    "type": "boolean",
    "resolved": true,
    "credential": false,
    "description": "Mask UUIDs (8-4-4-4-12 hexadecimal) in every tool result. Off by default: a UUID is an identifier, not a secret.",
    "env": {
      "name": "OCTOCODE_REDACT_UUID",
      "dotenv": "home",
      "workspaceNarrowsTo": "true"
    },
    "defaultValue": false
  },
  {
    "path": "output.mcpResult",
    "section": "output",
    "key": "mcpResult",
    "type": "enum",
    "resolved": true,
    "credential": false,
    "description": 'Format of an MCP tools/call reply: json sends the compact JSON result as one text block; text sends the text view the CLI prints (content lines, then `# key: value` lines, each continuation as a runnable `{"tool","input"}` call), which a client must not parse as JSON.',
    "env": {
      "name": "OCTOCODE_MCP_RESULT",
      "normalize": "lower",
      "dotenv": "home"
    },
    "defaultValue": "json",
    "values": [
      "json",
      "text"
    ]
  },
  {
    "path": "security.credentialFiles",
    "section": "security",
    "key": "credentialFiles",
    "type": "enum",
    "resolved": true,
    "credential": false,
    "description": "Whether local read and search tools may read the known credential files (.env and .env.*, credentials.json, .netrc, .npmrc, id_rsa*, *.pem and the like): withhold them, or allow them in the workspace and allowed paths so results match ripgrep. Known secret formats stay redacted. Credential directories such as .ssh, .aws and .git, the Octocode home, and known credential files directly in your home or in its dot directories (~/.npmrc, ~/.codex/auth.json) stay withheld; every other file, such as ~/.zshrc or a shell history, follows the allowed roots like any file. Graph snapshots, clasify and classification never read these files, and allow keeps saved local results in memory, so a cursor ends with its process.",
    "notes": "Security-sensitive: redaction recognizes known secret formats only, so a secret in no known format can appear in results. Home-only, so a repository cannot turn it on.",
    "env": {
      "name": "OCTOCODE_CREDENTIAL_FILES",
      "dotenv": "home",
      "normalize": "lower",
      "workspaceNarrowsTo": "withhold"
    },
    "defaultValue": "withhold",
    "values": [
      "withhold",
      "allow"
    ]
  },
  {
    "path": "storage.mode",
    "section": "storage",
    "key": "mode",
    "type": "enum",
    "resolved": true,
    "credential": false,
    "description": "Persistent saves continuation state and provider answers to disk; memory retains them only in the current runtime. Fresh local requests reuse only in-memory results keyed by the content they read; GitHub and registry answers repeat as their origin allows. Explicit graph snapshots are outputs; interrupted extraction checkpoints follow this setting.",
    "env": {
      "name": "OCTOCODE_STORAGE_MODE",
      "dotenv": "home",
      "normalize": "lower",
      "workspaceNarrowsTo": "memory"
    },
    "defaultValue": "persistent",
    "values": [
      "persistent",
      "memory"
    ]
  },
  {
    "path": "storage.cloneCacheLimit",
    "section": "storage",
    "key": "cloneCacheLimit",
    "type": "string",
    "resolved": true,
    "credential": false,
    "description": 'Byte cap for the clean ghCloneRepo checkouts under the Octocode home, as a whole number with an optional binary unit (B, KiB, MiB, GiB, TiB; K, M, G, T, KB, MB, GB and TB mean the same): "5GiB", "512MiB", "1073741824". After a new checkout is published, the least recently used checkouts are removed until the total fits. A checkout in use, one with local changes, and the one just published stay. 0 means no cap. An invalid value is reported and the default applies. The shared object stores behind the checkouts are not counted.',
    "notes": "Home-only, so a repository cannot make Octocode discard your checkouts.",
    "env": {
      "name": "OCTOCODE_CLONE_CACHE_LIMIT",
      "dotenv": "home",
      "normalize": "trim"
    },
    "defaultValue": "5GiB"
  },
  {
    "path": "classification.api",
    "section": "classification",
    "key": "api",
    "type": "string",
    "resolved": false,
    "credential": true,
    "description": "Classification provider API key. A nonblank value exposes clasify and read-tool classifiers in MCP and the CLI; without one, a CLI clasify call answers where to get a key. Discovery makes no provider request.",
    "notes": "Credential values are omitted from resolved configuration and diagnostics. Home-only, so a repository cannot turn classification on or send your evidence under its own provider account.",
    "env": {
      "name": "OCTOCODE_CLASSIFICATION_API",
      "dotenv": "home",
      "normalize": "trim"
    },
    "defaultValue": null
  },
  {
    "path": "classification.apiHost",
    "section": "classification",
    "key": "apiHost",
    "type": "url",
    "resolved": true,
    "credential": false,
    "description": "Classification provider API root, for a proxy, gateway or compatible provider that serves the same API.",
    "notes": "Calls require HTTPS except on loopback, and an API root with no path. Home-only, so a repository cannot redirect your key.",
    "env": {
      "name": "OCTOCODE_CLASSIFICATION_API_HOST",
      "dotenv": "home",
      "normalize": "trim"
    },
    "defaultValue": null
  },
  {
    "path": "classification.model",
    "section": "classification",
    "key": "model",
    "type": "string",
    "resolved": true,
    "credential": false,
    "description": "Classification model for read-tool classifiers and clasify; a clasify row's model overrides it.",
    "notes": "Pin an exact model (for example jev-1.13.0) for reproducible results; an alias can change between fresh evaluations.",
    "env": {
      "name": "OCTOCODE_CLASSIFICATION_MODEL",
      "dotenv": "all",
      "normalize": "trim"
    },
    "defaultValue": null
  }
];
var ENV_ALIASES = { "OCTOCODE_SECURITY_CHECKS": "OCTOCODE_ENABLE_SECURITY" };
var API_KEYS = [
  {
    "name": "TAVILY_API_KEY",
    "dotenv": "all",
    "group": "Web search",
    "description": "Tavily search API credential used by the octocode-brainstorming skill.",
    "docsUrl": "https://app.tavily.com/"
  },
  {
    "name": "EXA_API_KEY",
    "dotenv": "all",
    "group": "Web search",
    "description": "Exa search API credential used by the octocode-brainstorming skill.",
    "docsUrl": "https://dashboard.exa.ai/"
  },
  {
    "name": "SERPER_API_KEY",
    "dotenv": "all",
    "group": "Web search",
    "description": "Serper search API credential used by the octocode-brainstorming skill.",
    "docsUrl": "https://serper.dev/"
  },
  {
    "name": "DEEPSEEK_API_KEY",
    "dotenv": "all",
    "group": "Web search",
    "description": "DeepSeek search API credential used by the octocode-brainstorming skill.",
    "docsUrl": "https://platform.deepseek.com/api_keys"
  },
  {
    "name": "GH_TOKEN",
    "dotenv": "all",
    "group": "GitHub",
    "description": "GitHub CLI token"
  },
  {
    "name": "GITHUB_TOKEN",
    "dotenv": "all",
    "group": "GitHub",
    "description": "GitHub Actions token"
  },
  {
    "name": "SCRAPING_ANT",
    "dotenv": "all",
    "group": "Scraping",
    "description": "ScrapingAnt API credential used by the octocode-scraping skill.",
    "docsUrl": "https://app.scrapingant.com/"
  },
  {
    "name": "OCTOCODE_CLASSIFICATION_API",
    "dotenv": "home",
    "group": "Classification",
    "description": "Classification provider API key. A nonblank value exposes clasify and read-tool classifiers in MCP and the CLI; without one, a CLI clasify call answers where to get a key. Discovery makes no provider request.",
    "docsUrl": "https://docs.typesafe.ai/introduction"
  }
];
var PROTECTED_KEY_NAMES = ["PATH", "HOME", "SHELL", "USER", "LOGNAME", "PWD", "TMPDIR", "NODE_OPTIONS", "PYTHON", "TAVILY_BASE_URL", "EXA_BASE_URL", "SERPER_BASE_URL", "DEEPSEEK_BASE_URL", "HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY", "NO_PROXY", "SSL_CERT_FILE", "SSL_CERT_DIR", "GIT_SSL_CAINFO", "GIT_SSL_CAPATH", "CURL_CA_BUNDLE", "OCTOCODE_GITHUB_CLIENT_ID", "OCTOCODE_HOME", "OCTOCODE_CARGO", "GITHUB_API_URL", "OCTOCODE_ENABLE_LOCAL", "OCTOCODE_BETA", "ALLOWED_PATHS", "WORKSPACE_ROOT", "OCTOCODE_GRAPH_HOME", "OCTOCODE_ENABLE_CLONE", "OCTOCODE_ENABLE_SECURITY", "OCTOCODE_ALLOW_PRIVATE_REGISTRY", "OCTOCODE_LSP_AUTO_INSTALL", "OCTOCODE_TRUST_PROJECT_LSP_CONFIG", "OCTOCODE_LSP_TRUST_FETCHED_CODE", "OCTOCODE_REDACT_EMAILS", "OCTOCODE_REDACT_UUID", "OCTOCODE_MCP_RESULT", "OCTOCODE_CREDENTIAL_FILES", "OCTOCODE_STORAGE_MODE", "OCTOCODE_CLONE_CACHE_LIMIT", "OCTOCODE_CLASSIFICATION_API", "OCTOCODE_CLASSIFICATION_API_HOST"];
var HOME_TRUSTED_ENV_KEYS = ["TAVILY_BASE_URL", "EXA_BASE_URL", "SERPER_BASE_URL", "DEEPSEEK_BASE_URL", "OCTOCODE_GITHUB_CLIENT_ID", "OCTOCODE_CARGO", "GITHUB_API_URL", "OCTOCODE_ENABLE_LOCAL", "OCTOCODE_BETA", "ALLOWED_PATHS", "WORKSPACE_ROOT", "OCTOCODE_GRAPH_HOME", "OCTOCODE_ENABLE_CLONE", "OCTOCODE_ENABLE_SECURITY", "OCTOCODE_ALLOW_PRIVATE_REGISTRY", "OCTOCODE_LSP_AUTO_INSTALL", "OCTOCODE_TRUST_PROJECT_LSP_CONFIG", "OCTOCODE_LSP_TRUST_FETCHED_CODE", "OCTOCODE_REDACT_EMAILS", "OCTOCODE_REDACT_UUID", "OCTOCODE_MCP_RESULT", "OCTOCODE_CREDENTIAL_FILES", "OCTOCODE_STORAGE_MODE", "OCTOCODE_CLONE_CACHE_LIMIT", "OCTOCODE_CLASSIFICATION_API", "OCTOCODE_CLASSIFICATION_API_HOST"];
var DEFAULT_CONFIG_VALUE = { "github": { "apiUrl": "https://api.github.com" }, "local": { "enabled": false, "beta": false, "allowedPaths": [], "workspaceRoot": void 0, "graphHome": void 0 }, "tools": { "enabled": null, "disabled": null, "enableClone": false, "enableSecurity": false }, "network": { "allowPrivateRegistry": false }, "lsp": { "autoInstall": "prompt", "trustProjectConfig": false, "trustFetchedCode": false, "prewarm": "targeted" }, "output": { "redactEmails": false, "redactUuid": false, "mcpResult": "json" }, "security": { "credentialFiles": "withhold" }, "storage": { "mode": "persistent", "cloneCacheLimit": "5GiB" }, "classification": { "apiHost": void 0, "model": void 0 } };
function getOctocodeHome(env = process.env) {
  const override = env["OCTOCODE_HOME"];
  if (override && override.trim()) return path.resolve(override.trim());
  return path.join(homedir(), ".octocode");
}
var API_KEY_NAMES = new Set(
  API_KEYS.map((key) => key.name)
);
var PROTECTED_KEYS = new Set(PROTECTED_KEY_NAMES);
var PROTECTED_KEYS_CI = new Set(
  PROTECTED_KEY_NAMES.map((name) => name.toUpperCase())
);
function isProtectedKey(key) {
  if (PROTECTED_KEYS.has(key)) return true;
  return process.platform === "win32" && PROTECTED_KEYS_CI.has(key.toUpperCase());
}
function parseEnv(text) {
  const out = {};
  if (!text) return out;
  for (const rawLine of text.split("\n")) {
    const trimmed = rawLine.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const normalized = trimmed.startsWith("export ") ? trimmed.slice("export ".length).trim() : trimmed;
    const eq = normalized.indexOf("=");
    if (eq === -1) continue;
    const key = normalized.slice(0, eq).trim();
    if (!key) continue;
    out[key] = normalized.slice(eq + 1).trim().replace(/^["']|["']$/g, "");
  }
  return out;
}
function sameFile(left, right) {
  if (path2.resolve(left) === path2.resolve(right)) return true;
  try {
    return fs.realpathSync(left) === fs.realpathSync(right);
  } catch {
    return false;
  }
}
function readTextIfExists(filePath) {
  try {
    return fs.readFileSync(filePath, "utf8");
  } catch {
    return "";
  }
}
function canonicalEnvName(key) {
  return Object.hasOwn(ENV_ALIASES, key) ? ENV_ALIASES[key] : key;
}
function canonicalEnv(env) {
  const current = {};
  const earlier = [];
  for (const [key, value] of Object.entries(env)) {
    if (value === void 0) continue;
    const name = canonicalEnvName(key);
    if (name === key) current[key] = value;
    else earlier.push([name, value]);
  }
  for (const [name, value] of earlier) current[name] ??= value;
  return current;
}
function fileEnv(filePath) {
  const entries = Object.entries(parseEnv(readTextIfExists(filePath))).filter(
    ([, v]) => v.trim()
  );
  return canonicalEnv(Object.fromEntries(entries));
}
var CLASSIFICATION_KILL_SWITCH = CONFIG_FIELDS.find(
  (field) => field.path === "classification.api"
)?.env?.name;
var HOME_TRUSTED = new Set(HOME_TRUSTED_ENV_KEYS);
function context(options) {
  const env = options.env ?? process.env;
  const home = options.home ?? getOctocodeHome(env);
  const cwd = options.cwd ?? process.cwd();
  return {
    env,
    files: {
      home: path3.join(home, ".env"),
      workspace: path3.join(cwd, ".octocode", ".env")
    }
  };
}
function keyFiles(options = {}) {
  return context(options).files;
}
function getKey(requested, options = {}) {
  const name = canonicalEnvName(requested);
  const { env: processEnv, files } = context(options);
  const env = canonicalEnv(processEnv);
  const fromProcess = env[name]?.trim() ? env[name] : void 0;
  if (fromProcess)
    return { name, value: fromProcess.trim(), source: "process", files };
  if (name === CLASSIFICATION_KILL_SWITCH && env[name] !== void 0)
    return { name, files };
  if (isProtectedKey(name) && !HOME_TRUSTED.has(name)) return { name, files };
  const fromHome = fileEnv(files.home)[name];
  if (fromHome) return { name, value: fromHome, source: "home", files };
  if (isProtectedKey(name) || sameFile(files.workspace, files.home))
    return { name, files };
  const fromWorkspace = fileEnv(files.workspace)[name];
  if (fromWorkspace)
    return { name, value: fromWorkspace, source: "workspace", files };
  return { name, files };
}
function listKeys(names, options = {}) {
  const specs = names?.length ? names.map(
    (name) => API_KEYS.find((spec) => spec.name === name) ?? {
      name,
      dotenv: "all",
      description: "Not a declared Octocode key"
    }
  ) : API_KEYS;
  const keys = specs.map((spec) => {
    const { value, source } = getKey(spec.name, options);
    return {
      ...spec,
      key: value ? "set" : "missing",
      ...source ? { source } : {}
    };
  });
  return { files: keyFiles(options), keys };
}
function setKey(name, value, { scope = "home", ...options } = {}) {
  const spec = API_KEYS.find((candidate) => candidate.name === name);
  if (!spec)
    throw new Error(
      `${name} is not a declared Octocode API key (${API_KEYS.map((key) => key.name).join(", ")})`
    );
  if (scope === "workspace" && spec.dotenv === "home")
    throw new Error(`${name} is home-only: a workspace .env cannot set it`);
  const clean = value.trim();
  if (!clean) throw new Error(`no value for ${name}`);
  if (/[\r\n]/.test(clean)) throw new Error(`${name} must be one line`);
  const files = keyFiles(options);
  const file = files[scope];
  fs2.mkdirSync(path3.dirname(file), { recursive: true, mode: 448 });
  const names = /* @__PURE__ */ new Set([name]);
  let existing = "";
  try {
    existing = fs2.readFileSync(file, "utf8");
  } catch {
  }
  const kept = existing.split("\n").filter((line) => {
    const entry = line.trim().replace(/^export\s+/, "");
    const eq = entry.indexOf("=");
    return eq === -1 || !names.has(entry.slice(0, eq).trim());
  });
  while (kept.length && !kept.at(-1)?.trim()) kept.pop();
  fs2.writeFileSync(file, `${[...kept, `${name}=${clean}`].join("\n")}
`, {
    mode: 384
  });
  fs2.chmodSync(file, 384);
  const { source } = getKey(name, options);
  return { name, scope, file, ...source ? { source } : {} };
}
var USAGE = `Usage: keys.mjs list [NAME...]
       keys.mjs get NAME...
       keys.mjs set NAME --stdin [--workspace]

A key comes from the first place that sets it: the process environment, then <home>/.env
(OCTOCODE_HOME, default ~/.octocode), then <workspace>/.octocode/.env (the current directory).
list: every declared API key (or the named ones), set or missing, with its source and docs link.
get:  the same for the named keys; exit 1 when any is missing.
set:  saves the value read from stdin to <home>/.env, or with --workspace to <workspace>/.octocode/.env.
No command prints a key value.`;
function runKeysCli(argv, {
  readStdin = () => fs2.readFileSync(0, "utf8"),
  ...options
} = {}) {
  const [command, ...rest] = argv;
  const fail = (code, message, exitCode = 2) => ({
    exitCode,
    output: { ok: false, error: { code, message } }
  });
  if (!command || command === "--help" || command === "-h")
    return { exitCode: command ? 0 : 2, output: USAGE };
  if (command === "list" || command === "get") {
    const names = rest.filter((arg) => !arg.startsWith("-"));
    const unknown = rest.find((arg) => arg.startsWith("-"));
    if (unknown) return fail("invalidArguments", `unknown option ${unknown}`);
    if (command === "get" && !names.length)
      return fail("invalidArguments", "get needs at least one key name");
    const { files, keys } = listKeys(names, options);
    const missing = keys.filter((key) => key.key === "missing");
    const ok = command === "list" || missing.length === 0;
    return {
      exitCode: ok ? 0 : 1,
      output: {
        ok,
        files,
        keys: keys.map(
          (key) => key.key === "missing" ? { ...key, fix: `keys.mjs set ${key.name} --stdin` } : key
        )
      }
    };
  }
  if (command === "set") {
    const name = rest.find((arg) => !arg.startsWith("-"));
    const flags = rest.filter((arg) => arg.startsWith("-"));
    const badFlag = flags.find(
      (flag) => flag !== "--stdin" && flag !== "--workspace"
    );
    if (badFlag) return fail("invalidArguments", `unknown option ${badFlag}`);
    if (!name) return fail("invalidArguments", "set needs a key name");
    if (rest.filter((arg) => !arg.startsWith("-")).length > 1)
      return fail(
        "invalidArguments",
        "pass the value on stdin with --stdin, never as an argument"
      );
    if (!flags.includes("--stdin"))
      return fail(
        "invalidArguments",
        "set reads the value from stdin: pass --stdin and paste or pipe the key"
      );
    try {
      const saved = setKey(name, readStdin(), {
        ...options,
        scope: flags.includes("--workspace") ? "workspace" : "home"
      });
      return { exitCode: 0, output: { ok: true, ...saved } };
    } catch (error) {
      return fail(
        "invalidKey",
        error instanceof Error ? error.message : String(error)
      );
    }
  }
  return fail(
    "invalidArguments",
    `unknown command ${command} (list, get, set)`
  );
}
var DEFAULT_CONFIG = Object.freeze(DEFAULT_CONFIG_VALUE);

// scripts/lib/entrypoint.mjs
import { realpathSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
var realOrSelf = (file) => {
  try {
    return realpathSync(file);
  } catch {
    return resolve(file);
  }
};
function isMainModule(moduleUrl) {
  return Boolean(process.argv[1]) && realOrSelf(process.argv[1]) === realOrSelf(fileURLToPath(moduleUrl));
}

// src/keys.mjs
if (isMainModule(import.meta.url)) {
  const { exitCode, output } = runKeysCli(process.argv.slice(2));
  console.log(typeof output === "string" ? output : JSON.stringify(output, null, 2));
  process.exitCode = exitCode;
}
export {
  getKey,
  keyFiles,
  listKeys
};
