import type { OctocodeConfig, ValidationResult } from './types.js';
import { CONFIG_SCHEMA_VERSION } from './types.js';
import {
  MIN_TIMEOUT,
  MAX_TIMEOUT,
  MIN_RETRIES,
  MAX_RETRIES,
  MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
  MAX_OUTPUT_DEFAULT_CHAR_LENGTH,
} from './defaults.js';

function validateUrl(url: unknown, field: string): string | null {
  if (url === undefined || url === null) return null;

  if (typeof url !== 'string') {
    return `${field}: Must be a string`;
  }

  try {
    const parsed = new URL(url);
    if (!['http:', 'https:'].includes(parsed.protocol)) {
      return `${field}: Only http/https URLs allowed`;
    }
    return null;
  } catch {
    return `${field}: Invalid URL format`;
  }
}

function validateNumberRange(
  value: unknown,
  field: string,
  min: number,
  max: number
): string | null {
  if (value === undefined || value === null) return null;

  if (typeof value !== 'number' || isNaN(value)) {
    return `${field}: Must be a number`;
  }

  if (value < min || value > max) {
    return `${field}: Must be between ${min} and ${max}`;
  }

  return null;
}

function validateBoolean(value: unknown, field: string): string | null {
  if (value === undefined || value === null) return null;

  if (typeof value !== 'boolean') {
    return `${field}: Must be a boolean`;
  }

  return null;
}

function validateStringArray(value: unknown, field: string): string | null {
  if (value === undefined || value === null) return null;

  if (!Array.isArray(value)) {
    return `${field}: Must be an array`;
  }

  for (let i = 0; i < value.length; i++) {
    if (typeof value[i] !== 'string') {
      return `${field}[${i}]: Must be a string`;
    }
  }

  return null;
}

// Rejects empty strings, relative paths, and path traversal attempts.
function validateAllowedPathElements(paths: unknown[]): string[] {
  const errors: string[] = [];
  for (let i = 0; i < paths.length; i++) {
    const p = paths[i];
    if (typeof p !== 'string') continue;
    if (p.trim() === '') {
      errors.push(`local.allowedPaths[${i}]: empty or whitespace-only path`);
    } else if (!p.startsWith('/') && !p.startsWith('~')) {
      errors.push(
        `local.allowedPaths[${i}]: must be absolute path or start with ~ (got "${p}")`
      );
    } else if (p.includes('..')) {
      errors.push(
        `local.allowedPaths[${i}]: path traversal (..) not allowed (got "${p}")`
      );
    }
  }
  return errors;
}

function validateNullableStringArray(
  value: unknown,
  field: string
): string | null {
  if (value === undefined) return null;
  if (value === null) return null;

  return validateStringArray(value, field);
}

function validateString(value: unknown, field: string): string | null {
  if (value === undefined || value === null) return null;

  if (typeof value !== 'string') {
    return `${field}: Must be a string`;
  }

  return null;
}

function validateGitHub(github: unknown, errors: string[]): void {
  if (github === undefined || github === null) return;

  if (typeof github !== 'object' || Array.isArray(github)) {
    errors.push('github: Must be an object');
    return;
  }

  const gh = github as Record<string, unknown>;

  const apiUrlError = validateUrl(gh.apiUrl, 'github.apiUrl');
  if (apiUrlError) errors.push(apiUrlError);
}

function validateLocal(local: unknown, errors: string[]): void {
  if (local === undefined || local === null) return;

  if (typeof local !== 'object' || Array.isArray(local)) {
    errors.push('local: Must be an object');
    return;
  }

  const loc = local as Record<string, unknown>;

  const enabledError = validateBoolean(loc.enabled, 'local.enabled');
  if (enabledError) errors.push(enabledError);

  const enableCloneError = validateBoolean(
    loc.enableClone,
    'local.enableClone'
  );
  if (enableCloneError) errors.push(enableCloneError);

  const allowedPathsError = validateStringArray(
    loc.allowedPaths,
    'local.allowedPaths'
  );
  if (allowedPathsError) {
    errors.push(allowedPathsError);
  } else if (Array.isArray(loc.allowedPaths)) {
    const pathErrors = validateAllowedPathElements(
      loc.allowedPaths as unknown[]
    );
    errors.push(...pathErrors);
  }

  if (loc.workspaceRoot !== undefined && loc.workspaceRoot !== null) {
    const workspaceRootError = validateString(
      loc.workspaceRoot,
      'local.workspaceRoot'
    );
    if (workspaceRootError) {
      errors.push(workspaceRootError);
    } else if (
      typeof loc.workspaceRoot === 'string' &&
      !loc.workspaceRoot.startsWith('/') &&
      !loc.workspaceRoot.startsWith('~')
    ) {
      errors.push(
        'local.workspaceRoot: must be an absolute path or start with ~ (got "' +
          loc.workspaceRoot +
          '")'
      );
    } else if (
      typeof loc.workspaceRoot === 'string' &&
      loc.workspaceRoot.includes('..')
    ) {
      errors.push(
        'local.workspaceRoot: path traversal (..) not allowed (got "' +
          loc.workspaceRoot +
          '")'
      );
    }
  }
}

