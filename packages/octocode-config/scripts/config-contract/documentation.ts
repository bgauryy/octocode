// Renders the settings table inside docs/CONFIGURATION.md.
import { type FlatField, flattenFields, ownDefault } from './model.ts';
import type { Contract } from './schema.ts';

function markdownDefault(field: FlatField): string {
  if (field.displayDefault) return `\`${field.displayDefault}\``;
  const value = ownDefault(field);
  if (value === null) return 'unset';
  return `\`${typeof value === 'string' ? value : JSON.stringify(value)}\``;
}

/** Whether a workspace file may set this field: `yes`, `no`, or the one value that narrows it. */
function workspacePolicy(field: FlatField): string {
  if (field.env?.dotenv !== 'home' && field.env?.dotenv !== 'never') return 'yes';
  return field.env.workspaceNarrowsTo === undefined ? 'no' : `only \`${field.env.workspaceNarrowsTo}\``;
}

/** A setting's type as a reader thinks of it: a secret is a key, whatever holds it. */
function readerType(field: FlatField): string {
  if (field.credential) return 'key';
  if (field.type === 'stringArray') return 'list';
  if (field.type === 'enum') return (field.values ?? []).join(' \\| ');
  return field.type;
}

const code = (text: string): string => `\`${text}\``;
const yesNo = (value: unknown): string => (value ? 'yes' : 'no');
const cell = (text: string): string => text.replace(/\|/g, '\\|');
const withLink = (text: string, url?: string): string => (url ? `${text}; [get a key](${url})` : text);

const SETTINGS_START = '<!-- settings-table:start (generated from the config contract; do not edit) -->';
const SETTINGS_END = '<!-- settings-table:end -->';

/** `page` with its generated settings table replaced by the contract's current one. */
export function renderConfigurationPage(contract: Contract, page: string): string {
  const start = page.indexOf(SETTINGS_START);
  const end = page.indexOf(SETTINGS_END);
  if (start < 0 || end < start) throw new Error('docs/CONFIGURATION.md lost its settings-table markers.');
  const settingRows = flattenFields(contract)
    .map(field => `| ${code(field.path)} | ${field.env ? code(field.env.name) : '—'} | ${readerType(field)} | ${withLink(markdownDefault(field), field.docsUrl)} | ${yesNo(field.credential)} | ${workspacePolicy(field)} | ${cell([field.description, field.notes].filter(Boolean).join(' '))} |`);
  const environmentRows = Object.entries(contract.environment)
    .filter(([, entry]) => entry.type)
    .map(([name, entry]) => `| — | ${code(name)} | ${entry.type} | ${withLink(code(entry.displayDefault ?? 'unset'), entry.docsUrl)} | ${yesNo(entry.credential)} | ${entry.dotenv === 'home' || entry.dotenv === 'never' ? 'no' : 'yes'} | ${cell(entry.description ?? '')} |`);
  const table = ['| `.octocoderc` setting | Environment variable | Type | Default | Sensitive | Workspace may set | What it does |', '|---|---|---|---|---|---|---|', ...settingRows, ...environmentRows].join('\n');
  return `${page.slice(0, start)}${SETTINGS_START}\n\n${table}\n\n${page.slice(end)}`;
}
