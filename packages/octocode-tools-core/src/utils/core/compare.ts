export function compareIsoDateDescending(
  left?: string,
  right?: string
): number {
  if (!left && !right) return 0;
  if (!left) return 1;
  if (!right) return -1;

  const leftTime = Date.parse(left);
  const rightTime = Date.parse(right);
  if (Number.isNaN(leftTime) && Number.isNaN(rightTime)) return 0;
  if (Number.isNaN(leftTime)) return 1;
  if (Number.isNaN(rightTime)) return -1;
  return rightTime - leftTime;
}
