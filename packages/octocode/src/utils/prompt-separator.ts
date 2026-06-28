import { Separator } from './prompts.js';

export function separatorChoice<TChoice>(line?: string): TChoice {
  return new Separator(line) as unknown as TChoice;
}
