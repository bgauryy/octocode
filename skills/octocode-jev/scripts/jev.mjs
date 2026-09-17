#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { getOctocodeHome, loadConfigSync, loadOctocodeEnv, resolveNetwork, validateConfig } from './octocode-config.mjs';

const args = process.argv.slice(2);
const projectEnv = args.includes('--project-env');
const forwarded = args.filter(x => x !== '--project-env');
const binary = fileURLToPath(new URL(`../bin/octocode-jev-${process.platform}-${process.arch}${process.platform === 'win32' ? '.exe' : ''}`, import.meta.url));
function stop(message) {
  process.stderr.write(JSON.stringify({ error: { code: 2, message } }) + '\n');
  process.exit(2);
}
if (!existsSync(binary)) stop(`Missing binary for ${process.platform}/${process.arch}; run npm run build inside this skill folder.`);
const childEnv = { ...process.env };
const informational = forwarded.length === 0 || forwarded.length === 1 && ['--help', '-h', '--version'].includes(forwarded[0]);
if (!informational) {
  try {
    const home = getOctocodeHome();
    const result = loadConfigSync(home);
    if (!result.success && result.error !== 'Config file does not exist') stop('Cannot parse Octocode .octocoderc; repair it before running Jev. File contents omitted.');
    const config = result.config || {};
    if (config.env !== undefined && (config.env === null || typeof config.env !== 'object' || Array.isArray(config.env))) stop('.octocoderc env must be an object.');
    const { map } = loadOctocodeEnv({ home, cwd: process.cwd(), trusted: projectEnv });
    for (const name of ['OCTOCODE_JEV_KEY', 'OCTOCODE_JEV_MODEL', 'OCTOCODE_JEV_BASE_URL', 'REQUEST_TIMEOUT', 'MAX_RETRIES']) {
      const candidates = [config.env?.[name], config[name]];
      if (candidates.some(value => value !== undefined && typeof value !== 'string')) stop(`.octocoderc ${name} must be a string.`);
      const value = candidates.map(value => value?.trim()).find(Boolean);
      childEnv[name] = childEnv[name]?.trim() || map[name]?.trim() || value?.trim() || '';
    }
    // Resolve only the shared settings this HTTP client consumes. Other Octocode
    // sections remain valid alongside Jev and are not interpreted as Jev options.
    if (!validateConfig({ network: config.network }).valid) stop('Invalid .octocoderc network settings; timeout must be 5000..300000 and maxRetries 0..10.');
    for (const name of ['REQUEST_TIMEOUT', 'MAX_RETRIES']) process.env[name] = childEnv[name];
    const network = resolveNetwork(config.network);
    if (!forwarded.includes('--timeout-ms')) forwarded.push('--timeout-ms', String(network.timeout));
    if (!forwarded.includes('--retries')) forwarded.push('--retries', String(network.maxRetries));
  } catch { stop('Cannot load Jev configuration; check OCTOCODE_HOME and file permissions.'); }
}
const child = spawn(binary, forwarded, { env: childEnv, stdio: 'inherit' });
child.on('error', () => stop('Cannot execute host binary; run npm run build on this machine.'));
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
child.on('exit', (code, signal) => { process.exitCode = code ?? (signal === 'SIGINT' ? 130 : 143); });
