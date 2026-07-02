import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { getOctocodeHome } from '../home.js';
import type { OctocodeConfig, LoadConfigResult } from './types.js';

// ─── JSON5-compatible parser ──────────────────────────────────────────────────
// Handles: line comments (//) · block comments (/* */) · trailing commas.
// State-machine based so // or /* inside string literals is never stripped.

function stripJson5Features(content: string): string {
  let result = '';
  let i = 0;
  let inString = false;
  let stringChar = '';

  while (i < content.length) {
    const char = content[i]!;
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
      if (char === '\\' && i + 1 < content.length) {
        result += content[i + 1]!;
        i += 2;
        continue;
      }
      if (char === stringChar) inString = false;
      i++;
      continue;
    }

    if (char === '/' && nextChar === '/') {
      while (i < content.length && content[i] !== '\n') i++;
      continue;
    }

    if (char === '/' && nextChar === '*') {
      i += 2;
      while (i < content.length - 1) {
        if (content[i] === '*' && content[i + 1] === '/') { i += 2; break; }
        i++;
      }
      continue;
    }

    result += char;
    i++;
  }

  return result.replace(/,(\s*[}\]])/g, '$1');
}

function parseJson5(content: string): unknown {
  return JSON.parse(stripJson5Features(content));
}

// ─── Path helpers ─────────────────────────────────────────────────────────────

/** Absolute path to the `.octocoderc` config file. */
export function getConfigFilePath(home: string = getOctocodeHome()): string {
  return path.join(home, '.octocoderc');
}

/** True when the `.octocoderc` file exists at the canonical location. */
export function configExists(home?: string): boolean {
  return existsSync(getConfigFilePath(home));
}

// ─── Loader ───────────────────────────────────────────────────────────────────

export function loadConfigSync(home?: string): LoadConfigResult {
  const filePath = getConfigFilePath(home);

  if (!existsSync(filePath)) {
    return { success: false, error: 'Config file does not exist', path: filePath };
  }

  try {
    const content = readFileSync(filePath, 'utf-8');

    if (!content.trim()) {
      return { success: true, config: {}, path: filePath };
    }

    const parsed = parseJson5(content);

    // Simple structural guard (replaces zod OctocodeConfigSchema — which was
    // z.looseObject, so this is semantically equivalent with zero deps).
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      return {
        success: false,
        error: 'Config file has invalid structure: must be a JSON object',
        path: filePath,
      };
    }

    return { success: true, config: parsed as OctocodeConfig, path: filePath };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return { success: false, error: `Failed to parse config file: ${message}`, path: filePath };
  }
}

export async function loadConfig(home?: string): Promise<LoadConfigResult> {
  return loadConfigSync(home);
}
