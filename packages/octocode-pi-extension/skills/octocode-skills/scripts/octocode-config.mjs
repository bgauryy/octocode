// src/index.ts
import fs from "node:fs";
import path3 from "node:path";

// src/home.ts
import os from "node:os";
import path from "node:path";
function getOctocodeHome(env = process.env) {
  const override = env["OCTOCODE_HOME"];
  if (override && override.trim()) return path.resolve(override.trim());
  const home = os.homedir();
  const platform = os.platform();
  if (platform === "win32") {
    const appData = env["APPDATA"] ?? path.join(home, "AppData", "Roaming");
    return path.join(appData, ".octocode");
  }
  if (platform === "darwin") return path.join(home, ".octocode");
  const xdg = env["XDG_CONFIG_HOME"] ?? path.join(home, ".config");
  return path.join(xdg, ".octocode");
}

// src/config/types.ts
var CONFIG_SCHEMA_VERSION = 1;
var CONFIG_FILE_NAME = ".octocoderc";

// src/config/defaults.ts
var DEFAULT_GITHUB_CONFIG = {
  apiUrl: "https://api.github.com"
};
var DEFAULT_LOCAL_CONFIG = {
  enabled: true,
  enableClone: false,
  allowedPaths: [],
  workspaceRoot: void 0
};
var DEFAULT_TOOLS_CONFIG = {
  enabled: null,
  enableAdditional: null,
  disabled: null
};
var DEFAULT_NETWORK_CONFIG = {
  timeout: 3e4,
  maxRetries: 3
};
var DEFAULT_LSP_CONFIG = {
  configPath: void 0
};
var DEFAULT_OUTPUT_CONFIG = {
  format: "yaml",
  pagination: {
    defaultCharLength: 2e4
  }
};
var DEFAULT_CONFIG = {
  version: 1,
  github: DEFAULT_GITHUB_CONFIG,
  local: DEFAULT_LOCAL_CONFIG,
  tools: DEFAULT_TOOLS_CONFIG,
  network: DEFAULT_NETWORK_CONFIG,
  lsp: DEFAULT_LSP_CONFIG,
  output: DEFAULT_OUTPUT_CONFIG
};
var MIN_TIMEOUT = 5e3;
var MAX_TIMEOUT = 3e5;
var MIN_RETRIES = 0;
var MAX_RETRIES = 10;
var MIN_OUTPUT_DEFAULT_CHAR_LENGTH = 1e3;
var MAX_OUTPUT_DEFAULT_CHAR_LENGTH = 5e4;

// src/config/runtimeSurface.ts
var SURFACE_KEY = "__octocodeRuntimeSurface__";
function setRuntimeSurface(surface) {
  globalThis[SURFACE_KEY] = surface;
}
function getRuntimeSurface() {
  return globalThis[SURFACE_KEY] ?? "mcp";
}
function _resetRuntimeSurface() {
  delete globalThis[SURFACE_KEY];
}

