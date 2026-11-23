// What can run here, and how to enable the rest. Never installs or starts anything.
import { execFileSync } from 'node:child_process';
import { cliOk, errorMessage, octocode } from './lib.ts';
import type { ToolReply } from './tools.ts';

const has = (cmd: string, args: string[]) => {
  try {
    execFileSync(cmd, args, { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
};
const HINTS: Record<string, string> = {
  cli: 'install `octocode` on PATH or set OCTOCODE_CLI to an installed executable',
  graph:
    'the octocode CLI lacks `graphBuild`/`graphQuery`: rebuild or update it',
  structureSearch:
    'check `octocode structureSearch` for this workspace (WORKSPACE_ROOT, allowed roots); a refused listing stops the run',
  astSearch:
    'AST outlines in the viewer stay empty until `octocode astSearch` works here',
  lspSearch: 'static calls are available without LSP confirmation',
  rsync:
    'install rsync: a tree that changes during ingest is graphed from a frozen copy',
  opener: 'open the printed URL by hand (no `open`/`xdg-open`)',
};

async function tool(name: string, input: unknown) {
  try {
    // A tool reply envelope; its first row's error code is read optionally.
    const r = (await octocode([name, '--json', JSON.stringify(input)], {
      partial: true,
    })) as ToolReply;
    const row = r.results?.[0]?.data;
    return row?.errorCode ? `error: ${row.errorCode}` : 'ok';
  } catch (error) {
    return `error: ${errorMessage(error).split('\n')[0].slice(0, 160)}`;
  }
}

/** Each layer's state: `ok…`, `missing` or `error: …`. */
export interface Layers {
  cli: string;
  graph?: string;
  structureSearch?: string;
  astSearch?: string;
  lspSearch?: string;
  rsync?: string;
  opener?: string;
}

export async function preflight() {
  const here = process.cwd();
  const r: Layers = { cli: (await cliOk(['--version'])) ? 'ok' : 'missing' };
  r.graph =
    (await cliOk(['schema', 'graphBuild'])) &&
    (await cliOk(['schema', 'graphQuery']))
      ? 'ok'
      : 'missing';
  r.structureSearch = await tool('structureSearch', {
    queries: [
      {
        operation: 'files',
        path: here,
        entryType: 'file',
        view: 'paths',
        limit: 1,
      },
    ],
  });
  r.astSearch = (await cliOk(['schema', 'astSearch'])) ? 'ok' : 'missing';
  r.lspSearch = (await cliOk(['schema', 'lspSearch']))
    ? 'ok (tool advertised; confirmation depends on the language server)'
    : 'missing';
  r.rsync = has('rsync', ['--version']) ? 'ok' : 'missing';
  r.opener =
    process.platform === 'darwin' ||
    process.platform === 'win32' ||
    has('which', ['xdg-open'])
      ? 'ok'
      : 'missing';
  return r;
}

export async function main() {
  if (process.argv.includes('--help')) {
    process.stdout.write('usage: octocode-graph preflight [--json]\n');
    process.exit(0);
  }
  const r = await preflight();
  const todo = Object.entries(r)
    .filter(([, v]) => !v.startsWith('ok'))
    .map(([k, v]) => ({ layer: k, state: v, enable: HINTS[k] }));
  if (process.argv.includes('--json'))
    process.stdout.write(`${JSON.stringify({ layers: r, todo })}\n`);
  else {
    for (const [k, v] of Object.entries(r))
      process.stdout.write(
        `${v.startsWith('ok') ? 'ok     ' : 'MISSING'} ${k.padEnd(16)} ${v}\n`
      );
    if (todo.length)
      process.stdout.write(
        `\nTo enable:\n${todo.map(t => `- ${t.layer}: ${t.enable}`).join('\n')}\n`
      );
  }
  process.exit(r.cli === 'ok' && r.graph === 'ok' ? 0 : 1);
}
