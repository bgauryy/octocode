# ORM joins change aggregate semantics

## Goal

Investigate the following scenario: A Django queryset combines reverse-relation filtering with annotation and returns unexpected counts.

## Questions

- Why can a Django annotation combined with reverse-relation filtering produce unexpected counts?
- How does changing the order of annotation and filtering change SQL semantics?
- When does distinct express the intended query, and when does it conceal a different problem?