// src/config/validator.ts
function validateUrl(url, field) {
  if (url === void 0 || url === null) return null;
  if (typeof url !== "string") {
    return `${field}: Must be a string`;
  }
  try {
    const parsed = new URL(url);
    if (!["http:", "https:"].includes(parsed.protocol)) {
      return `${field}: Only http/https URLs allowed`;
    }
    return null;
  } catch {
    return `${field}: Invalid URL format`;
  }
}
function validateNumberRange(value, field, min, max) {
  if (value === void 0 || value === null) return null;
  if (typeof value !== "number" || isNaN(value)) {
    return `${field}: Must be a number`;
  }
  if (value < min || value > max) {
    return `${field}: Must be between ${min} and ${max}`;
  }
  return null;
}
function validateBoolean(value, field) {
  if (value === void 0 || value === null) return null;
  if (typeof value !== "boolean") {
    return `${field}: Must be a boolean`;
  }
  return null;
}
function validateStringArray(value, field) {
  if (value === void 0 || value === null) return null;
  if (!Array.isArray(value)) {
    return `${field}: Must be an array`;
  }
  for (let i = 0; i < value.length; i++) {
    if (typeof value[i] !== "string") {
      return `${field}[${i}]: Must be a string`;
    }
  }
  return null;
}
function validateAllowedPathElements(paths) {
  const errors = [];
  for (let i = 0; i < paths.length; i++) {
    const p = paths[i];
    if (typeof p !== "string") continue;
    if (p.trim() === "") {
      errors.push(`local.allowedPaths[${i}]: empty or whitespace-only path`);
    } else if (!p.startsWith("/") && !p.startsWith("~")) {
      errors.push(
        `local.allowedPaths[${i}]: must be absolute path or start with ~ (got "${p}")`
      );
    } else if (p.includes("..")) {
      errors.push(
        `local.allowedPaths[${i}]: path traversal (..) not allowed (got "${p}")`
      );
    }
  }
  return errors;
}
function validateNullableStringArray(value, field) {
  if (value === void 0) return null;
  if (value === null) return null;
  return validateStringArray(value, field);
}
function validateString(value, field) {
  if (value === void 0 || value === null) return null;
  if (typeof value !== "string") {
    return `${field}: Must be a string`;
  }
  return null;
}
function validateGitHub(github, errors) {
  if (github === void 0 || github === null) return;
  if (typeof github !== "object" || Array.isArray(github)) {
    errors.push("github: Must be an object");
    return;
  }
  const gh = github;
  const apiUrlError = validateUrl(gh.apiUrl, "github.apiUrl");
  if (apiUrlError) errors.push(apiUrlError);
}
function validateLocal(local, errors) {
  if (local === void 0 || local === null) return;
  if (typeof local !== "object" || Array.isArray(local)) {
    errors.push("local: Must be an object");
    return;
  }
  const loc = local;
  const enabledError = validateBoolean(loc.enabled, "local.enabled");
  if (enabledError) errors.push(enabledError);
  const enableCloneError = validateBoolean(
    loc.enableClone,
    "local.enableClone"
  );
  if (enableCloneError) errors.push(enableCloneError);
  const allowedPathsError = validateStringArray(
    loc.allowedPaths,
    "local.allowedPaths"
  );
  if (allowedPathsError) {
    errors.push(allowedPathsError);
  } else if (Array.isArray(loc.allowedPaths)) {
    const pathErrors = validateAllowedPathElements(
      loc.allowedPaths
    );
    errors.push(...pathErrors);
  }
  if (loc.workspaceRoot !== void 0 && loc.workspaceRoot !== null) {
    const workspaceRootError = validateString(
      loc.workspaceRoot,
      "local.workspaceRoot"
    );
    if (workspaceRootError) {
      errors.push(workspaceRootError);
    } else if (typeof loc.workspaceRoot === "string" && !loc.workspaceRoot.startsWith("/") && !loc.workspaceRoot.startsWith("~")) {
      errors.push(
        'local.workspaceRoot: must be an absolute path or start with ~ (got "' + loc.workspaceRoot + '")'
      );
    } else if (typeof loc.workspaceRoot === "string" && loc.workspaceRoot.includes("..")) {
      errors.push(
        'local.workspaceRoot: path traversal (..) not allowed (got "' + loc.workspaceRoot + '")'
      );
    }
  }
}
function validateTools(tools, errors) {
  if (tools === void 0 || tools === null) return;
  if (typeof tools !== "object" || Array.isArray(tools)) {
    errors.push("tools: Must be an object");
    return;
  }
  const t = tools;
  const enabledError = validateNullableStringArray(t.enabled, "tools.enabled");
  if (enabledError) errors.push(enabledError);
  const enableAdditionalError = validateNullableStringArray(
    t.enableAdditional,
    "tools.enableAdditional"
  );
  if (enableAdditionalError) errors.push(enableAdditionalError);
  const disabledError = validateNullableStringArray(
    t.disabled,
    "tools.disabled"
  );
  if (disabledError) errors.push(disabledError);
}
function validateNetwork(network, errors) {
  if (network === void 0 || network === null) return;
  if (typeof network !== "object" || Array.isArray(network)) {
    errors.push("network: Must be an object");
    return;
  }
  const net = network;
  const timeoutError = validateNumberRange(
    net.timeout,
    "network.timeout",
    MIN_TIMEOUT,
    MAX_TIMEOUT
  );
  if (timeoutError) errors.push(timeoutError);
  const retriesError = validateNumberRange(
    net.maxRetries,
    "network.maxRetries",
    MIN_RETRIES,
    MAX_RETRIES
  );
  if (retriesError) errors.push(retriesError);
}
function validateLsp(lsp, errors) {
  if (lsp === void 0 || lsp === null) return;
  if (typeof lsp !== "object" || Array.isArray(lsp)) {
    errors.push("lsp: Must be an object");
    return;
  }
  const l = lsp;
  const configPathError = validateString(l.configPath, "lsp.configPath");
  if (configPathError) errors.push(configPathError);
}
function validateOutput(output, errors) {
  if (output === void 0 || output === null) return;
  if (typeof output !== "object" || Array.isArray(output)) {
    errors.push("output: Must be an object");
    return;
  }
  const out = output;
  if (out.format !== void 0) {
    if (typeof out.format !== "string") {
      errors.push("output.format: Must be a string");
    } else if (!["yaml", "json"].includes(out.format)) {
      errors.push("output.format: Must be one of: yaml, json");
    }
  }
  if (out.pagination !== void 0 && out.pagination !== null) {
    if (typeof out.pagination !== "object" || Array.isArray(out.pagination)) {
      errors.push("output.pagination: Must be an object");
    } else {
      const pagination = out.pagination;
      const defaultCharLengthError = validateNumberRange(
        pagination.defaultCharLength,
        "output.pagination.defaultCharLength",
        MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
        MAX_OUTPUT_DEFAULT_CHAR_LENGTH
      );
      if (defaultCharLengthError) errors.push(defaultCharLengthError);
    }
  }
}
function validateConfig(config) {
  const errors = [];
  const warnings = [];
  if (typeof config !== "object" || config === null || Array.isArray(config)) {
    return {
      valid: false,
      errors: ["Configuration must be a JSON object"],
      warnings: []
    };
  }
  const cfg = config;
  if (cfg.version !== void 0) {
    if (typeof cfg.version !== "number" || !Number.isInteger(cfg.version)) {
      errors.push("version: Must be an integer");
    } else if (cfg.version > CONFIG_SCHEMA_VERSION) {
      warnings.push(
        `version: Config version ${cfg.version} is newer than supported version ${CONFIG_SCHEMA_VERSION}`
      );
    }
  }
  validateGitHub(cfg.github, errors);
  validateLocal(cfg.local, errors);
  validateTools(cfg.tools, errors);
  validateNetwork(cfg.network, errors);
  validateLsp(cfg.lsp, errors);
  validateOutput(cfg.output, errors);
  const knownKeys = /* @__PURE__ */ new Set([
    "$schema",
    "version",
    "github",
    "local",
    "tools",
    "network",
    "lsp",
    "output"
  ]);
  for (const key of Object.keys(cfg)) {
    if (!knownKeys.has(key)) {
      warnings.push(`Unknown configuration key: ${key}`);
    }
  }
  return {
    valid: errors.length === 0,
    errors,
    warnings,
    config: errors.length === 0 ? config : void 0
  };
}

