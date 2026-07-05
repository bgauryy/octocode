/**
 * extract-hook-files.ts — Extract file paths from a hook JSON payload (stdin).
 *
 * Handles Claude-style tool_input payloads, Pi tool events (`input`/`args`),
 * and Codex apply_patch command strings. Prints one path per line, deduplicated.
 * Exits 0 on any error (fail-open).
 *
 * Compiled to dist/bin/extract-hook-files.js.
 */

let raw = '';
process.stdin.on('data', (chunk: Buffer | string) => { raw += String(chunk); });
process.stdin.on('end', () => {
  try {
    const data: unknown = JSON.parse(raw);
    const root = data !== null && typeof data === 'object'
      ? data as Record<string, unknown>
      : {} as Record<string, unknown>;
    const toolInput = root.tool_input ?? root.input ?? root.args ?? null;
    const ti = toolInput !== null && typeof toolInput === 'object'
      ? (toolInput as Record<string, unknown>)
      : root;

    const paths: string[] = [];

    function add(value: unknown): void {
      if (typeof value === 'string' && value.trim()) {
        paths.push(value.trim());
      } else if (Array.isArray(value)) {
        for (const item of value) add(item);
      }
    }

    add(ti['file_path']);
    add(ti['path']);
    add(ti['file_paths']);

    const command = ti['command'];
    if (typeof command === 'string') {
      for (const line of command.split('\n')) {
        const addUpdDel = line.match(/^\*\*\* (?:Add|Update|Delete) File: (.+)$/);
        if (addUpdDel) { paths.push(addUpdDel[1]!.trim()); continue; }
        const moveTo = line.match(/^\*\*\* Move to: (.+)$/);
        if (moveTo) paths.push(moveTo[1]!.trim());
      }
    }

    const seen = new Set<string>();
    for (const p of paths) {
      if (p && !seen.has(p)) {
        seen.add(p);
        process.stdout.write(p + '\n');
      }
    }
  } catch {
    // Fail-open: parse error → print nothing, exit 0
  }
  process.exit(0);
});
