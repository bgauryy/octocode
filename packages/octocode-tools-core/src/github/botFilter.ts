const KNOWN_BOT_LOGINS = new Set([
  'vercel',
  'pkg-pr-new',
  'coderabbitai',
  'github-actions',
  'codecov',
  'changeset-bot',
  'netlify',
  'sonarcloud',
  'socket-security',
]);

export function isBotAuthor(login: string): boolean {
  const lower = login.toLowerCase();
  return lower.endsWith('[bot]') || KNOWN_BOT_LOGINS.has(lower);
}
