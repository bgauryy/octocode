#!/usr/bin/env node
import { ApiClient } from './client.js';
import { apiDir, listInstances, type InstanceRecord } from './registry.js';
import { errorMessage } from '../shared/util.js';

const USAGE = `octocode-pi-api — talk to running Octocode Pi sessions (start them with OCTOCODE_API=1)

  octocode-pi-api list
  octocode-pi-api status [--to <id>]
  octocode-pi-api send [--to <id>] [--from <name>] [--wait] [--mode steer|followUp] <text…>
  octocode-pi-api watch [--to <id>] [--types a,b] [--top-level]
  octocode-pi-api abort [--to <id>]

--to defaults to the only running instance (or the one in the current directory).
--top-level drops tool events of calls other tools made (those with parentId).`;

const BOOLEAN_FLAGS = new Set(['wait', 'top-level']);

function pick(instances: InstanceRecord[], id: string | undefined): InstanceRecord {
  const found = id ? instances.filter((entry) => entry.id === id) : instances.length > 1 ? instances.filter((entry) => entry.cwd === process.cwd()) : instances;
  if (found.length !== 1) throw new Error(found.length === 0 ? 'No running instance matches. Start Pi with OCTOCODE_API=1.' : `Several instances match; pass --to <id>: ${found.map((entry) => entry.id).join(', ')}`);
  return found[0]!;
}

async function main(argv: string[]): Promise<void> {
  const [command, ...rest] = argv;
  const flags = new Map<string, string | true>();
  const words: string[] = [];
  for (let index = 0; index < rest.length; index += 1) {
    const arg = rest[index]!;
    if (!arg.startsWith('--')) words.push(arg);
    else if (BOOLEAN_FLAGS.has(arg.slice(2))) flags.set(arg.slice(2), true);
    else flags.set(arg.slice(2), rest[++index] ?? '');
  }
  const instances = listInstances(apiDir());
  if (command === 'list') {
    for (const entry of instances) console.log(`${entry.id}\tpid ${entry.pid}\t${entry.cwd}`);
    return;
  }
  if (!command || !['status', 'send', 'watch', 'abort'].includes(command)) {
    console.log(USAGE);
    process.exitCode = command ? 1 : 0;
    return;
  }
  const client = new ApiClient(pick(instances, flags.get('to') as string | undefined));
  if (command === 'status') console.log(JSON.stringify(await client.call('status'), null, 2));
  else if (command === 'abort') console.log(JSON.stringify(await client.call('turn.abort')));
  else if (command === 'send') {
    const result = await client.call<{ text?: string }>('message.send', {
      text: words.join(' '),
      ...(flags.has('from') ? { from: flags.get('from') } : {}),
      ...(flags.has('mode') ? { mode: flags.get('mode') } : {}),
      ...(flags.get('wait') ? { wait: true } : {}),
    });
    console.log(flags.get('wait') ? (result.text ?? '') : JSON.stringify(result));
  } else {
    const types = typeof flags.get('types') === 'string' ? (flags.get('types') as string).split(',') : undefined;
    await client.watch((event) => console.log(JSON.stringify(event)), { ...(types ? { types } : {}), ...(flags.get('top-level') ? { topLevel: true } : {}) });
  }
}

main(process.argv.slice(2)).catch((error: unknown) => {
  console.error(errorMessage(error));
  process.exitCode = 1;
});
