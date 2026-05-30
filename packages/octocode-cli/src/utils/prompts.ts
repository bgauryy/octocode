import {
  select as _select,
  confirm as _confirm,
  input as _input,
  checkbox as _checkbox,
  search as _search,
  Separator as _Separator,
} from '@inquirer/prompts';

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

type ConfirmFunction = (config: {
  message: string;
  default?: boolean;
}) => Promise<boolean>;

type InputFunction = (config: {
  message: string;
  default?: string;
  validate?: (value: string) => boolean | string | Promise<boolean | string>;
}) => Promise<string>;

type CheckboxFunction = <T>(config: {
  message: string;
  choices: Array<{
    name: string;
    value: T;
    checked?: boolean;
    disabled?: boolean | string;
    description?: string;
  }>;
  pageSize?: number;
  loop?: boolean;
  required?: boolean;
  theme?: {
    prefix?: string;
    style?: {
      highlight?: (text: string) => string;
      message?: (text: string) => string;
    };
  };
}) => Promise<T[]>;

type SearchFunction = <T>(config: {
  message: string;
  source: (
    term: string | undefined,
    opt: { signal: AbortSignal }
  ) =>
    | Promise<
        Array<{
          value: T;
          name?: string;
          description?: string;
          disabled?: boolean | string;
        }>
      >
    | Array<{
        value: T;
        name?: string;
        description?: string;
        disabled?: boolean | string;
      }>;
  pageSize?: number;
  theme?: {
    prefix?: string;
    style?: {
      highlight?: (text: string) => string;
      message?: (text: string) => string;
    };
  };
}) => Promise<T>;

type SeparatorInstance = {
  type: 'separator';
  separator: string;
};

type SeparatorClass = {
  new (separator?: string): SeparatorInstance;
};

export const select = _select as unknown as SelectFunction;
export const confirm = _confirm as unknown as ConfirmFunction;
export const input = _input as unknown as InputFunction;
export const checkbox = _checkbox as unknown as CheckboxFunction;
export const search = _search as unknown as SearchFunction;
export const Separator = _Separator as unknown as SeparatorClass;

export async function loadInquirer(): Promise<void> {}

export function isInquirerLoaded(): boolean {
  return true;
}

export async function selectWithCancel<T>(config: SelectConfig<T>): Promise<T> {
  return (await (_select as (cfg: SelectConfig<T>) => Promise<T>))(config);
}
