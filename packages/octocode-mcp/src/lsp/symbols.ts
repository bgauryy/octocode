import { SymbolKind as LSPSymbolKind } from 'vscode-languageserver-protocol';
import type { SymbolKind } from './types.js';

export function convertSymbolKind(kind: LSPSymbolKind): SymbolKind {
  switch (kind) {
    case LSPSymbolKind.Function:
      return 'function';
    case LSPSymbolKind.Method:
      return 'method';
    case LSPSymbolKind.Constructor:
      return 'method';
    case LSPSymbolKind.Class:
      return 'class';
    case LSPSymbolKind.Struct:
      return 'class';
    case LSPSymbolKind.Interface:
      return 'interface';
    case LSPSymbolKind.Variable:
      return 'variable';
    case LSPSymbolKind.Constant:
      return 'constant';
    case LSPSymbolKind.Property:
    case LSPSymbolKind.Field:
      return 'property';
    case LSPSymbolKind.Enum:
      return 'enum';
    case LSPSymbolKind.EnumMember:
      return 'constant';
    case LSPSymbolKind.Module:
    case LSPSymbolKind.Package:
    case LSPSymbolKind.File:
      return 'module';
    case LSPSymbolKind.Namespace:
      return 'namespace';
    case LSPSymbolKind.TypeParameter:
      return 'type';
    default:
      return 'unknown';
  }
}

export function toLSPSymbolKind(kind: SymbolKind): LSPSymbolKind {
  switch (kind) {
    case 'function':
      return LSPSymbolKind.Function;
    case 'method':
      return LSPSymbolKind.Method;
    case 'class':
      return LSPSymbolKind.Class;
    case 'interface':
      return LSPSymbolKind.Interface;
    case 'variable':
      return LSPSymbolKind.Variable;
    case 'constant':
      return LSPSymbolKind.Constant;
    case 'property':
      return LSPSymbolKind.Property;
    case 'enum':
      return LSPSymbolKind.Enum;
    case 'module':
      return LSPSymbolKind.Module;
    case 'namespace':
      return LSPSymbolKind.Namespace;
    case 'type':
      return LSPSymbolKind.TypeParameter;
    default:
      return LSPSymbolKind.Function;
  }
}
