/** Directory segments hidden during discovery and blocked during file access. */
export const SENSITIVE_DIRECTORY_NAMES = [
  '.git',
  '.ssh',
  '.aws',
  '.docker',
  '.azure',
  '.kube',
  '.terraform',
  'secrets',
  'private',
  '.password-store',
  '.thunderbird',
  '.evolution',
  '.vagrant',
  '.minikube',
  '.bitcoin',
  '.ethereum',
  '.electrum',
] as const;

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^$\{\}()|[\]\\]/g, '\\$&');
}

export const SENSITIVE_DIRECTORY_PATTERNS: RegExp[] =
  SENSITIVE_DIRECTORY_NAMES.map(
    (name) => new RegExp(`(?:^|/)${escapeRegex(name)}(?:/|$)`),
  );
