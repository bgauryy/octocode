#!/usr/bin/env node
// `group open|add|list|close [--name N] [--urls a,b] [--private|--no-private] [--background] [--new-window] [--tab ID] --port N`:
// private tab-group management: argv validation here, the browser work in group-tabs.mts.
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromeCommands } from './chrome-contract.mjs';
import { errorMessage } from './error-message.mjs';
import { fail, optionValue, parse } from './cli-options.mjs';
import {
  addToGroup,
  closeGroup,
  listGroups,
  openGroup,
} from './group-tabs.mjs';

export const groupActions = ['open', 'add', 'list', 'close'] as const;
export type GroupAction = (typeof groupActions)[number];
const namePattern = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/;
const urlPattern = /^(https?:\/\/[^\s,]+|about:blank)$/;
/** Which optional flags each action takes; anything else is refused. */
const actionFlags: Record<GroupAction, readonly string[]> = {
  open: [
    '--name',
    '--urls',
    '--private',
    '--no-private',
    '--background',
    '--new-window',
  ],
  add: ['--name', '--urls', '--background'],
  list: [],
  close: ['--name', '--tab'],
};
const optionalFlags = [
  '--name',
  '--urls',
  '--private',
  '--no-private',
  '--background',
  '--new-window',
  '--tab',
];
export type GroupRequest = {
  action: GroupAction;
  name?: string;
  urls: string[];
  private: boolean;
  background: boolean;
  newWindow: boolean;
  tab?: string;
  port: number;
};
/** Validated `group` argv: the action, its fields (private defaults to true) and the required port. */
export function parseGroupArgs(args: string[]): GroupRequest {
  const { opts, positional } = parse(
    args,
    ['--port', '--name', '--urls', '--tab'],
    ['--private', '--no-private', '--background', '--new-window']
  );
  const action = positional[0] as GroupAction;
  if (!groupActions.includes(action) || positional.length !== 1)
    fail(`group needs one action: ${groupActions.join(', ')}`);
  for (const flag of optionalFlags)
    if (opts[flag] !== undefined && !actionFlags[action].includes(flag))
      fail(`${flag} does not apply to ${action}`);
  if (opts['--private'] && opts['--no-private'])
    fail('Use one of --private and --no-private');
  const name = optionValue(opts, '--name');
  if (name !== undefined && !namePattern.test(name))
    fail('group name: letters, digits, . _ -; at most 64 characters');
  if ((action === 'add' || action === 'close') && name === undefined)
    fail(`${action} needs --name`);
  const urlsText = optionValue(opts, '--urls');
  const urls = urlsText === undefined ? [] : urlsText.split(',');
  for (const url of urls)
    if (!urlPattern.test(url))
      fail(`--urls entries must be http(s) or about:blank: ${url}`);
  if (action === 'add' && urls.length === 0) fail('add needs --urls');
  const port = optionValue(opts, '--port');
  if (
    port === undefined ||
    !/^\d+$/.test(port) ||
    Number(port) < 1 ||
    Number(port) > 65535
  )
    fail('group needs --port, a positive integer <= 65535');
  return {
    action,
    name,
    urls,
    private: !opts['--no-private'],
    background: Boolean(opts['--background']),
    newWindow: Boolean(opts['--new-window']),
    tab: optionValue(opts, '--tab'),
    port: Number(port),
  };
}

export async function groupMain(args: string[]) {
  const request = parseGroupArgs(args);
  const note = (line: string) => process.stderr.write(`${line}\n`);
  const { port } = request;
  let reply;
  if (request.action === 'open') reply = await openGroup(port, request, note);
  else if (request.action === 'add')
    reply = await addToGroup(port, { ...request, name: request.name! }, note);
  else if (request.action === 'list') reply = await listGroups(port);
  else
    reply = await closeGroup(port, { name: request.name!, tab: request.tab });
  process.stdout.write(`${JSON.stringify(reply)}\n`);
  return 0;
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const args = process.argv.slice(2);
  if (args.includes('--help') || args.includes('-h')) {
    process.stdout.write(
      `group: ${chromeCommands.group}\nUsage: group open|add|list|close [--name N] [--urls a,b] [--private|--no-private] [--background] [--new-window] [--tab ID] --port N\n`
    );
  } else {
    try {
      process.exitCode = await groupMain(args);
    } catch (error) {
      process.stderr.write(`[GROUP] ${errorMessage(error)}\n`);
      process.exitCode = 2;
    }
  }
}
