// Git smart HTTP for the tool-health mock: a bare `octo/demo.git` whose history
// is the model's main commits, served by `git http-backend` behind Basic auth.
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { DATE } from './mock-model.mjs';

const GIT_ENV = {
  GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null',
  GIT_AUTHOR_NAME: 'Octo Dev', GIT_AUTHOR_EMAIL: 'dev@example.invalid', GIT_COMMITTER_NAME: 'Octo Dev', GIT_COMMITTER_EMAIL: 'dev@example.invalid',
};
const git = (args, cwd, date = DATE(0)) => spawnSync('git', args, { cwd, encoding: 'utf8', env: { ...process.env, ...GIT_ENV, GIT_AUTHOR_DATE: date, GIT_COMMITTER_DATE: date } });

/** Commit each model commit in `work`, rewriting its oid/tree to the real ones; a failure reason or null. */
function commitHistory(model, work) {
  for (const commit of model.commits) {
    for (const file of commit.added) {
      fs.mkdirSync(path.dirname(path.join(work, file)), { recursive: true });
      fs.writeFileSync(path.join(work, file), model.files[file]);
    }
    for (const args of [['add', '-A'], ['-c', 'commit.gpgsign=false', 'commit', '-q', '-m', commit.message]]) {
      const r = git(args, work, commit.date);
      if (r.status !== 0) return `git ${args.at(-3) ?? args[0]} failed: ${r.stderr.trim()}`;
    }
    commit.oid = git(['rev-parse', 'HEAD'], work).stdout.trim();
    commit.tree = git(['rev-parse', 'HEAD^{tree}'], work).stdout.trim();
  }
  model.commits.forEach((commit, i) => { commit.parent = model.commits[i - 1]?.oid; });
  return null;
}

/**
 * A bare `octo/demo.git` under `<tmpDir>/git` whose history is the model's
 * commits (same files, messages and dates). The model's commit and tree SHAs
 * are replaced by the real ones, so REST answers and a clone agree. Returns
 * `{ root }`, or `{ reason }` when git is unavailable.
 */
export function makeGitRepo(model, tmpDir) {
  if (git(['--version']).status !== 0) return { reason: 'git is not installed' };
  const work = path.join(tmpDir, 'git-work');
  const root = path.join(tmpDir, 'git');
  fs.mkdirSync(work, { recursive: true });
  const init = git(['init', '-q', '-b', 'main'], work);
  if (init.status !== 0) return { reason: `git init failed: ${init.stderr.trim()}` };
  const failed = commitHistory(model, work);
  if (failed) return { reason: failed };
  fs.mkdirSync(path.join(root, 'octo'), { recursive: true });
  const bare = git(['clone', '-q', '--bare', work, path.join(root, 'octo', 'demo.git')], tmpDir);
  if (bare.status !== 0) return { reason: `git clone --bare failed: ${bare.stderr.trim()}` };
  git(['config', 'http.uploadpack', 'true'], path.join(root, 'octo', 'demo.git'));
  return { root };
}

/** Split a CGI response into its status, headers and body. */
function cgiResponse(buffer) {
  const split = buffer.indexOf('\r\n\r\n');
  const headers = {};
  let status = 200;
  for (const line of buffer.subarray(0, split).toString('utf8').split('\r\n')) {
    const i = line.indexOf(':');
    if (i < 0) continue;
    const key = line.slice(0, i).trim().toLowerCase(), value = line.slice(i + 1).trim();
    if (key === 'status') status = Number.parseInt(value, 10);
    else headers[key] = value;
  }
  return { status, headers, body: buffer.subarray(split + 4) };
}

/**
 * Answer one git smart-HTTP request (`request` = { req, url, raw }) for `server`
 * = { root, tokens, gitAuth }: Basic auth as `x-access-token` with a known
 * token, recorded in `gitAuth` as `{ user, tokenMatches }` (never the secret).
 */
export function serveGit(request, res, server) {
  const { req, url, raw } = request;
  const auth = /^Basic (.+)$/.exec(req.headers.authorization ?? '');
  if (!auth) { res.writeHead(401, { 'www-authenticate': 'Basic realm="octocode-tool-health"' }); res.end(); return; }
  const [user, ...rest] = Buffer.from(auth[1], 'base64').toString('utf8').split(':');
  const tokenMatches = server.tokens.has(rest.join(':'));
  server.gitAuth.push({ user, tokenMatches });
  if (user !== 'x-access-token' || !tokenMatches) { res.writeHead(403); res.end(); return; }
  const child = spawn('git', ['http-backend'], { env: {
    ...process.env, GIT_PROJECT_ROOT: server.root, GIT_HTTP_EXPORT_ALL: '1', PATH_INFO: url.pathname, QUERY_STRING: url.search.slice(1),
    REQUEST_METHOD: req.method, CONTENT_TYPE: req.headers['content-type'] ?? '', CONTENT_LENGTH: String(raw.length),
    HTTP_CONTENT_ENCODING: req.headers['content-encoding'] ?? '', GIT_PROTOCOL: req.headers['git-protocol'] ?? '', REMOTE_USER: user, REMOTE_ADDR: '127.0.0.1',
  } });
  const out = [];
  child.stdout.on('data', c => out.push(c));
  child.on('close', () => {
    const { status, headers, body } = cgiResponse(Buffer.concat(out));
    res.writeHead(status, headers);
    res.end(body);
  });
  child.stdin.end(raw);
}