// src/config/loader.ts
import { existsSync, readFileSync } from "node:fs";
import path2 from "node:path";
function stripJson5Features(content) {
  let result = "";
  let i = 0;
  let inString = false;
  let stringChar = "";
  while (i < content.length) {
    const char = content[i];
    const nextChar = content[i + 1];
    if (!inString && (char === '"' || char === "'")) {
      inString = true;
      stringChar = char;
      result += char;
      i++;
      continue;
    }
    if (inString) {
      result += char;
      if (char === "\\" && i + 1 < content.length) {
        result += content[i + 1];
        i += 2;
        continue;
      }
      if (char === stringChar) inString = false;
      i++;
      continue;
    }
    if (char === "/" && nextChar === "/") {
      while (i < content.length && content[i] !== "\n") i++;
      continue;
    }
    if (char === "/" && nextChar === "*") {
      i += 2;
      while (i < content.length - 1) {
        if (content[i] === "*" && content[i + 1] === "/") {
          i += 2;
          break;
        }
        i++;
      }
      continue;
    }
    result += char;
    i++;
  }
  return result.replace(/,(\s*[}\]])/g, "$1");
}
function parseJson5(content) {
  return JSON.parse(stripJson5Features(content));
}
function getConfigFilePath(home = getOctocodeHome()) {
  return path2.join(home, ".octocoderc");
}
function configExists(home) {
  return existsSync(getConfigFilePath(home));
}
function loadConfigSync(home) {
  const filePath = getConfigFilePath(home);
  if (!existsSync(filePath)) {
    return { success: false, error: "Config file does not exist", path: filePath };
  }
  try {
    const content = readFileSync(filePath, "utf-8");
    if (!content.trim()) {
      return { success: true, config: {}, path: filePath };
    }
    const parsed = parseJson5(content);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      return {
        success: false,
        error: "Config file has invalid structure: must be a JSON object",
        path: filePath
      };
    }
    return { success: true, config: parsed, path: filePath };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return { success: false, error: `Failed to parse config file: ${message}`, path: filePath };
  }
}
async function loadConfig(home) {
  return loadConfigSync(home);
}

