import { c, bold, dim } from '../../utils/colors.js';
import { select, input, checkbox } from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import {
  type ConfigOption,
  getAllTools,
  parseBooleanValue,
} from './config-data.js';

export async function editBooleanOption(
  option: ConfigOption,
  currentValue: string
): Promise<string | null> {
  const isEnabled = parseBooleanValue(currentValue);
  const currentStatus = isEnabled
    ? c('green', 'enabled')
    : c('dim', 'disabled');

  console.log();
  console.log(`  ${bold(option.name)}`);
  console.log(`  ${dim(option.description)}`);
  console.log(`  ${dim('Current:')} ${currentStatus}`);
  console.log();

  type BoolChoice = 'enable' | 'disable' | 'cancel';
  const choice = await select<BoolChoice>({
    message: `${option.name}:`,
    choices: [
      {
        name: `${c('green', '✅')} Enable`,
        value: 'enable',
      },
      {
        name: `${c('yellow', '○')} Disable`,
        value: 'disable',
      },
      separatorChoice<{ name: string; value: BoolChoice }>(),
      {
        name: `${c('dim', '- Cancel')}`,
        value: 'cancel',
      },
    ],
    loop: false,
  });

  if (choice === 'cancel') return null;
  return choice === 'enable' ? '1' : 'false';
}

export async function editStringOption(
  option: ConfigOption,
  currentValue: string
): Promise<string | null> {
  const displayCurrent =
    currentValue && currentValue !== option.defaultValue
      ? c('cyan', currentValue)
      : c('dim', currentValue || option.defaultValue);

  console.log();
  console.log(`  ${bold(option.name)}`);
  console.log(`  ${dim(option.description)}`);
  console.log(`  ${dim('Current:')} ${displayCurrent}`);
  console.log(`  ${dim('Default:')} ${option.defaultValue}`);
  console.log(`  ${dim('(Leave empty and press Enter to cancel)')}`);
  console.log();

  const newValue = await input({
    message: `${option.name}:`,
    default: '',
    validate: (value: string) => {
      if (!value.trim()) {
        return true;
      }
      if (
        option.validation?.pattern &&
        !option.validation.pattern.test(value)
      ) {
        return 'Invalid format';
      }
      return true;
    },
  });

  if (!newValue.trim()) {
    return null;
  }

  return newValue === option.defaultValue ? '' : newValue;
}

export async function editNumberOption(
  option: ConfigOption,
  currentValue: string
): Promise<string | null> {
  const displayCurrent =
    currentValue && currentValue !== option.defaultValue
      ? c('cyan', currentValue)
      : c('dim', currentValue || option.defaultValue);

  console.log();
  console.log(`  ${bold(option.name)}`);
  console.log(`  ${dim(option.description)}`);
  console.log(`  ${dim('Current:')} ${displayCurrent}`);
  if (
    option.validation?.min !== undefined ||
    option.validation?.max !== undefined
  ) {
    const min = option.validation?.min ?? 0;
    const max = option.validation?.max ?? Infinity;
    console.log(`  ${dim('Range:')} ${min} - ${max === Infinity ? '∞' : max}`);
  }
  console.log(`  ${dim('Default:')} ${option.defaultValue}`);
  console.log(`  ${dim('(Leave empty and press Enter to cancel)')}`);
  console.log();

  const newValue = await input({
    message: `${option.name}:`,
    default: '',
    validate: (value: string) => {
      if (!value.trim()) {
        return true;
      }
      const num = parseInt(value, 10);
      if (isNaN(num)) {
        return 'Please enter a valid number';
      }
      if (option.validation?.min !== undefined && num < option.validation.min) {
        return `Minimum value is ${option.validation.min}`;
      }
      if (option.validation?.max !== undefined && num > option.validation.max) {
        return `Maximum value is ${option.validation.max}`;
      }
      return true;
    },
  });

  if (!newValue.trim()) {
    return null;
  }

  return newValue === option.defaultValue ? '' : newValue;
}

export async function editArrayOption(
  option: ConfigOption,
  currentValue: string
): Promise<string | null> {
  const allTools = getAllTools();
  const currentTools = currentValue
    ? currentValue
        .split(',')
        .map(t => t.trim())
        .filter(Boolean)
    : [];

  const currentDisplay =
    currentTools.length > 0
      ? currentTools.join(', ')
      : option.id === 'toolsToRun'
        ? c('dim', '(all tools)')
        : c('dim', '(none)');

  console.log();
  console.log(`  ${bold(option.name)}`);
  console.log(`  ${dim(option.description)}`);
  console.log(`  ${dim('Current:')} ${currentDisplay}`);
  console.log();

  type ArrayAction = 'select' | 'clear' | 'cancel';
  const action = await select<ArrayAction>({
    message: `${option.name}:`,
    choices: [
      {
        name: 'EDIT Select tools',
        value: 'select',
        description: 'Choose which tools to include',
      },
      {
        name: `${c('yellow', '↺')} Clear all`,
        value: 'clear',
        description:
          option.id === 'toolsToRun'
            ? 'Reset to all tools enabled'
            : 'Remove all tools from this list',
      },
      separatorChoice<{ name: string; value: ArrayAction }>(),
      {
        name: `${c('dim', '- Cancel')}`,
        value: 'cancel',
      },
    ],
    loop: false,
  });

  if (action === 'cancel') {
    return null;
  }

  if (action === 'clear') {
    return '';
  }

  const choices: Array<{
    name: string;
    value: string;
    checked?: boolean;
    description?: string;
  }> = [];

  choices.push({
    name: c('blue', '── GitHub Tools ──'),
    value: '__separator_github__',
    disabled: true,
  } as unknown as (typeof choices)[number]);

  for (const tool of allTools.filter(t => t.category === 'github')) {
    choices.push({
      name: `${tool.name} ${c('dim', `(${tool.id})`)}`,
      value: tool.id,
      checked: currentTools.includes(tool.id),
      description: tool.description,
    });
  }

  choices.push({
    name: c('yellow', '── Local Tools ──'),
    value: '__separator_local__',
    disabled: true,
  } as unknown as (typeof choices)[number]);

  for (const tool of allTools.filter(t => t.category === 'local')) {
    choices.push({
      name: `${tool.name} ${c('dim', `(${tool.id})`)}`,
      value: tool.id,
      checked: currentTools.includes(tool.id),
      description: tool.description,
    });
  }

  choices.push({
    name: c('dim', '── Actions ──'),
    value: '__separator_actions__',
    disabled: true,
  } as unknown as (typeof choices)[number]);

  choices.push({
    name: `${c('dim', '- Cancel (keep current)')}`,
    value: '__cancel__',
    checked: false,
    description: 'Go back without changes',
  });

  console.log();
  console.log(
    `  ${dim('Use Space to select/deselect, Enter to confirm, or select Cancel')}`
  );

  const selected = await checkbox<string>({
    message: `Select tools for ${option.name}:`,
    choices,
    pageSize: 16,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('cyan', text),
        message: (text: string) => bold(text),
      },
    },
  });

  if (selected.includes('__cancel__')) {
    return null;
  }

  const validTools = selected.filter(
    t => !t.startsWith('__separator') && t !== '__cancel__'
  );
  return validTools.length > 0 ? validTools.join(',') : '';
}
