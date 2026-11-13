# Deserialization conversion error recovery

## Goal

Investigate the following scenario: One property contains a value that cannot convert to its declared type.

## Questions

- What state remains when a property conversion fails during deserialization?
- How does handled-error recovery affect reader position and later properties?
- How do user-converter failures differ from built-in conversion failures?
