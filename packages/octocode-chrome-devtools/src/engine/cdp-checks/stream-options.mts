// Typed recipe options arrive as environment variables: booleans `1`/`0`, arrays as JSON.
export function flagOption(name: string, fallback: boolean): boolean {
  const value = process.env[name];
  if (value === undefined || value === '') return fallback;
  if (value === '1' || value === 'true') return true;
  if (value === '0' || value === 'false') return false;
  throw new Error(`${name} must be 1 or 0`);
}

export function textOption(name: string): string | undefined {
  return process.env[name] || undefined;
}

export function countOption(name: string): number | undefined {
  const value = process.env[name];
  if (value === undefined || value === '') return undefined;
  const count = Number(value);
  if (!Number.isFinite(count) || count < 0)
    throw new Error(`${name} must be a non-negative number`);
  return count;
}

export function stringsOption(name: string): string[] | undefined {
  const value = process.env[name];
  if (value === undefined || value === '') return undefined;
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    throw new Error(`${name} must be a JSON array of strings`);
  }
  if (!Array.isArray(parsed) || parsed.some(item => typeof item !== 'string'))
    throw new Error(`${name} must be a JSON array of strings`);
  return parsed;
}

export const sleep = (ms: number) =>
  new Promise<void>(resolve => setTimeout(resolve, ms));

/** One line of the check's stdout protocol. */
export const say = (line: string) => process.stdout.write(`${line}\n`);
