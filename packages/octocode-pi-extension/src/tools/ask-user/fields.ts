export interface AskField {
  name: string;
  label?: string;
  placeholder?: string;
  required?: boolean;
  minLength?: number;
  maxLength?: number;
  pattern?: string;
}

export function normalizeFields(raw: AskField[] | undefined): AskField[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((f): f is AskField => Boolean(f && typeof f.name === 'string' && f.name.length > 0))
    .map((f) => ({
      name: f.name,
      label: f.label,
      placeholder: f.placeholder,
      required: f.required === true,
      minLength: typeof f.minLength === 'number' && Number.isFinite(f.minLength) ? Math.max(0, Math.floor(f.minLength)) : undefined,
      maxLength: typeof f.maxLength === 'number' && Number.isFinite(f.maxLength) ? Math.max(1, Math.floor(f.maxLength)) : undefined,
      pattern: typeof f.pattern === 'string' && f.pattern.length > 0 ? f.pattern : undefined,
    }));
}

export function validateUniqueFieldNames(fields: readonly AskField[]): void {
  const seen = new Set<string>();
  for (const field of fields) {
    if (seen.has(field.name)) throw new Error(`field names must be unique; duplicate "${field.name}".`);
    seen.add(field.name);
  }
}

export function validateFieldValue(field: AskField, raw: string): string | undefined {
  const label = field.label || field.name;
  const value = raw.trim();
  if (field.required && !value) return `${label} is required.`;
  if (field.minLength !== undefined && value.length < field.minLength) return `${label} must be at least ${field.minLength} character${field.minLength === 1 ? '' : 's'}.`;
  if (field.maxLength !== undefined && value.length > field.maxLength) return `${label} must be at most ${field.maxLength} character${field.maxLength === 1 ? '' : 's'}.`;
  if (field.pattern) {
    try {
      if (!new RegExp(field.pattern).test(value)) return `${label} has the wrong format.`;
    } catch {
      return `${label} validation pattern is invalid.`;
    }
  }
  return undefined;
}
