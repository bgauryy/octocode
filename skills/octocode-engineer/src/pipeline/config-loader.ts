import fs from 'node:fs';
import path from 'node:path';

import type { AnalysisOptions } from '../types/index.js';

type ConfigOverrides = Partial<Omit<AnalysisOptions, 'root' | 'packageRoot' | 'clearCache'>>;

const CONFIG_NAMES = ['.octocode-scan.json', '.octocode-scan.jsonc'];

export function loadConfigFile(
  root: string,
  explicitPath: string | null
): ConfigOverrides | null {
  if (explicitPath) {
    const abs = path.isAbsolute(explicitPath)
      ? explicitPath
      : path.resolve(root, explicitPath);
    return readJsonConfig(abs);
  }

  for (const name of CONFIG_NAMES) {
    const candidate = path.join(root, name);
    if (fs.existsSync(candidate)) {
      return readJsonConfig(candidate);
    }
  }

  const pkgJsonPath = path.join(root, 'package.json');
  if (fs.existsSync(pkgJsonPath)) {
    try {
      const pkg = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf8'));
      if (pkg.octocode && typeof pkg.octocode === 'object') {
        return normalizeConfig(pkg.octocode);
      }
    } catch { /* skip */ }
  }

  return null;
}

function readJsonConfig(filePath: string): ConfigOverrides | null {
  try {
    let raw = fs.readFileSync(filePath, 'utf8');
    raw = raw.replace(/\/\/.*$/gm, '').replace(/\/\*[\s\S]*?\*\//g, '');
    return normalizeConfig(JSON.parse(raw));
  } catch {
    return null;
  }
}

function normalizeConfig(obj: Record<string, unknown>): ConfigOverrides {
  const result: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(obj)) {
    const camelKey = key.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());

    if (camelKey === 'features' && typeof value === 'string') {
      result[camelKey] = new Set(value.split(',').map(s => s.trim()));
    } else if (camelKey === 'scope' && typeof value === 'string') {
      result[camelKey] = value.split(',').map(s => s.trim());
    } else if (camelKey === 'ignoreDirs' && Array.isArray(value)) {
      result[camelKey] = new Set(value as string[]);
    } else if (camelKey === 'thresholds' && typeof value === 'object' && value !== null) {
      result[camelKey] = value;
    } else {
      result[camelKey] = value;
    }
  }

  return result as ConfigOverrides;
}

export function mergeConfigIntoDefaults(
  defaults: AnalysisOptions,
  config: ConfigOverrides,
  cliArgs: AnalysisOptions
): AnalysisOptions {
  const merged = { ...defaults };

  for (const [key, value] of Object.entries(config)) {
    if (key === 'thresholds' && typeof value === 'object' && value !== null) {
      merged.thresholds = {
        ...merged.thresholds,
        ...(value as unknown as Record<string, number>),
      };
    } else {
      (merged as Record<string, unknown>)[key] = value;
    }
  }

  for (const key of Object.keys(cliArgs)) {
    const cliVal = (cliArgs as unknown as Record<string, unknown>)[key];
    const defVal = (defaults as unknown as Record<string, unknown>)[key];
    if (cliVal !== defVal) {
      (merged as unknown as Record<string, unknown>)[key] = cliVal;
    }
  }

  return merged;
}