// src/config/resolverSections.ts
function parseBooleanEnv(value) {
  if (value === void 0 || value === null) return void 0;
  const trimmed = value.trim().toLowerCase();
  if (trimmed === "") return void 0;
  if (trimmed === "true" || trimmed === "1") return true;
  if (trimmed === "false" || trimmed === "0") return false;
  return void 0;
}
function parseIntEnv(value) {
  if (value === void 0 || value === null) return void 0;
  const trimmed = value.trim();
  if (trimmed === "") return void 0;
  const parsed = parseInt(trimmed, 10);
  if (isNaN(parsed)) return void 0;
  return parsed;
}
function parseStringArrayEnv(value) {
  if (value === void 0 || value === null) return void 0;
  const trimmed = value.trim();
  if (trimmed === "") return void 0;
  return trimmed.split(",").map((s) => s.trim()).filter((s) => s.length > 0);
}
function resolveGitHub(fileConfig) {
  const envApiUrl = process.env.GITHUB_API_URL?.trim();
  return {
    apiUrl: envApiUrl || fileConfig?.apiUrl || DEFAULT_GITHUB_CONFIG.apiUrl
  };
}
function resolveLocal(fileConfig) {
  const isCli = getRuntimeSurface() === "cli";
  const envEnableLocal = parseBooleanEnv(process.env.ENABLE_LOCAL);
  const envEnableClone = parseBooleanEnv(process.env.ENABLE_CLONE);
  const envAllowedPaths = parseStringArrayEnv(process.env.ALLOWED_PATHS);
  const envWorkspaceRoot = process.env.WORKSPACE_ROOT?.trim() || void 0;
  return {
    // Local tools are on by default. ENABLE_LOCAL=false (or local.enabled=false)
    // is an explicit opt-out for users who want to remove the local surface.
    enabled: envEnableLocal ?? fileConfig?.enabled ?? DEFAULT_LOCAL_CONFIG.enabled,
    // Clone: an explicit ENABLE_CLONE (env) or .octocoderc value wins for both
    // surfaces, so `false` disables everywhere. Otherwise the default is
    // surface-specific: ENABLED for the CLI, DISABLED for the MCP server.
    enableClone: envEnableClone ?? fileConfig?.enableClone ?? (isCli ? true : DEFAULT_LOCAL_CONFIG.enableClone),
    allowedPaths: envAllowedPaths ?? fileConfig?.allowedPaths ?? DEFAULT_LOCAL_CONFIG.allowedPaths,
    workspaceRoot: envWorkspaceRoot ?? fileConfig?.workspaceRoot ?? DEFAULT_LOCAL_CONFIG.workspaceRoot
  };
}
function resolveTools(fileConfig) {
  const envToolsToRun = parseStringArrayEnv(process.env.TOOLS_TO_RUN);
  const envEnableTools = parseStringArrayEnv(process.env.ENABLE_TOOLS);
  const envDisableTools = parseStringArrayEnv(process.env.DISABLE_TOOLS);
  return {
    enabled: envToolsToRun ?? fileConfig?.enabled ?? DEFAULT_TOOLS_CONFIG.enabled,
    enableAdditional: envEnableTools ?? fileConfig?.enableAdditional ?? DEFAULT_TOOLS_CONFIG.enableAdditional,
    disabled: envDisableTools ?? fileConfig?.disabled ?? DEFAULT_TOOLS_CONFIG.disabled
  };
}
function resolveNetwork(fileConfig) {
  const envTimeout = parseIntEnv(process.env.REQUEST_TIMEOUT);
  const envMaxRetries = parseIntEnv(process.env.MAX_RETRIES);
  let timeout = envTimeout ?? fileConfig?.timeout ?? DEFAULT_NETWORK_CONFIG.timeout;
  timeout = Math.max(MIN_TIMEOUT, Math.min(MAX_TIMEOUT, timeout));
  let maxRetries = envMaxRetries ?? fileConfig?.maxRetries ?? DEFAULT_NETWORK_CONFIG.maxRetries;
  maxRetries = Math.max(MIN_RETRIES, Math.min(MAX_RETRIES, maxRetries));
  return { timeout, maxRetries };
}
function resolveLsp(fileConfig) {
  const envConfigPath = process.env.OCTOCODE_LSP_CONFIG?.trim() || void 0;
  return {
    configPath: envConfigPath ?? fileConfig?.configPath ?? DEFAULT_LSP_CONFIG.configPath
  };
}
var VALID_OUTPUT_FORMATS = /* @__PURE__ */ new Set(["yaml", "json"]);
function resolveOutput(fileConfig) {
  const envFormat = process.env.OCTOCODE_OUTPUT_FORMAT?.trim().toLowerCase();
  const envDefaultCharLength = parseIntEnv(
    process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH
  );
  const resolved = envFormat || fileConfig?.format || DEFAULT_OUTPUT_CONFIG.format;
  const configuredDefaultCharLength = envDefaultCharLength ?? fileConfig?.pagination?.defaultCharLength ?? DEFAULT_OUTPUT_CONFIG.pagination.defaultCharLength;
  const clampedDefaultCharLength = Math.max(
    MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
    Math.min(MAX_OUTPUT_DEFAULT_CHAR_LENGTH, configuredDefaultCharLength)
  );
  return {
    format: VALID_OUTPUT_FORMATS.has(resolved) ? resolved : DEFAULT_OUTPUT_CONFIG.format,
    pagination: {
      defaultCharLength: clampedDefaultCharLength
    }
  };
}

