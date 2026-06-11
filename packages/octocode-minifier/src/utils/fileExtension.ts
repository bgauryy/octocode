export type GetExtensionOptions = {
  lowercase?: boolean;
  fallback?: string;
};

export function getExtension(
  filePath: string,
  options?: GetExtensionOptions
): string {
  // Strip any leading directory path first so basenames like ".gitignore" work
  // correctly whether the caller passes a bare name or a full path.
  const basename = filePath.split(/[\\/]/).pop()!;
  const parts = basename.split('.');

  // No dot at all (e.g. "Makefile", "README") → no extension.
  if (parts.length <= 1) {
    return options?.fallback ?? '';
  }

  // Dotfiles: ".gitignore" → parts = ['', 'gitignore'].
  // The extension IS the part after the leading dot — return it so that
  // MINIFY_CONFIG entries for "gitignore", "env", "dockerignore" etc. are hit.
  if (parts.length === 2 && parts[0] === '') {
    const ext = parts[1]!;
    return options?.lowercase ? ext.toLowerCase() : ext;
  }

  const ext = parts[parts.length - 1]!;
  return options?.lowercase ? ext.toLowerCase() : ext;
}
