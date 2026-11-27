# JSON parse failure and exception policy

## Goal

Investigate the following scenario: The same malformed nested JSON produces an exception in one caller and a discarded value in another.

## Questions

- Why can the same malformed JSON throw an exception for one caller and return a discarded value for another?
- How do parse settings and SAX or filtering callbacks change the outcome?
- How do lexical errors, structural errors, and deliberate filtering differ?
