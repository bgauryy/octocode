#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { validateFlags } from '../cli-flags.mjs';
import { errorMessage } from '../error-message.mjs';
const args = process.argv.slice(2);
const arg = (flag: string, fallback?: string) =>
  args.includes(flag) ? args[args.indexOf(flag) + 1] : fallback;
if (args.includes('--help') || args.includes('-h')) {
  console.log(
    'Usage: snapshot-query.mjs --file <page-snapshot.json> [--page 1] [--limit 60] [--view refs|outline] [--text <substring>] [--regex <pattern>]\nPages saved evidence without recapturing a changing page. --text (case-insensitive) or --regex keeps only refs whose role, name or frame matches, each with its outline line; with --view outline it filters the outline lines.'
  );
  process.exit(0);
}
try {
  validateFlags(args, [
    '--file',
    '--page',
    '--limit',
    '--view',
    '--text',
    '--regex',
  ]);
  const fileArg = arg('--file');
  if (!fileArg) throw new Error('--file is required');
  const file = resolve(fileArg);
  const page = Number(arg('--page', '1')),
    limit = Number(arg('--limit', '60'));
  const view = arg('--view', 'refs');
  if (
    !Number.isSafeInteger(page) ||
    page < 1 ||
    !Number.isSafeInteger(limit) ||
    limit < 1 ||
    limit > 300 ||
    !['refs', 'outline'].includes(view ?? '')
  )
    throw new Error('Invalid page, limit, or view');
  const text = arg('--text'),
    regexArg = arg('--regex');
  if (text !== undefined && regexArg !== undefined)
    throw new Error('Use one of --text and --regex');
  let test: ((value: string) => boolean) | null = null;
  if (text !== undefined) {
    const needle = text.toLowerCase();
    test = value => value.toLowerCase().includes(needle);
  } else if (regexArg !== undefined) {
    let pattern: RegExp;
    try {
      pattern = new RegExp(regexArg, 'i');
    } catch (error) {
      throw new Error(`Invalid --regex: ${errorMessage(error)}`, {
        cause: error,
      });
    }
    test = value => pattern.test(value);
  }
  const data = JSON.parse(readFileSync(file, 'utf8'));
  const all =
    view === 'refs'
      ? Object.entries(
          data.refs as Record<string, Record<string, unknown>>
        ).map(([ref, row]) => ({
          ref,
          ...row,
          line: `[${ref}] ${String(row.role)}${typeof row.name === 'string' && row.name ? ` "${row.name}"` : ''}`,
        }))
      : data.outline;
  if (!Array.isArray(all))
    throw new Error('Requested view unavailable in this capture');
  const rows = test
    ? all.filter(row =>
        test(
          view === 'refs'
            ? [row.role, row.name, row.frame].filter(Boolean).join(' ')
            : String(row.line ?? JSON.stringify(row))
        )
      )
    : all;
  const offset = (page - 1) * limit;
  const hasMore = offset + limit < rows.length;
  console.log(
    JSON.stringify(
      {
        file,
        url: data.url,
        title: data.title,
        view,
        ...(test
          ? { filter: text !== undefined ? { text } : { regex: regexArg } }
          : {}),
        page,
        limit,
        totalRows: rows.length,
        rows: rows.slice(offset, offset + limit),
        ...(hasMore
          ? {
              next: {
                continue: {
                  command: process.execPath,
                  args: [
                    fileURLToPath(import.meta.url),
                    '--file',
                    file,
                    '--page',
                    String(page + 1),
                    '--limit',
                    String(limit),
                    '--view',
                    view,
                    ...(text !== undefined ? ['--text', text] : []),
                    ...(regexArg !== undefined ? ['--regex', regexArg] : []),
                  ],
                },
              },
            }
          : {}),
      },
      null,
      2
    )
  );
} catch (error) {
  console.error('[SNAPSHOT_QUERY] ' + errorMessage(error));
  process.exitCode = 1;
}
