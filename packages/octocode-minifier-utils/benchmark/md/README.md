# Markdown (.md)

Source sample: `md/rust-readme.md`

Strategy: `markdown`

Agent rating: **5.5/10 (needs work)**

Agent understanding from minified output: **9.2/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 3304 | - | - | - |
| content-view | 3303 | 0% | 0.278 ms | 5.5/10 |
| applyMinification | 3303 | 0% | 0.249 ms | 5.5/10 |
| sync minify | 3303 | 0% | 0.242 ms | 5.5/10 |
| async minify | 3303 | 0% | 0.272 ms | 5.5/10 |
| symbols | n/a | n/a | 0.003 ms | n/a |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 5/10 |
| symbol context | 7/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 3304 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 3303 | 0% | 9.2/10 excellent | 10/10 | 10/10 |
| minify | 3303 | 0% | 9.2/10 excellent | 10/10 | 10/10 |
| symbols | n/a | n/a | n/a | n/a | n/a |

## Notes

- markdown text strategy.
- content-view kept original because the readable output was not shorter.
- symbols are not implemented for this extension.

## Before Excerpt

```markdown
<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg">
    <img alt="The Rust Programming Language: A language empowering everyone to build reliable and efficient software"
         src="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg"
         width="50%">
  </picture>

[Website][Rust] | [Getting started] | [Learn] | [Documentation] | [Contributing]
</div>

This is the main source code repository for [Rust]. It contains the compiler,
standard library, and documentation.

[Rust]: https://www.rust-lang.org/
[Getting Started]: https://www.rust-lang.org/learn/get-started
[Learn]: https://www.rust-lang.org/learn
[Documentation]: https://www.rust-lang.org/learn#learn-use
[Contributing]: CONTRIBUTING.md

## Why Rust?

- **Performance:** Fast and memory-efficient, suitable for critical services, embedded devices, and easily integrated with o

... [truncated 1504 chars] ...

) for details.

## Trademark

[The Rust Foundation][rust-foundation] owns and protects the Rust and Cargo
trademarks and logos (the "Rust Trademarks").

If you want to use these names or brands, please read the
[Rust language trademark policy][trademark-policy].

Third-party logos may be subject to third-party copyrights and trademarks. See
[Licenses][policies-licenses] for details.

[rust-foundation]: https://rustfoundation.org/
[trademark-policy]: https://rustfoundation.org/policy/rust-trademark-policy/
[policies-licenses]: https://www.rust-lang.org/policies/licenses

```

## Content-View Excerpt

```markdown
<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg">
    <img alt="The Rust Programming Language: A language empowering everyone to build reliable and efficient software"
         src="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg"
         width="50%">
  </picture>

[Website][Rust] | [Getting started] | [Learn] | [Documentation] | [Contributing]
</div>

This is the main source code repository for [Rust]. It contains the compiler,
standard library, and documentation.

[Rust]: https://www.rust-lang.org/
[Getting Started]: https://www.rust-lang.org/learn/get-started
[Learn]: https://www.rust-lang.org/learn
[Documentation]: https://www.rust-lang.org/learn#learn-use
[Contributing]: CONTRIBUTING.md

## Why Rust?

- **Performance:** Fast and memory-efficient, suitable for critical services, embedded devices, and easily integrated with o

... [truncated 1503 chars] ...

T) for details.

## Trademark

[The Rust Foundation][rust-foundation] owns and protects the Rust and Cargo
trademarks and logos (the "Rust Trademarks").

If you want to use these names or brands, please read the
[Rust language trademark policy][trademark-policy].

Third-party logos may be subject to third-party copyrights and trademarks. See
[Licenses][policies-licenses] for details.

[rust-foundation]: https://rustfoundation.org/
[trademark-policy]: https://rustfoundation.org/policy/rust-trademark-policy/
[policies-licenses]: https://www.rust-lang.org/policies/licenses
```

## Apply Minification Excerpt

