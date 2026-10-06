import { commandHelp, commandHelpJson, defaultOutput, rootHelp, rootHelpJson, stripAnsi } from './help.js';
import { parseCommandInput, type CliCommand, type CliSpec, type Flag } from './spec.js';

export interface CliIo {
  stdout: (text: string) => void;
  stderr: (text: string) => void;
}

function writeStdout(text: string): void {
  process.stdout.write(text);
}

function writeStderr(text: string): void {
  process.stderr.write(text);
}

function jsonText(value: unknown): string {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function isNegativeNumber(flag: Flag, token: string): boolean {
  const numeric = flag.kind === 'number'
    || flag.kind === 'integer'
    || (flag.kind === 'array' && (flag.itemKind === 'number' || flag.itemKind === 'integer'));
  return numeric && /^-?(?:\d+|\d*\.\d+)$/.test(token);
}

function coerce(flag: Flag, raw: string, itemKind: Flag['kind'] | Flag['itemKind']): { ok: true; value: unknown } | { ok: false; message: string } {
  if (itemKind === 'string') return { ok: true, value: raw };
  if (itemKind === 'integer') {
    if (!/^-?\d+$/.test(raw)) return { ok: false, message: `Invalid integer for --${flag.name}: ${raw}` };
    return { ok: true, value: Number(raw) };
  }
  if (itemKind === 'number') {
    if (!/^-?(?:\d+|\d*\.\d+)$/.test(raw) || !Number.isFinite(Number(raw))) {
      return { ok: false, message: `Invalid number for --${flag.name}: ${raw}` };
    }
    return { ok: true, value: Number(raw) };
  }
  if (itemKind === 'boolean') {
    if (raw !== 'true' && raw !== 'false') return { ok: false, message: `Invalid boolean for --${flag.name}: ${raw}` };
    return { ok: true, value: raw === 'true' };
  }
  if (itemKind === 'enum') {
    const match = (flag.enumValues ?? []).find(value => String(value) === raw);
    if (match === undefined) return { ok: false, message: `Invalid value for --${flag.name}: ${raw}` };
    return { ok: true, value: match };
  }
  try {
    return { ok: true, value: JSON.parse(raw) };
  } catch {
    return { ok: false, message: `Invalid JSON for --${flag.name}` };
  }
}

const CLI_VIEW = Symbol.for('octocode-mcp-cli.view');

export interface CliView {
  text: string;
  json: unknown;
  readonly [CLI_VIEW]: true;
}

export function cliView(text: string, json: unknown): CliView {
  return { text, json, [CLI_VIEW]: true };
}

export function isCliView(value: unknown): value is CliView {
  return Boolean(value) && typeof value === 'object' && CLI_VIEW in (value as object);
}

function isScalar(value: unknown): boolean {
  return value === null || value === undefined || typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

/** Full text of MCP content blocks. Plain text stays text. Every other block is its JSON. */
export function contentText(content: readonly unknown[]): string {
  return content.map(block => {
    if (!isRecord(block)) return String(block);
    const plain = block.type === 'text'
      && typeof block.text === 'string'
      && Object.keys(block).every(key => key === 'type' || key === 'text');
    return plain ? block.text : JSON.stringify(block);
  }).join('\n');
}

function scalarText(value: unknown): string {
  if (value === null) return 'null';
  if (value === undefined) return '';
  return String(value);
}

function scalarColumns(rows: readonly Record<string, unknown>[]): boolean {
  let keys = 0;
  for (const row of rows) {
    for (const value of Object.values(row)) {
      keys += 1;
      if (!isScalar(value)) return false;
    }
  }
  return keys > 0;
}

function tableLines(rows: readonly Record<string, unknown>[], indent: number): string[] {
  const keys: string[] = [];
  const seen = new Set<string>();
  for (const row of rows) {
    for (const key of Object.keys(row)) {
      if (seen.has(key)) continue;
      seen.add(key);
      keys.push(key);
    }
  }
  const cells = rows.map(row => keys.map(key => scalarText(row[key])));
  const widths = keys.map((key, index) => Math.max(key.length, ...cells.map(row => row[index]?.length ?? 0)));
  const pad = ' '.repeat(indent);
  const render = (cols: string[]) => {
    const shown = cols.map((col, index) => (index === cols.length - 1 ? col : col.padEnd(widths[index] ?? 0)));
    return `${pad}${shown.join('  ')}`;
  };
  return [render(keys), ...cells.map(render)];
}

function formatRecord(record: Record<string, unknown>, indent: number): string[] {
  const pad = ' '.repeat(indent);
  const lines: string[] = [];
  for (const [key, child] of Object.entries(record)) {
    if (child === undefined) continue;
    if (isScalar(child)) {
      lines.push(`${pad}${key}: ${scalarText(child)}`);
      continue;
    }
    if (Array.isArray(child) && (child.length === 0 || child.every(isScalar))) {
      const text = child.length === 0 ? '(none)' : child.map(scalarText).join(', ');
      lines.push(`${pad}${key}: ${text}`);
      continue;
    }
    lines.push(`${pad}${key}:`);
    lines.push(...formatValue(child, indent + 2));
  }
  return lines;
}

function formatValue(value: unknown, indent: number): string[] {
  const pad = ' '.repeat(indent);
  if (isScalar(value)) return [`${pad}${scalarText(value)}`];
  if (Array.isArray(value)) {
    if (value.length === 0) return [`${pad}(none)`];
    if (value.every(isScalar)) return [`${pad}${value.map(scalarText).join(', ')}`];
    if (value.length > 1 && value.every(isRecord) && scalarColumns(value)) return tableLines(value, indent);
    if (value.every(isRecord)) {
      return value.flatMap((item, index) => {
        const lines = formatRecord(item, indent);
        return index === 0 ? lines : ['', ...lines];
      });
    }
    return value.flatMap(item => formatValue(item, indent));
  }
  if (isRecord(value)) return formatRecord(value, indent);
  return [`${pad}${String(value)}`];
}

function humanText(value: unknown): string {
  const text = formatValue(value, 0).join('\n');
  if (text === '') return '\n';
  return text.endsWith('\n') ? text : `${text}\n`;
}

function renderResult(result: unknown, asJson: boolean, noColor: boolean): string {
  const view = isCliView(result) ? result : undefined;
  let text: string;
  if (asJson) {
    const value = view ? view.json : result;
    text = jsonText(value === undefined ? null : value);
  } else if (view) {
    const body = view.json;
    if (isRecord(body) && Object.prototype.hasOwnProperty.call(body, 'structuredContent')) {
      const structured = humanText(body.structuredContent);
      const content = Array.isArray(body.content) && body.content.length > 0 ? contentText(body.content) : '';
      text = content.length === 0
        ? structured
        : `${structured.endsWith('\n') ? structured : `${structured}\n`}${content.endsWith('\n') ? content : `${content}\n`}`;
    } else {
      text = view.text.endsWith('\n') ? view.text : `${view.text}\n`;
    }
  } else if (typeof result === 'string') {
    text = result.endsWith('\n') ? result : `${result}\n`;
  } else if (result === undefined) {
    text = '\n';
  } else {
    text = humanText(result);
  }
  return noColor ? stripAnsi(text) : text;
}

export async function runCli(spec: CliSpec, argv: readonly string[], io?: CliIo): Promise<number> {
  const stdout = io?.stdout ?? writeStdout;
  const stderr = io?.stderr ?? writeStderr;
  const fail = (message: string): number => {
    stderr(message.endsWith('\n') ? message : `${message}\n`);
    return 2;
  };

  const wantsHelp = argv.some(token => token === '--help' || token === '-h');
  const asJson = argv.includes('--json');
  if (wantsHelp) {
    const positionals = argv.filter(token => !token.startsWith('-'));
    const head = positionals[0] === 'help' ? positionals[1] : positionals[0];
    const command = head ? spec.commands.find(item => item.name === head) : undefined;
    if (head && !command) return fail(`Unknown command: ${head}`);
    if (command) stdout(asJson ? jsonText(commandHelpJson(spec, command)) : commandHelp(spec, command));
    else stdout(asJson ? jsonText(rootHelpJson(spec)) : rootHelp(spec));
    return 0;
  }
  if (argv.includes('--version')) {
    stdout(`${spec.version}\n`);
    return 0;
  }

  let commandName: string | undefined;
  const rest: string[] = [];
  for (const token of argv) {
    if (!commandName && !token.startsWith('-')) {
      commandName = token;
      continue;
    }
    rest.push(token);
  }
  if (!commandName || commandName === 'help') {
    const subject = commandName === 'help' ? rest.find(token => !token.startsWith('-')) : undefined;
    if (commandName === 'help' && subject) {
      const command = spec.commands.find(item => item.name === subject);
      if (!command) return fail(`Unknown command: ${subject}`);
      stdout(asJson ? jsonText(commandHelpJson(spec, command)) : commandHelp(spec, command));
      return 0;
    }
    for (const token of rest) {
      if (token.startsWith('-') && token !== '--json' && token !== '--no-input' && token !== '--no-color') {
        return fail(`Unknown flag: ${token.split('=')[0]}`);
      }
    }
    stdout(asJson ? jsonText(rootHelpJson(spec)) : commandName === 'help' ? rootHelp(spec) : defaultOutput(spec));
    return 0;
  }

  const command = spec.commands.find(item => item.name === commandName);
  if (!command) return fail(`Unknown command: ${commandName}`);
  const parsed = parseFlags(command, rest);
  if (parsed.error) return fail(parsed.error);
  const missing = command.flags.filter(flag => flag.required && !parsed.present.has(flag.property));
  if (missing.length > 0) {
    stdout(`missing required flag: ${missing.map(flag => `--${flag.name}`).join(', ')}\n${commandHelp(spec, command)}`);
    return 2;
  }
  let input: Record<string, unknown>;
  try {
    input = parseCommandInput(command, parsed.values);
  } catch (error) {
    stderr(`${error instanceof Error ? error.message : String(error)}\n`);
    return 2;
  }
  try {
    const result = await command.run(input);
    if (result !== undefined) stdout(renderResult(result, parsed.json, parsed.noColor));
    return 0;
  } catch (error) {
    stderr(`${error instanceof Error ? error.message : String(error)}\n`);
    if (error instanceof Error && 'exitCode' in error && typeof error.exitCode === 'number' && Number.isInteger(error.exitCode) && error.exitCode > 0 && error.exitCode < 256) return error.exitCode;
    return 1;
  }
}

interface FlagParse {
  values: Record<string, unknown>;
  present: Set<string>;
  json: boolean;
  noColor: boolean;
  error?: string;
}

function parseFlags(command: CliCommand, rest: readonly string[]): FlagParse {
  const values: Record<string, unknown> = {};
  const present = new Set<string>();
  let json = false;
  let noColor = false;
  for (let index = 0; index < rest.length; index += 1) {
    const token = rest[index];
    if (token === undefined) break;
    if (token === '--') {
      const extra = rest[index + 1];
      if (extra !== undefined) return { values, present, json, noColor, error: `Unexpected argument: ${extra}` };
      break;
    }
    if (!token.startsWith('-')) {
      return { values, present, json, noColor, error: `Unexpected argument: ${token}` };
    }
    if (!token.startsWith('--')) {
      return { values, present, json, noColor, error: `Unknown flag: ${token}` };
    }
    const eq = token.indexOf('=');
    const body = eq === -1 ? token.slice(2) : token.slice(2, eq);
    const inline = eq === -1 ? undefined : token.slice(eq + 1);
    if (body === 'json' || body === 'no-input' || body === 'no-color') {
      if (inline !== undefined) return { values, present, json, noColor, error: `Flag --${body} does not take a value` };
      if (body === 'json') json = true;
      if (body === 'no-color') noColor = true;
      continue;
    }
    const flag = command.flags.find(item => item.name === body);
    if (!flag) return { values, present, json, noColor, error: `Unknown flag: --${body}` };
    if (flag.presence) {
      if (inline !== undefined) return { values, present, json, noColor, error: `Flag --${flag.name} does not take a value` };
      values[flag.property] = true;
      present.add(flag.property);
      continue;
    }
    const taken = takeValue(flag, inline, rest, index);
    if (taken.error !== undefined || taken.value === undefined) {
      return { values, present, json, noColor, error: taken.error ?? `Missing value for --${flag.name}` };
    }
    index = taken.next;
    if (flag.kind === 'array' && taken.value.startsWith('[')) {
      try {
        const list: unknown = JSON.parse(taken.value);
        if (Array.isArray(list)) {
          values[flag.property] = [...(Array.isArray(values[flag.property]) ? values[flag.property] as unknown[] : []), ...list];
          present.add(flag.property);
          continue;
        }
      } catch { /* A string array item may begin with '['; coerce it normally. */ }
    }
    const kind = flag.kind === 'array' ? flag.itemKind : flag.kind;
    const coerced = coerce(flag, taken.value, kind);
    if (!coerced.ok) return { values, present, json, noColor, error: coerced.message };
    if (flag.kind === 'array') {
      const list = Array.isArray(values[flag.property]) ? values[flag.property] as unknown[] : [];
      list.push(coerced.value);
      values[flag.property] = list;
    } else {
      values[flag.property] = coerced.value;
    }
    present.add(flag.property);
  }
  return { values, present, json, noColor };
}

function takeValue(
  flag: Flag,
  inline: string | undefined,
  rest: readonly string[],
  index: number,
): { value: string; next: number; error?: undefined } | { error: string; value?: undefined; next: number } {
  if (inline !== undefined) return { value: inline, next: index };
  const next = rest[index + 1];
  if (next === undefined || (next.startsWith('-') && !isNegativeNumber(flag, next))) {
    return { error: `Missing value for --${flag.name}`, next: index };
  }
  return { value: next, next: index + 1 };
}