// src/config/resolverCache.ts
function buildResolvedConfig(fileConfig, configPath) {
  const hasFile = fileConfig !== void 0;
  const hasEnvOverrides = process.env.GITHUB_API_URL !== void 0 || process.env.ENABLE_LOCAL !== void 0 || process.env.ENABLE_CLONE !== void 0 || process.env.ALLOWED_PATHS !== void 0 || process.env.WORKSPACE_ROOT !== void 0 || process.env.TOOLS_TO_RUN !== void 0 || process.env.ENABLE_TOOLS !== void 0 || process.env.DISABLE_TOOLS !== void 0 || process.env.REQUEST_TIMEOUT !== void 0 || process.env.MAX_RETRIES !== void 0 || process.env.OCTOCODE_LSP_CONFIG !== void 0 || process.env.OCTOCODE_OUTPUT_FORMAT !== void 0 || process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH !== void 0;
  let source;
  if (hasFile && hasEnvOverrides) {
    source = "mixed";
  } else if (hasFile) {
    source = "file";
  } else {
    source = "defaults";
  }
  return {
    version: fileConfig?.version ?? DEFAULT_CONFIG.version,
    github: resolveGitHub(fileConfig?.github),
    local: resolveLocal(fileConfig?.local),
    tools: resolveTools(fileConfig?.tools),
    network: resolveNetwork(fileConfig?.network),
    lsp: resolveLsp(fileConfig?.lsp),
    output: resolveOutput(fileConfig?.output),
    source,
    configPath: hasFile ? configPath : void 0
  };
}
function resolveConfigSync() {
  const loadResult = loadConfigSync();
  if (loadResult.success && loadResult.config) {
    const validation = validateConfig(loadResult.config);
    if (!validation.valid) {
      return buildResolvedConfig(void 0);
    }
    return buildResolvedConfig(loadResult.config, loadResult.path);
  }
  return buildResolvedConfig(void 0);
}
async function resolveConfig() {
  return resolveConfigSync();
}
var cachedConfig = null;
var cacheTimestamp = 0;
var CACHE_TTL_MS = 6e4;
function getConfigSync() {
  const now = Date.now();
  if (cachedConfig && now - cacheTimestamp < CACHE_TTL_MS) {
    return cachedConfig;
  }
  cachedConfig = resolveConfigSync();
  cacheTimestamp = now;
  return cachedConfig;
}
async function getConfig() {
  return getConfigSync();
}
async function reloadConfig() {
  invalidateConfigCache();
  return getConfig();
}
function invalidateConfigCache() {
  cachedConfig = null;
  cacheTimestamp = 0;
}
function _resetConfigCache() {
  cachedConfig = null;
  cacheTimestamp = 0;
}
function _getCacheState() {
  return {
    cached: cachedConfig !== null,
    timestamp: cacheTimestamp
  };
}

