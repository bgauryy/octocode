/**
 * Serialize an OQL JSON `StructuralRule` into the engine's YAML rule string.
 *
 * The structural engine's `rule` field is a YAML string with a top-level
 * `rule:` key (matching the `grep --rule` surface), NOT a JSON object. Agents
 * author the JSON object form (per the contract); this lowers it to the engine
 * format. Covers the V1 subset: pattern, kind, inside, has, not, all, any,
 * stopBy.
 */
import type { StructuralRule } from '../types.js';

const INDENT = '  ';

function scalar(value: string): string {
  // Double-quoted YAML scalars are JSON-compatible, so JSON.stringify yields a
  // safe quoted form for patterns containing $, {, }, :, #, etc.
  return JSON.stringify(value);
}

/** Emit the key/value lines for one rule's body at the given indent depth. */
function ruleBodyLines(rule: StructuralRule, depth: number): string[] {
  const pad = INDENT.repeat(depth);
  const lines: string[] = [];

  if (rule.pattern !== undefined)
    lines.push(`${pad}pattern: ${scalar(rule.pattern)}`);
  if (rule.kind !== undefined) lines.push(`${pad}kind: ${scalar(rule.kind)}`);

  for (const rel of ['inside', 'has', 'not'] as const) {
    const child = rule[rel];
    if (child) {
      lines.push(`${pad}${rel}:`);
      lines.push(...ruleBodyLines(child, depth + 1));
    }
  }

  for (const combinator of ['all', 'any'] as const) {
    const items = rule[combinator];
    if (items && items.length > 0) {
      lines.push(`${pad}${combinator}:`);
      for (const item of items) {
        const itemLines = ruleBodyLines(item, depth + 1);
        // Sequence item: first body line gets the "- " marker, rest align under it.
        const marker = `${INDENT.repeat(depth + 1)}- `;
        const contIndent = `${INDENT.repeat(depth + 1)}  `;
        if (itemLines.length === 0) continue;
        lines.push(marker + itemLines[0]!.trimStart());
        for (const extra of itemLines.slice(1)) {
          lines.push(contIndent + extra.trimStart());
        }
      }
    }
  }

  if (rule.stopBy !== undefined) lines.push(`${pad}stopBy: ${rule.stopBy}`);

  return lines;
}

export function structuralRuleToYaml(rule: StructuralRule): string {
  return ['rule:', ...ruleBodyLines(rule, 1)].join('\n');
}
