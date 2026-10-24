import { spawnSync } from 'node:child_process';
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const rustModule = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '../scripts/tool-contract/rust.ts'
);

/** A `cargo` that answers `typify --version`, then writes `stderrBytes` of diagnostics and exits `status`. */
function fakeCargo(dir: string, stderrBytes: number, status: number): void {
  const script = join(dir, 'cargo');
  writeFileSync(
    script,
    `#!/bin/sh
[ "$2" = "--version" ] && { echo "cargo-typify 0.8.0"; exit 0; }
out=""
while [ $# -gt 0 ]; do [ "$1" = "-o" ] && out="$2"; shift; done
head -c ${stderrBytes} /dev/zero | tr '\\0' 'x' >&2
[ ${status} -eq 0 ] && echo 'pub struct Id(::std::string::String);' > "$out"
exit ${status}
`
  );
  chmodSync(script, 0o755);
}

/** `renderRust` in a child process whose PATH holds only the fake cargo, so stderr forwarding is observable. */
function render(stderrBytes: number, status: number) {
  const dir = mkdtempSync(join(tmpdir(), 'rust-render-'));
  try {
    fakeCargo(dir, stderrBytes, status);
    const code = `import { renderRust } from ${JSON.stringify(rustModule)};
process.stdout.write(renderRust('// header', '{"$defs":{}}', new Set(['Id'])));`;
    return spawnSync(
      process.execPath,
      ['--input-type=module', '-e', code],
      {
        encoding: 'utf8',
        maxBuffer: 64 << 20,
        env: { PATH: `${dir}${delimiter}/usr/bin:/bin` },
      }
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

describe.skipIf(process.platform === 'win32')('renderRust typify diagnostics', () => {
  it('streams diagnostics past the in-memory buffer limit without failing', () => {
    // execFileSync buffers a piped stream up to 1 MiB and then fails with ENOBUFS.
    const bytes = 4 << 20;
    const run = render(bytes, 0);
    expect(run.status, run.stderr.slice(0, 300)).toBe(0);
    expect(run.stderr).toHaveLength(bytes);
    expect(run.stdout).toContain('pub struct Id');
  });

  it('shows diagnostics and a non-zero exit when typify fails', () => {
    const run = render(2048, 3);
    expect(run.status).not.toBe(0);
    expect(run.stderr).toContain('x'.repeat(2048));
    expect(run.stderr).toMatch(/Command failed: cargo typify/);
  });
});
