// Compare JSON numeric lexemes without rounding or allocating exponent-sized strings.
function decimal(value: unknown) {
  const raw = JSON.isRawJSON(value)
    ? (value as { rawJSON: string }).rawJSON
    : typeof value === 'number' && Number.isFinite(value)
      ? String(value)
      : typeof value === 'bigint'
        ? String(value)
        : null;
  if (raw === null) return null;
  const match = /^(-?)(\d+)(?:\.(\d+))?(?:[eE]([+-]?\d+))?$/.exec(raw);
  if (!match) return null;
  const fraction = match[3] ?? '';
  const digits = (match[2] + fraction).replace(/^0+/, '');
  return {
    sign: digits ? (match[1] ? -1 : 1) : 0,
    digits: digits.replace(/0+$/, ''),
    order:
      BigInt(match[4] ?? '0') - BigInt(fraction.length) + BigInt(digits.length),
  };
}
export function compareNumeric(
  actual: unknown,
  expected: unknown
): number | null {
  const a = decimal(actual),
    b = decimal(expected);
  if (!a || !b) return null;
  if (a.sign !== b.sign) return a.sign < b.sign ? -1 : 1;
  if (!a.sign) return 0;
  if (a.order !== b.order) return (a.order < b.order ? -1 : 1) * a.sign;
  const width = Math.max(a.digits.length, b.digits.length);
  const left = a.digits.padEnd(width, '0'),
    right = b.digits.padEnd(width, '0');
  return (left === right ? 0 : left < right ? -1 : 1) * a.sign;
}
