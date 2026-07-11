import { isAbsolute, relative, resolve, sep } from 'node:path';

export function isPathInside(baseDir: string, targetPath: string): boolean {
  const normalizedBase = resolve(baseDir);
  const normalizedTarget = resolve(targetPath);
  const relativePath = relative(normalizedBase, normalizedTarget);

  return (
    relativePath !== '..' &&
    !relativePath.startsWith(`..${sep}`) &&
    !isAbsolute(relativePath)
  );
}

export function isSafeSkillName(skillName: string): boolean {
  const trimmed = skillName.trim();
  return (
    trimmed.length > 0 &&
    trimmed === skillName &&
    trimmed !== '.' &&
    trimmed !== '..' &&
    !trimmed.includes('\0') &&
    !trimmed.includes('/') &&
    !trimmed.includes('\\')
  );
}

export function resolveSkillDestination(
  destDir: string,
  skillName: string
): string | null {
  if (!isSafeSkillName(skillName)) {
    return null;
  }

  const destinationPath = resolve(destDir, skillName);
  return isPathInside(destDir, destinationPath) ? destinationPath : null;
}
