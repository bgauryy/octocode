import { existsSync, readFileSync } from 'node:fs';
import { paths } from '../paths.js';
import type { OctocodeConfig, LoadConfigResult } from './types.js';
import { OctocodeConfigSchema } from './schemas.js';

export const CONFIG_FILE_PATH = paths.config;

function stripJson5Features(content: string): string {
  let result = '';
  let i = 0;
  let inString = false;
  let stringChar = '';

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
      if (char === '\\' && i + 1 < content.length) {
        result += content[i + 1];
        i += 2;
        continue;
      }
      if (char === stringChar) {
        inString = false;
      }
      i++;
      continue;
    }

    if (char === '/' && nextChar === '/') {
      while (i < content.length && content[i] !== '\n') {
        i++;
      }
      continue;
    }

    if (char === '/' && nextChar === '*') {
      i += 2;
      while (i < content.length - 1) {
        if (content[i] === '*' && content[i + 1] === '/') {
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

  result = result.replace(/,(\s*[}\]])/g, '$1');

  return result;
}

function parseJson5(content: string): unknown {
  const jsonContent = stripJson5Features(content);
  return JSON.parse(jsonContent);
}

export function configExists(): boolean {
  return existsSync(CONFIG_FILE_PATH);
}

export async function loadConfig(): Promise<LoadConfigResult> {
  return loadConfigSync();
}

export function loadConfigSync(): LoadConfigResult {
  const path = CONFIG_FILE_PATH;

  if (!existsSync(path)) {
    return {
      success: false,
      error: 'Config file does not exist',
      path,
    };
  }

  try {
    const content = readFileSync(path, 'utf-8');

    if (!content.trim()) {
      return {
        success: true,
        config: {},
        path,
      };
    }

    const parsed = parseJson5(content);

    const result = OctocodeConfigSchema.safeParse(parsed);
    if (!result.success) {
      return {
        success: false,
        error: `Config file has invalid structure: ${result.error.issues[0]?.message ?? 'unknown error'}`,
        path,
      };
    }

    return {
      success: true,
      config: result.data as OctocodeConfig,
      path,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Unknown error';
    return {
      success: false,
      error: `Failed to parse config file: ${message}`,
      path,
    };
  }
}

export function getConfigPath(): string {
  return CONFIG_FILE_PATH;
}

export function getOctocodeDir(): string {
  return paths.home;
}
