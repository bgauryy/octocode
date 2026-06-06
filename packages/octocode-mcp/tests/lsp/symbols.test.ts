import { describe, it, expect } from 'vitest';
import { SymbolKind as LSPSymbolKind } from 'vscode-languageserver-protocol';
import { convertSymbolKind, toLSPSymbolKind } from '../../src/lsp/symbols.js';

describe('LSP Symbol Kind Conversion', () => {
  describe('convertSymbolKind', () => {
    it('should convert Function to "function"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Function)).toBe('function');
    });

    it('should convert Method to "method"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Method)).toBe('method');
    });

    it('should convert Class to "class"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Class)).toBe('class');
    });

    it('should convert Interface to "interface"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Interface)).toBe('interface');
    });

    it('should convert Variable to "variable"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Variable)).toBe('variable');
    });

    it('should convert Constant to "constant"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Constant)).toBe('constant');
    });

    it('should convert Property to "property"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Property)).toBe('property');
    });

    it('should convert Enum to "enum"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Enum)).toBe('enum');
    });

    it('should convert Module to "module"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Module)).toBe('module');
    });

    it('should convert Namespace to "namespace"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Namespace)).toBe('namespace');
    });

    it('should convert TypeParameter to "type"', () => {
      expect(convertSymbolKind(LSPSymbolKind.TypeParameter)).toBe('type');
    });

    it('should return "unknown" for unrecognized kinds', () => {
      expect(convertSymbolKind(99 as LSPSymbolKind)).toBe('unknown');
    });

    it('should map Constructor to "method"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Constructor)).toBe('method');
    });

    it('should map Struct to "class"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Struct)).toBe('class');
    });

    it('should map Field to "property"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Field)).toBe('property');
    });

    it('should map EnumMember to "constant"', () => {
      expect(convertSymbolKind(LSPSymbolKind.EnumMember)).toBe('constant');
    });

    it('should map Package to "module"', () => {
      expect(convertSymbolKind(LSPSymbolKind.Package)).toBe('module');
    });

    it('should map File to "module"', () => {
      expect(convertSymbolKind(LSPSymbolKind.File)).toBe('module');
    });
  });

  describe('toLSPSymbolKind', () => {
    it('should convert "function" to Function', () => {
      expect(toLSPSymbolKind('function')).toBe(LSPSymbolKind.Function);
    });

    it('should convert "method" to Method', () => {
      expect(toLSPSymbolKind('method')).toBe(LSPSymbolKind.Method);
    });

    it('should convert "class" to Class', () => {
      expect(toLSPSymbolKind('class')).toBe(LSPSymbolKind.Class);
    });

    it('should convert "interface" to Interface', () => {
      expect(toLSPSymbolKind('interface')).toBe(LSPSymbolKind.Interface);
    });

    it('should convert "variable" to Variable', () => {
      expect(toLSPSymbolKind('variable')).toBe(LSPSymbolKind.Variable);
    });

    it('should convert "constant" to Constant', () => {
      expect(toLSPSymbolKind('constant')).toBe(LSPSymbolKind.Constant);
    });

    it('should convert "property" to Property', () => {
      expect(toLSPSymbolKind('property')).toBe(LSPSymbolKind.Property);
    });

    it('should convert "enum" to Enum', () => {
      expect(toLSPSymbolKind('enum')).toBe(LSPSymbolKind.Enum);
    });

    it('should convert "module" to Module', () => {
      expect(toLSPSymbolKind('module')).toBe(LSPSymbolKind.Module);
    });

    it('should convert "namespace" to Namespace', () => {
      expect(toLSPSymbolKind('namespace')).toBe(LSPSymbolKind.Namespace);
    });

    it('should convert "type" to TypeParameter', () => {
      expect(toLSPSymbolKind('type')).toBe(LSPSymbolKind.TypeParameter);
    });

    it('should return Function as default for "unknown"', () => {
      expect(toLSPSymbolKind('unknown')).toBe(LSPSymbolKind.Function);
    });

    it('should return Function as default for unrecognized strings', () => {
      expect(toLSPSymbolKind('invalid' as any)).toBe(LSPSymbolKind.Function);
    });
  });

  describe('round-trip conversion', () => {
    const symbolKinds: LSPSymbolKind[] = [
      LSPSymbolKind.Function,
      LSPSymbolKind.Method,
      LSPSymbolKind.Class,
      LSPSymbolKind.Interface,
      LSPSymbolKind.Variable,
      LSPSymbolKind.Constant,
      LSPSymbolKind.Property,
      LSPSymbolKind.Enum,
      LSPSymbolKind.Module,
      LSPSymbolKind.Namespace,
      LSPSymbolKind.TypeParameter,
    ];

    it.each(symbolKinds)(
      'should round-trip LSPSymbolKind %i correctly',
      kind => {
        const internal = convertSymbolKind(kind);
        const backToLSP = toLSPSymbolKind(internal);
        expect(backToLSP).toBe(kind);
      }
    );
  });
});
