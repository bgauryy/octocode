// Skill source resolution for skill-sync --resolve-source: a GitHub URL, owner/repo[/path] or a local folder
// becomes {owner, repo, ref, path} plus the ghCloneRepo input that fetches it.
import { existsSync, statSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const NAME = /^[A-Za-z0-9_.-]+$/;
const COMMIT = /^[0-9a-f]{40}$/i;

const withoutSkillFile = (parts) => (parts.at(-1) === 'SKILL.md' ? parts.slice(0, -1) : parts);

function segments(text) {
  return text.split('/').filter(Boolean).map((part) => {
    try { return decodeURIComponent(part); } catch { return part; }
  });
}

// A URL's ref and path share slashes: each split point is a candidate until the remote's refs decide.
function splits(rest) {
  const parts = withoutSkillFile(rest);
  return parts.map((_, index) => ({ ref: parts.slice(0, index + 1).join('/'), path: parts.slice(index + 1).join('/') }));
}

const repoOf = (owner, repo) => (owner && repo && NAME.test(owner) && NAME.test(repo.replace(/\.git$/, '')) ? { owner, repo: repo.replace(/\.git$/, '') } : null);

/** The source's owner, repo and every {ref, path} reading of it; null when it is neither GitHub nor a local skill. */
export function parseSource(input) {
  const text = String(input || '').trim();
  if (!text) return null;
  const local = resolve(text);
  if (existsSync(local)) {
    if (statSync(local).isDirectory()) return { kind: 'local', path: local };
    return basename(local) === 'SKILL.md' ? { kind: 'local', path: dirname(local) } : null;
  }
  const url = text.match(/^(?:https?:\/\/)?(?:www\.)?(github\.com|raw\.githubusercontent\.com)\/([^?#]*)/i);
  if (url) {
    const [owner, repo, ...rest] = segments(url[2]);
    const named = repoOf(owner, repo);
    if (!named) return null;
    if (url[1].toLowerCase() === 'raw.githubusercontent.com') return rest.length ? { kind: 'github', ...named, candidates: splits(rest) } : null;
    const [mode, ...tail] = rest;
    if (!mode) return { kind: 'github', ...named, candidates: [{ ref: null, path: '' }] };
    if ((mode !== 'tree' && mode !== 'blob') || !splits(tail).length) return null;
    return { kind: 'github', ...named, candidates: splits(tail) };
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(text)) return null;
  const [owner, repo, ...rest] = segments(text);
  const named = repoOf(owner, repo);
  return named ? { kind: 'github', ...named, candidates: [{ ref: null, path: withoutSkillFile(rest).join('/') }] } : null;
}

/** Branch and tag names among the candidates that exist on the remote, from `git ls-remote` (read-only). */
export function remoteRefs(owner, repo, candidates) {
  const patterns = candidates.flatMap(({ ref }) => [`refs/heads/${ref}`, `refs/tags/${ref}`]);
  const run = spawnSync('git', ['ls-remote', '--refs', `https://github.com/${owner}/${repo}.git`, ...patterns], {
    encoding: 'utf8',
    env: { ...process.env, GIT_TERMINAL_PROMPT: '0' },
  });
  if (run.error) return { ran: false, ok: false, error: run.error.message };
  if (run.status !== 0) return { ran: true, ok: false, error: (run.stderr || '').trim().split('\n')[0] || `git ls-remote exited ${run.status}` };
  return { ran: true, ok: true, refs: run.stdout.split('\n').map((line) => line.split('\t')[1]).filter(Boolean) };
}

const remoteKind = (refs, ref) => {
  if (refs.includes(`refs/heads/${ref}`)) return 'branch';
  return refs.includes(`refs/tags/${ref}`) ? 'tag' : 'missing';
};

/** One {ref, path} when the source decides it, else every candidate and why the choice stays open; null for an unknown source. */
export function resolveSource(input, { lookup = remoteRefs } = {}) {
  const parsed = parseSource(input);
  if (!parsed) return null;
  if (parsed.kind === 'local') return { kind: 'local', input, path: parsed.path, ambiguous: false };
  const { owner, repo, candidates } = parsed;
  let decided = null;
  let refCheck = { ran: false };
  if (candidates.length === 1) [decided] = candidates;
  else if (COMMIT.test(candidates[0].ref)) [decided] = candidates;
  else {
    refCheck = lookup(owner, repo, candidates);
    for (const candidate of candidates) candidate.remote = refCheck.ok ? remoteKind(refCheck.refs, candidate.ref) : 'unchecked';
    const found = candidates.filter((candidate) => candidate.remote !== 'missing' && candidate.remote !== 'unchecked');
    if (found.length === 1) [decided] = found;
  }
  if (!decided) return { kind: 'github', input, owner, repo, ref: null, path: null, ambiguous: true, candidates, refCheck };
  const { ref, path } = decided;
  const query = { owner, repo, ...(ref ? { ref } : {}), ...(path ? { paths: [path] } : {}) };
  return { kind: 'github', input, owner, repo, ref, path, ambiguous: false, candidates, refCheck, ghCloneRepo: { queries: [query] } };
}
