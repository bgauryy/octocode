interface SkillFrontmatter {
  name?: string;
  description?: string;
  category?: string;
}

function stripQuotes(value: string): string {
  if (value.length >= 2) {
    const first = value[0];
    const last = value[value.length - 1];
    if ((first === '"' || first === "'") && first === last) {
      return value.slice(1, -1);
    }
  }
  return value;
}

function extractField(lines: string[], key: string): string | undefined {
  const keyPattern = new RegExp(`^${key}:(.*)$`);
  const idx = lines.findIndex(line => keyPattern.test(line));
  if (idx === -1) {
    return undefined;
  }

  const inline = (lines[idx].match(keyPattern)?.[1] ?? '').trim();

  if (/^[|>][+-]?\d*$/.test(inline)) {
    const folded = inline.startsWith('>');
    const body: string[] = [];
    for (let i = idx + 1; i < lines.length; i++) {
      const line = lines[i];
      if (line.trim() === '') {
        body.push('');
        continue;
      }
      if (!/^\s/.test(line)) {
        break;
      }
      body.push(line.replace(/^\s+/, ''));
    }
    while (body.length > 0 && body[body.length - 1] === '') {
      body.pop();
    }

    if (!folded) {
      return body.join('\n').trim();
    }

    let out = '';
    for (const line of body) {
      if (line === '') {
        out += '\n';
      } else {
        out += out && !out.endsWith('\n') ? ` ${line}` : line;
      }
    }
    return out.trim();
  }

  if (inline === '') {
    return undefined;
  }

  return stripQuotes(inline).trim();
}

export function parseSkillFrontmatter(
  content: string
): SkillFrontmatter | null {
  const match = content.match(/^---\s*\n([\s\S]*?)\n---/);
  if (!match) return null;

  const lines = match[1].split('\n');

  return {
    name: extractField(lines, 'name'),
    description: extractField(lines, 'description'),
    category: extractField(lines, 'category'),
  };
}