function validateTools(tools: unknown, errors: string[]): void {
  if (tools === undefined || tools === null) return;

  if (typeof tools !== 'object' || Array.isArray(tools)) {
    errors.push('tools: Must be an object');
    return;
  }

  const t = tools as Record<string, unknown>;

  const enabledError = validateNullableStringArray(t.enabled, 'tools.enabled');
  if (enabledError) errors.push(enabledError);

  const enableAdditionalError = validateNullableStringArray(
    t.enableAdditional,
    'tools.enableAdditional'
  );
  if (enableAdditionalError) errors.push(enableAdditionalError);

  const disabledError = validateNullableStringArray(
    t.disabled,
    'tools.disabled'
  );
  if (disabledError) errors.push(disabledError);
}

function validateNetwork(network: unknown, errors: string[]): void {
  if (network === undefined || network === null) return;

  if (typeof network !== 'object' || Array.isArray(network)) {
    errors.push('network: Must be an object');
    return;
  }

  const net = network as Record<string, unknown>;

  const timeoutError = validateNumberRange(
    net.timeout,
    'network.timeout',
    MIN_TIMEOUT,
    MAX_TIMEOUT
  );
  if (timeoutError) errors.push(timeoutError);

  const retriesError = validateNumberRange(
    net.maxRetries,
    'network.maxRetries',
    MIN_RETRIES,
    MAX_RETRIES
  );
  if (retriesError) errors.push(retriesError);
}

function validateTelemetry(telemetry: unknown, errors: string[]): void {
  if (telemetry === undefined || telemetry === null) return;

  if (typeof telemetry !== 'object' || Array.isArray(telemetry)) {
    errors.push('telemetry: Must be an object');
    return;
  }

  const tel = telemetry as Record<string, unknown>;

  const loggingError = validateBoolean(tel.logging, 'telemetry.logging');
  if (loggingError) errors.push(loggingError);
}

function validateLsp(lsp: unknown, errors: string[]): void {
  if (lsp === undefined || lsp === null) return;

  if (typeof lsp !== 'object' || Array.isArray(lsp)) {
    errors.push('lsp: Must be an object');
    return;
  }

  const l = lsp as Record<string, unknown>;

  const configPathError = validateString(l.configPath, 'lsp.configPath');
  if (configPathError) errors.push(configPathError);
}

function validateOutput(output: unknown, errors: string[]): void {
  if (output === undefined || output === null) return;

  if (typeof output !== 'object' || Array.isArray(output)) {
    errors.push('output: Must be an object');
    return;
  }

  const out = output as Record<string, unknown>;

  if (out.format !== undefined) {
    if (typeof out.format !== 'string') {
      errors.push('output.format: Must be a string');
    } else if (!['yaml', 'json'].includes(out.format)) {
      errors.push('output.format: Must be one of: yaml, json');
    }
  }

  if (out.pagination !== undefined && out.pagination !== null) {
    if (typeof out.pagination !== 'object' || Array.isArray(out.pagination)) {
      errors.push('output.pagination: Must be an object');
    } else {
      const pagination = out.pagination as Record<string, unknown>;
      const defaultCharLengthError = validateNumberRange(
        pagination.defaultCharLength,
        'output.pagination.defaultCharLength',
        MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
        MAX_OUTPUT_DEFAULT_CHAR_LENGTH
      );
      if (defaultCharLengthError) errors.push(defaultCharLengthError);
    }
  }
}

export function validateConfig(config: unknown): ValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];

  if (typeof config !== 'object' || config === null || Array.isArray(config)) {
    return {
      valid: false,
      errors: ['Configuration must be a JSON object'],
      warnings: [],
    };
  }

  const cfg = config as Record<string, unknown>;

  if (cfg.version !== undefined) {
    if (typeof cfg.version !== 'number' || !Number.isInteger(cfg.version)) {
      errors.push('version: Must be an integer');
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
  validateTelemetry(cfg.telemetry, errors);
  validateLsp(cfg.lsp, errors);
  validateOutput(cfg.output, errors);

  const knownKeys = new Set([
    '$schema',
    'version',
    'github',
    'local',
    'tools',
    'network',
    'telemetry',
    'lsp',
    'output',
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
    config: errors.length === 0 ? (config as OctocodeConfig) : undefined,
  };
}
