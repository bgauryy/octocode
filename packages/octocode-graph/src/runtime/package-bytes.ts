// What an external package costs, as a build measured it into data.json (package-closure.ts measurePackages):
// `externals[].bytes` with `bytesMethod` and `bytesEvidence`. Pure over data.json, so the query engine, the MCP
// server and the browser worker read it alike.

/** How a package's bytes were measured: its inputs in a bundler's output, or the files its installed entry loads. */
export type BytesMethod = 'bundle' | 'installed-closure';

/** The evidence behind a package's bytes, or why it has none. */
export interface BytesEvidence {
  /** The bundler file (bundle), the entry file the closure starts from (installed-closure), or the importer a package
   * failed to resolve from; root-relative. */
  source?: string;
  /** Files counted. */
  files?: number;
  /** The installed version (installed-closure). */
  version?: string;
  /** A bundle's measure: bytes in output, or module size before rendering (webpack stats, rolldown analyze). */
  measure?: 'output' | 'module';
  /** Installed packages the closure walked, as name@version (the package itself first). */
  packages?: string[];
  /** Specifiers the closure could not resolve, with the file that names them: they add no bytes. */
  unresolved?: { specifier: string; from: string }[];
  /** Why the package has no bytes (a builtin, not installed, another ecosystem). */
  reason?: string;
}

/** The fields a build adds to each `externals[]` row. */
export interface PackageSize {
  bytes: number | null;
  bytesMethod: BytesMethod | null;
  bytesEvidence: BytesEvidence;
}

/** A package's measured bytes in a saved graph (`pkg:<name>`), or null when the graph lists no such external. */
export function packageBytes(
  D: { externals?: readonly ({ id: string } & Partial<PackageSize>)[] },
  pkgId: string
): PackageSize | null {
  const x = D.externals?.find(e => e.id === pkgId);
  if (!x) return null;
  return {
    bytes: typeof x.bytes === 'number' ? x.bytes : null,
    bytesMethod: x.bytesMethod ?? null,
    bytesEvidence: x.bytesEvidence ?? {
      reason: 'not measured: the graph was built before package bytes were',
    },
  };
}