// src/config/resolver.ts
function getConfigValue(keyPath) {
  const config = getConfigSync();
  const parts = keyPath.split(".");
  let current = config;
  for (const part of parts) {
    if (current === null || current === void 0 || typeof current !== "object") return void 0;
    current = current[part];
  }
  return current;
}

// src/tokens/envTokens.ts
var ENV_TOKEN_VARS = [
  "OCTOCODE_TOKEN",
  "GH_TOKEN",
  "GITHUB_TOKEN",
  "GITHUB_PERSONAL_ACCESS_TOKEN"
];
function getTokenFromEnv(env = process.env) {
  for (const envVar of ENV_TOKEN_VARS) {
    const token = env[envVar];
    if (token && token.trim()) return token.trim();
  }
  return null;
}
function getEnvTokenSource(env = process.env) {
  for (const envVar of ENV_TOKEN_VARS) {
    const token = env[envVar];
    if (token && token.trim()) return `env:${envVar}`;
  }
  return null;
}
function hasEnvToken(env = process.env) {
  return getTokenFromEnv(env) !== null;
}
function resolveEnvToken(env = process.env) {
  for (const envVar of ENV_TOKEN_VARS) {
    const token = env[envVar];
    if (token?.trim()) {
      return {
        token: token.trim(),
        source: `env:${envVar}`
      };
    }
  }
  return null;
}

