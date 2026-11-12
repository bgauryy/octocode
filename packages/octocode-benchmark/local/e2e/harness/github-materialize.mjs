import fs from 'node:fs';
import path from 'node:path';

const data = result => result.json?.results?.[0]?.data;
const succeeds = result => result.json?.results?.length > 0
  && result.json.results.every(row => row.status !== 'error' && !row.data?.error)
  && (result.surface === 'mcp' ? !result.isError : result.status === 0);

/** Adversarial cache state and writer concurrency through the actual CLI and MCP. */
export async function checkMaterialization(ctx) {
  const checks = [];
  const query = { owner: 'octo', repo: 'demo', maxDepth: 3, materialize: true };
  const envelope = { queries: [query] };
  const original = Buffer.from(ctx.mock.model.repos.find(repo => repo.name === 'demo').files['src/greeter.ts']);
  const record = (name, ok, detail = '') => checks.push([name, !!ok, detail]);
  for (const surface of ctx.surfaces) {
    const call = ctx.callOn(surface);
    const first = await call('ghStructure', envelope);
    record(`materialize.initial.${surface}`, succeeds(first), `exit ${first.status}, isError ${first.isError}`);
    const checkout = data(first)?.location?.path;
    if (!checkout) continue;
    const target = path.join(checkout, 'src/greeter.ts');
    const altered = Buffer.from(original.map(byte => byte === 10 ? 10 : 88));
    fs.writeFileSync(target, altered);
    const repaired = await call('ghStructure', envelope);
    record(`materialize.contentIdentity.${surface}`, succeeds(repaired) && fs.readFileSync(target).equals(original), 'same-size edit must be restored from the pinned provider file');

    if (process.platform !== 'win32') {
      const outside = path.join(ctx.tmp, `materialize-outside-${surface}`);
      fs.mkdirSync(outside, { recursive: true });
      const sentinel = path.join(outside, 'greeter.ts');
      const sentinelBytes = Buffer.from('outside the materialized checkout');
      fs.writeFileSync(sentinel, sentinelBytes);
      fs.unlinkSync(target);
      fs.symlinkSync(sentinel, target);
      const leaf = await call('ghStructure', envelope);
      record(`materialize.leafLink.${surface}`, succeeds(leaf)
        && fs.readFileSync(sentinel).equals(sentinelBytes)
        && !fs.lstatSync(target).isSymbolicLink()
        && fs.readFileSync(target).equals(original), 'replace the link itself; outside bytes must stay unchanged');

      const source = path.join(checkout, 'src');
      const saved = path.join(ctx.tmp, `materialize-saved-src-${surface}`);
      fs.renameSync(source, saved);
      fs.symlinkSync(outside, source, 'dir');
      try {
        const ancestor = await call('ghStructure', envelope);
        const error = data(ancestor);
        record(`materialize.ancestorLink.${surface}`, ancestor.json?.results?.[0]?.status === 'error'
          && error?.errorCode !== 'invalidInput'
          && fs.readFileSync(sentinel).equals(sentinelBytes), `errorCode ${error?.errorCode}`);
        record(`materialize.ioError.${surface}`, surface === 'mcp' ? ancestor.isError : ancestor.status === 5,
          `exit ${ancestor.status}, isError ${ancestor.isError}`);
      } finally {
        fs.unlinkSync(source);
        fs.renameSync(saved, source);
      }
    } else {
      ctx.skips.push('materialize local symlink probes require Unix; Windows fallback confinement is not established by this run');
    }

    const coldRepo = ctx.mock.model.repos.filter(repo => repo.name !== query.repo)[ctx.surfaces.indexOf(surface)];
    const coldQuery = { ...query, owner: coldRepo.owner, repo: coldRepo.name };
    const pair = await call('ghStructure', { queries: [coldQuery, coldQuery] });
    record(`materialize.concurrentRows.${surface}`, succeeds(pair) && pair.json.results.length === 2, 'identical batch rows must both finish');
    const manifest = JSON.parse(fs.readFileSync(`${checkout}.manifest.json`, 'utf8'));
    record(`materialize.manifest.${surface}`, Object.keys(manifest.files).length === 5
      && Object.values(manifest.files).every(identity => Number.isInteger(identity.bytes) && /^[a-f0-9]{64}$/.test(identity.sha256)), 'all five fixture files retain a size and digest');
    record(`materialize.outputContract.${surface}`, !ctx.validateOutput('ghStructure', pair.raw), ctx.validateOutput('ghStructure', pair.raw) ?? '');

    const language = await call('ghStructure', { queries: [{ owner: 'octo', repo: 'demo', operation: 'languages', page: 2 }] });
    const code = language.json?.errorCode ?? data(language)?.errorCode;
    record(`materialize.languagePageRejected.${surface}`, code === 'invalidInput', `errorCode ${code}`);
  }

  const processHome = path.join(ctx.tmp, 'materialize-process-home');
  const processes = await Promise.all([
    ctx.cliCall('ghStructure', envelope, { env: { OCTOCODE_HOME: processHome } }),
    ctx.cliCall('ghStructure', envelope, { env: { OCTOCODE_HOME: processHome } }),
  ]);
  record('materialize.concurrentProcesses', processes.every(succeeds), JSON.stringify(processes.map(result => ({
    exit: result.status, error: result.json?.errorCode ?? data(result)?.errorCode,
    message: result.json?.error ?? data(result)?.error,
  }))));
  return checks;
}
