#!/usr/bin/env node
// Source of scripts/keys.mjs: the Octocode API-key lookup from @octocodeai/config, bundled so the
// installed skill needs no packages. Scripts import getKey from ./keys.mjs; run it for the CLI.
// A key comes from the process environment, then <home>/.env, then <workspace>/.octocode/.env.
import { runKeysCli } from '@octocodeai/config';
import { isMainModule } from '../scripts/lib/entrypoint.mjs';

export { getKey, keyFiles, listKeys } from '@octocodeai/config';

if (isMainModule(import.meta.url)) {
  const { exitCode, output } = runKeysCli(process.argv.slice(2));
  console.log(typeof output === 'string' ? output : JSON.stringify(output, null, 2));
  process.exitCode = exitCode;
}
