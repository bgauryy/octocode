/** Resolve an explicit copy/move target directory before the positional destination. */
export function shellCopyDestination(args: string[]): string | undefined {
  const operands: string[] = [];
  let destination: string | undefined;
  let endOptions = false;
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index]!;
    if (!endOptions && arg === '--') { endOptions = true; continue; }
    if (!endOptions && (arg === '-t' || arg === '--target-directory')) { destination = args[++index]; continue; }
    if (!endOptions && arg.startsWith('--target-directory=')) { destination = arg.slice('--target-directory='.length); continue; }
    if (!endOptions && arg.startsWith('-t') && arg.length > 2) { destination = arg.slice(2); continue; }
    if (!endOptions && arg.startsWith('-')) continue;
    operands.push(arg);
  }
  return destination ?? operands.at(-1);
}
