// SKILL.md frontmatter parsing for skill-review: the common YAML scalar forms, not a full YAML parser.

// A folded or literal block scalar: the indented lines after `key:`.
function blockScalar(lines, key) {
  const start = lines.findIndex((item) => item.startsWith(`${key}:`));
  const parts = [];
  for (const item of lines.slice(start + 1)) {
    if (item && !/^\s+/.test(item)) break;
    parts.push(item.trim());
  }
  return parts.join(' ').trim();
}

// A plain scalar may continue on indented lines; YAML folds them with single spaces.
function plainScalar(lines, key, line) {
  const parts = [line];
  for (const item of lines.slice(lines.findIndex((l) => l.startsWith(`${key}:`)) + 1)) {
    if (!/^\s+\S/.test(item)) break;
    parts.push(item.trim());
  }
  return parts.join(' ').replace(/\s+#.*$/, '').trim();
}

// A double- or single-quoted scalar's value (null when malformed); undefined when the line is not quoted.
function quotedScalar(line) {
  if (line.startsWith('"')) {
    const quoted = line.match(/^"(?:\\.|[^"\\])*"/);
    try { return JSON.parse(quoted?.[0] || ''); } catch { return null; }
  }
  if (line.startsWith("'")) return line.match(/^'((?:''|[^'])*)'/)?.[1]?.replace(/''/g, "'") ?? null;
  return undefined;
}

function yamlField(block, key) {
  const line = block.match(new RegExp(`^${key}:[ \\t]*(.*)$`, 'm'))?.[1]?.trim();
  if (!line) return '';
  const lines = block.split('\n');
  if (/^[>|][+-]?(?:\s+#.*)?$/.test(line)) return blockScalar(lines, key);
  const quoted = quotedScalar(line);
  if (quoted !== undefined) return quoted;
  if (/^[[\]{&*!]/.test(line)) return null; // Advanced YAML needs a YAML-aware validator.
  return plainScalar(lines, key, line);
}

export function frontmatter(source) {
  source = source.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
  const match = source.match(/^---\s*\n([\s\S]*?)\n---(?:\n|$)/);
  if (!match) return null;
  return { name: yamlField(match[1], 'name'), description: yamlField(match[1], 'description') };
}

// Public skills are embedded in the native binary: `build` decodes `description` from one line.
export function jsonDescriptionProblem(source) {
  const lines = source.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n').match(/^---\s*\n([\s\S]*?)\n---(?:\n|$)/)?.[1].split('\n').filter((line) => line.startsWith('description:')) ?? [];
  let value;
  try { value = JSON.parse(lines[0]?.slice('description:'.length).trim()); } catch { /* reported below */ }
  return lines.length === 1 && typeof value === 'string' ? null : 'Public skills need one `description: "<JSON string>"` line; the native build embeds it.';
}
