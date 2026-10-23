// The usage line of a command: each flag as a shell token, required flags first.
import type { Flag } from './spec.js';

export function shellWord(value: string): string {
  if (value.length > 0 && /^[A-Za-z0-9_./:@+=,-]+$/.test(value)) return value;
  return `"${value.replace(/(["\\$`])/g, '\\$&')}"`;
}

export function flagToken(flag: Flag): string {
  if (flag.presence) return `--${flag.name}`;
  if (flag.kind === 'enum') {
    const choices = (flag.enumValues ?? []).map(value => shellWord(String(value))).join('|');
    return `--${flag.name} <${choices}>`;
  }
  if (flag.kind === 'array') return `--${flag.name} <${flag.itemKind ?? 'json'}>`;
  if (flag.kind === 'json') return `--${flag.name} <json>`;
  if (flag.kind === 'boolean') return `--${flag.name} <true|false>`;
  return `--${flag.name} <${flag.kind}>`;
}

export function usagePattern(bin: string, command: string, flags: readonly Flag[]): string {
  const ordered = [...flags.filter(flag => flag.required), ...flags.filter(flag => !flag.required)];
  const parts = [`${bin} ${command}`];
  for (const flag of ordered) {
    const token = flagToken(flag);
    parts.push(flag.required ? token : `[${token}]`);
  }
  return parts.join(' ');
}