```markdown
<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg">
    <img alt="The Rust Programming Language: A language empowering everyone to build reliable and efficient software"
         src="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg"
         width="50%">
  </picture>

[Website][Rust] | [Getting started] | [Learn] | [Documentation] | [Contributing]
</div>

This is the main source code repository for [Rust]. It contains the compiler,
standard library, and documentation.

[Rust]: https://www.rust-lang.org/
[Getting Started]: https://www.rust-lang.org/learn/get-started
[Learn]: https://www.rust-lang.org/learn
[Documentation]: https://www.rust-lang.org/learn#learn-use
[Contributing]: CONTRIBUTING.md

## Why Rust?

- **Performance:** Fast and memory-efficient, suitable for critical services, embedded devices, and easily integrated with o

... [truncated 1503 chars] ...

T) for details.

## Trademark

[The Rust Foundation][rust-foundation] owns and protects the Rust and Cargo
trademarks and logos (the "Rust Trademarks").

If you want to use these names or brands, please read the
[Rust language trademark policy][trademark-policy].

Third-party logos may be subject to third-party copyrights and trademarks. See
[Licenses][policies-licenses] for details.

[rust-foundation]: https://rustfoundation.org/
[trademark-policy]: https://rustfoundation.org/policy/rust-trademark-policy/
[policies-licenses]: https://www.rust-lang.org/policies/licenses
```

## Sync Minify Excerpt

```markdown
<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg">
    <img alt="The Rust Programming Language: A language empowering everyone to build reliable and efficient software"
         src="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg"
         width="50%">
  </picture>

[Website][Rust] | [Getting started] | [Learn] | [Documentation] | [Contributing]
</div>

This is the main source code repository for [Rust]. It contains the compiler,
standard library, and documentation.

[Rust]: https://www.rust-lang.org/
[Getting Started]: https://www.rust-lang.org/learn/get-started
[Learn]: https://www.rust-lang.org/learn
[Documentation]: https://www.rust-lang.org/learn#learn-use
[Contributing]: CONTRIBUTING.md

## Why Rust?

- **Performance:** Fast and memory-efficient, suitable for critical services, embedded devices, and easily integrated with o

... [truncated 1503 chars] ...

T) for details.

## Trademark

[The Rust Foundation][rust-foundation] owns and protects the Rust and Cargo
trademarks and logos (the "Rust Trademarks").

If you want to use these names or brands, please read the
[Rust language trademark policy][trademark-policy].

Third-party logos may be subject to third-party copyrights and trademarks. See
[Licenses][policies-licenses] for details.

[rust-foundation]: https://rustfoundation.org/
[trademark-policy]: https://rustfoundation.org/policy/rust-trademark-policy/
[policies-licenses]: https://www.rust-lang.org/policies/licenses
```

## Async Minify Excerpt

```markdown
<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg">
    <img alt="The Rust Programming Language: A language empowering everyone to build reliable and efficient software"
         src="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg"
         width="50%">
  </picture>

[Website][Rust] | [Getting started] | [Learn] | [Documentation] | [Contributing]
</div>

This is the main source code repository for [Rust]. It contains the compiler,
standard library, and documentation.

[Rust]: https://www.rust-lang.org/
[Getting Started]: https://www.rust-lang.org/learn/get-started
[Learn]: https://www.rust-lang.org/learn
[Documentation]: https://www.rust-lang.org/learn#learn-use
[Contributing]: CONTRIBUTING.md

## Why Rust?

- **Performance:** Fast and memory-efficient, suitable for critical services, embedded devices, and easily integrated with o

... [truncated 1503 chars] ...

T) for details.

## Trademark

[The Rust Foundation][rust-foundation] owns and protects the Rust and Cargo
trademarks and logos (the "Rust Trademarks").

If you want to use these names or brands, please read the
[Rust language trademark policy][trademark-policy].

Third-party logos may be subject to third-party copyrights and trademarks. See
[Licenses][policies-licenses] for details.

[rust-foundation]: https://rustfoundation.org/
[trademark-policy]: https://rustfoundation.org/policy/rust-trademark-policy/
[policies-licenses]: https://www.rust-lang.org/policies/licenses
```

## Symbols

```txt
No symbols returned for this sample.
```
