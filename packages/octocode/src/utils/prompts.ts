import { select as _select } from '@inquirer/prompts';

type SelectConfig<T> = {
  message: string;
  choices: Array<
    | {
        name: string;
        value: T;
        description?: string;
      }
    | { type: 'separator'; separator?: string }
  >;
  pageSize?: number;
  loop?: boolean;
  theme?: {
    prefix?: string;
    style?: {
      highlight?: (text: string) => string;
      message?: (text: string) => string;
    };
  };
};

type SelectFunction = <T>(config: SelectConfig<T>) => Promise<T>;

export const select = _select as unknown as SelectFunction;
