import {
  CONFIG_FIELDS,
  CONFIG_SCHEMA_VERSION,
  type ConfigFieldSpec,
} from './contract.generated.js';
import type { ValidationResult } from './types.js';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function getPath(root: unknown, fieldPath: string): unknown {
  let current = root;
  for (const part of fieldPath.split('.')) {
    if (!isRecord(current)) return undefined;
    current = current[part];
  }
  return current;
}

function isAbsoluteOrHomePath(value: string): boolean {
  return (
    value.startsWith('/') ||
    /^~(?:[\\/]|$)/.test(value) ||
    /^[A-Za-z]:[\\/]/.test(value)
  );
}

function hasTraversalSegment(value: string): boolean {
  return value.split(/[\\/]/).includes('..');
}

function validateUrl(fieldPath: string, value: string, errors: string[]): void {
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    errors.push(`${fieldPath}: Invalid URL format`);
    return;
  }
  if (!['http:', 'https:'].includes(parsed.protocol)) {
    errors.push(`${fieldPath}: Only http/https URLs allowed`);
  }
}

function validatePath(fieldPath: string, value: string, errors: string[]): void {
  if (value.trim() === '') {
    errors.push(`${fieldPath}: empty or whitespace-only path`);
  } else if (!isAbsoluteOrHomePath(value)) {
    errors.push(
      `${fieldPath}: must be absolute path (starting with /, ~/, or a Windows drive)`
    );
  } else if (hasTraversalSegment(value)) {
    errors.push(`${fieldPath}: must not contain '..' traversal segments`);
  }
}

function validateField(
  field: ConfigFieldSpec,
  value: unknown,
  errors: string[],
  warnings: string[]
): void {
  if (value === undefined) return;
  if (value === null && field.type !== 'schemaVersion') return;

  switch (field.type) {
    case 'schemaVersion':
      if (typeof value !== 'number' || !Number.isInteger(value)) {
        errors.push(`${field.path}: Must be an integer`);
      } else if (value > CONFIG_SCHEMA_VERSION) {
        warnings.push(
          `Configuration version ${value} is newer than supported version ${CONFIG_SCHEMA_VERSION}`
        );
      }
      return;
    case 'boolean':
      if (typeof value !== 'boolean') errors.push(`${field.path}: Must be a boolean`);
      return;
    case 'number':
      if (typeof value !== 'number' || !Number.isFinite(value)) {
        errors.push(`${field.path}: Must be a number`);
      } else if (
        (field.minimum !== undefined && value < field.minimum) ||
        (field.maximum !== undefined && value > field.maximum)
      ) {
        errors.push(
          `${field.path}: Must be between ${field.minimum} and ${field.maximum}`
        );
      }
      return;
    case 'stringArray':
      if (!Array.isArray(value)) {
        errors.push(`${field.path}: Must be an array`);
        return;
      }
      value.forEach((item, index) => {
        const itemPath = `${field.path}[${index}]`;
        if (typeof item !== 'string') {
          errors.push(`${itemPath}: Must be a string`);
        } else if (field.itemFormat === 'path') {
          validatePath(itemPath, item, errors);
        }
      });
      return;
    case 'enum':
      if (typeof value !== 'string') {
        errors.push(`${field.path}: Must be a string`);
      } else if (!field.values?.includes(value)) {
        const expected =
          field.enumStyle === 'quotedOr'
            ? field.values!.map(item => JSON.stringify(item)).join(' or ')
            : `one of: ${field.values!.join(', ')}`;
        errors.push(`${field.path}: Must be ${expected}`);
      }
      return;
    case 'url':
    case 'path':
    case 'string':
      if (typeof value !== 'string') {
        errors.push(`${field.path}: Must be a string`);
      } else if (field.type === 'url') {
        validateUrl(field.path, value, errors);
      } else if (field.type === 'path') {
        validatePath(field.path, value, errors);
      }
      return;
  }
}

function sectionPaths(): string[] {
  return [
    ...new Set(
      CONFIG_FIELDS.filter(field => field.file)
        .map(field => field.section)
        .filter(Boolean)
        .flatMap(section => {
          const parts = section.split('.');
          return parts.map((_, index) => parts.slice(0, index + 1).join('.'));
        })
    ),
  ];
}

function warnUnknownKeys(config: Record<string, unknown>, warnings: string[]): void {
  const sections = sectionPaths();
  const knownAt = new Map<string, Set<string>>([['', new Set(['$schema'])]]);
  for (const field of CONFIG_FIELDS.filter(candidate => candidate.file)) {
    const known = knownAt.get(field.section) ?? new Set<string>();
    known.add(field.key);
    knownAt.set(field.section, known);
  }
  for (const section of sections) {
    const parent = section.includes('.') ? section.slice(0, section.lastIndexOf('.')) : '';
    const key = section.slice(section.lastIndexOf('.') + 1);
    const known = knownAt.get(parent) ?? new Set<string>();
    known.add(key);
    knownAt.set(parent, known);
  }

  for (const [section, known] of knownAt) {
    const value = section === '' ? config : getPath(config, section);
    if (!isRecord(value)) continue;
    for (const key of Object.keys(value)) {
      if (!known.has(key)) {
        warnings.push(
          `Unknown configuration key: ${section.length === 0 ? key : `${section}.${key}`}`
        );
      }
    }
  }
}

export function validateConfig(config: unknown): ValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];
  if (!isRecord(config)) {
    return {
      valid: false,
      errors: ['Configuration must be an object'],
      warnings,
    };
  }

  for (const section of sectionPaths()) {
    const value = getPath(config, section);
    if (value !== undefined && value !== null && !isRecord(value)) {
      errors.push(`${section}: Must be an object`);
    }
  }

  for (const field of CONFIG_FIELDS.filter(candidate => candidate.file)) {
    const parent = field.section === '' ? config : getPath(config, field.section);
    if (parent === undefined || !isRecord(parent)) continue;
    validateField(field, parent[field.key], errors, warnings);
  }

  warnUnknownKeys(config, warnings);
  return { valid: errors.length === 0, errors, warnings };
}