// src/index.ts
var PROTECTED_KEYS = /* @__PURE__ */ new Set([
  "PATH",
  "HOME",
  "SHELL",
  "USER",
  "LOGNAME",
  "PWD",
  "TMPDIR",
  "NODE_OPTIONS",
  "OCTOCODE_TOKEN",
  "GH_TOKEN",
  "GITHUB_TOKEN",
  "GITHUB_PERSONAL_ACCESS_TOKEN",
  "PYTHON"
]);
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
function readTextIfExists(filePath) {
  try {
    return fs.readFileSync(filePath, "utf8");
  } catch {
    return "";
  }
}
function loadOctocodeEnv({
  home,
  cwd,
  trusted = false
} = {}) {
  const map = {};
  const sources = {};
  if (home) {
    for (const [k, v] of Object.entries(parseEnv(readTextIfExists(path3.join(home, ".env"))))) {
      map[k] = v;
      sources[k] = "global";
    }
  }
  if (cwd && trusted) {
    for (const [k, v] of Object.entries(
      parseEnv(readTextIfExists(path3.join(cwd, ".octocode", ".env")))
    )) {
      map[k] = v;
      sources[k] = "project";
    }
  }
  return { map, sources };
}
function applyOctocodeEnv(map, { env = process.env } = {}) {
  const applied = [];
  const skippedProtected = [];
  const skippedExisting = [];
  for (const [key, value] of Object.entries(map ?? {})) {
    if (PROTECTED_KEYS.has(key)) {
      skippedProtected.push(key);
      continue;
    }
    const existing = env[key];
    if (existing !== void 0 && existing !== "") {
      skippedExisting.push(key);
      continue;
    }
    env[key] = value;
    applied.push(key);
  }
  return { applied, skippedProtected, skippedExisting };
}
function propagateOctocodeEnv({
  home = getOctocodeHome(),
  cwd,
  trusted = false,
  env = process.env
} = {}) {
  const { map, sources } = loadOctocodeEnv({ home, cwd, trusted });
  const result = applyOctocodeEnv(map, { env });
  return { ...result, sources, keys: Object.keys(map) };
}
function loadOctocoderc(home = getOctocodeHome()) {
  const result = loadConfigSync(home);
  if (result.success) return result.config ? result.config : {};
  if (result.error && result.error !== "Config file does not exist") {
    process.stderr.write(`[octocode-config] Failed to parse .octocoderc: ${result.error}
`);
  }
  return {};
}
export {
  CONFIG_FILE_NAME,
  CONFIG_SCHEMA_VERSION,
  DEFAULT_CONFIG,
  DEFAULT_GITHUB_CONFIG,
  DEFAULT_LOCAL_CONFIG,
  DEFAULT_LSP_CONFIG,
  DEFAULT_NETWORK_CONFIG,
  DEFAULT_OUTPUT_CONFIG,
  DEFAULT_TOOLS_CONFIG,
  ENV_TOKEN_VARS,
  MAX_OUTPUT_DEFAULT_CHAR_LENGTH,
  MAX_RETRIES,
  MAX_TIMEOUT,
  MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
  MIN_RETRIES,
  MIN_TIMEOUT,
  PROTECTED_KEYS,
  _getCacheState,
  _resetConfigCache,
  _resetRuntimeSurface,
  applyOctocodeEnv,
  configExists,
  getConfig,
  getConfigFilePath,
  getConfigSync,
  getConfigValue,
  getEnvTokenSource,
  getOctocodeHome,
  getRuntimeSurface,
  getTokenFromEnv,
  hasEnvToken,
  invalidateConfigCache,
  loadConfig,
  loadConfigSync,
  loadOctocodeEnv,
  loadOctocoderc,
  parseBooleanEnv,
  parseEnv,
  parseIntEnv,
  parseStringArrayEnv,
  propagateOctocodeEnv,
  reloadConfig,
  resolveConfig,
  resolveConfigSync,
  resolveEnvToken,
  resolveGitHub,
  resolveLocal,
  resolveLsp,
  resolveNetwork,
  resolveOutput,
  resolveTools,
  setRuntimeSurface,
  validateConfig
};
//# sourceMappingURL=index.js.map
