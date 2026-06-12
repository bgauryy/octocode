# SCSS (.scss)

Source sample: `scss/_buttons.scss`

Strategy: `aggressive`

Agent rating: **8.5/10 (strong)**

Agent understanding from minified output: **9.6/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 7057 | - | - | - |
| content-view | 5411 | 23.3% | 2.088 ms | 8.3/10 |
| applyMinification | 5411 | 23.3% | 2.044 ms | 8.3/10 |
| sync minify | 5411 | 23.3% | 1.892 ms | 8.3/10 |
| async minify | 5411 | 23.3% | 1.939 ms | 8.3/10 |
| symbols | 1523 | 78.4% | 0.271 ms | 9/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 9/10 |
| context budget | 8/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 7057 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 5411 | 23.3% | 9.6/10 excellent | 10/10 | 10/10 |
| minify | 5411 | 23.3% | 9.6/10 excellent | 10/10 | 10/10 |
| symbols | 1523 | 78.4% | 8.1/10 strong | 6.7/10 | 7.1/10 |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```scss
//
// Base styles
//

.btn {
  // scss-docs-start btn-css-vars
  --#{$prefix}btn-padding-x: #{$btn-padding-x};
  --#{$prefix}btn-padding-y: #{$btn-padding-y};
  --#{$prefix}btn-font-family: #{$btn-font-family};
  @include rfs($btn-font-size, --#{$prefix}btn-font-size);
  --#{$prefix}btn-font-weight: #{$btn-font-weight};
  --#{$prefix}btn-line-height: #{$btn-line-height};
  --#{$prefix}btn-color: #{$btn-color};
  --#{$prefix}btn-bg: transparent;
  --#{$prefix}btn-border-width: #{$btn-border-width};
  --#{$prefix}btn-border-color: transparent;
  --#{$prefix}btn-border-radius: #{$btn-border-radius};
  --#{$prefix}btn-hover-border-color: transparent;
  --#{$prefix}btn-box-shadow: #{$btn-box-shadow};
  --#{$prefix}btn-disabled-opacity: #{$btn-disabled-opacity};
  --#{$prefix}btn-focus-box-shadow: 0 0 0 #{$btn-focus-width} rgba(var(--#{$prefix}btn-focus-shadow-rgb), .5);
  // scss-docs-end btn-css-vars

  display: inline-block;
  padding: var(--#{$prefix}btn-padding-y) var(--#{$prefix}btn-padding-x);
  font-family: var(--#{$prefix}btn-font-family);
  @include font-size(var(--#{$prefix}btn-font-size));
  font-weight: var(--#{$prefix}btn-font-weight);
  line-height: var(--#{$prefix}btn-line-height);
  color: var(

... [truncated 5257 chars] ...

decoration;
  @if $enable-gradients {
    background-image: none;
  }

  &:hover,
  &:focus-visible {
    text-decoration: $link-hover-decoration;
  }

  &:focus-visible {
    color: var(--#{$prefix}btn-color);
  }

  &:hover {
    color: var(--#{$prefix}btn-hover-color);
  }

  // No need for an active state here
}


//
// Button Sizes
//

.btn-lg {
  @include button-size($btn-padding-y-lg, $btn-padding-x-lg, $btn-font-size-lg, $btn-border-radius-lg);
}

.btn-sm {
  @include button-size($btn-padding-y-sm, $btn-padding-x-sm, $btn-font-size-sm, $btn-border-radius-sm);
}

```

## Content-View Excerpt

```scss
.btn{--#{$prefix}btn-padding-x:#{$btn-padding-x};--#{$prefix}btn-padding-y:#{$btn-padding-y};--#{$prefix}btn-font-family:#{$btn-font-family};@include rfs($btn-font-size,--#{$prefix}btn-font-size);--#{$prefix}btn-font-weight:#{$btn-font-weight};--#{$prefix}btn-line-height:#{$btn-line-height};--#{$prefix}btn-color:#{$btn-color};--#{$prefix}btn-bg:transparent;--#{$prefix}btn-border-width:#{$btn-border-width};--#{$prefix}btn-border-color:transparent;--#{$prefix}btn-border-radius:#{$btn-border-radius};--#{$prefix}btn-hover-border-color:transparent;--#{$prefix}btn-box-shadow:#{$btn-box-shadow};--#{$prefix}btn-disabled-opacity:#{$btn-disabled-opacity};--#{$prefix}btn-focus-box-shadow:0 0 0 #{$btn-focus-width}rgba(var(--#{$prefix}btn-focus-shadow-rgb),.5);display:inline-block;padding:var(--#{$prefix}btn-padding-y) var(--#{$prefix}btn-padding-x);font-family:var(--#{$prefix}btn-font-family);@include font-size(var(--#{$prefix}btn-font-size));font-weight:var(--#{$prefix}btn-font-weight);line-height:var(--#{$prefix}btn-line-height);color:var(--#{$prefix}btn-color);text-align:center;text-decoration:if($link-decoration == none,null,none);white-space:$btn-white-space;vertical-align:middle;cursor:if($enable-button-pointer

... [truncated 3611 chars] ...

r-color:transparent;--#{$prefix}btn-box-shadow:0 0 0 #000;--#{$prefix}btn-focus-shadow-rgb:#{$btn-link-focus-shadow-rgb};text-decoration:$link-decoration;@if $enable-gradients{background-image:none;}&:hover,&:focus-visible{text-decoration:$link-hover-decoration;}&:focus-visible{color:var(--#{$prefix}btn-color);}&:hover{color:var(--#{$prefix}btn-hover-color);}}.btn-lg{@include button-size($btn-padding-y-lg,$btn-padding-x-lg,$btn-font-size-lg,$btn-border-radius-lg);}.btn-sm{@include button-size($btn-padding-y-sm,$btn-padding-x-sm,$btn-font-size-sm,$btn-border-radius-sm);}
```

## Apply Minification Excerpt

```scss
.btn{--#{$prefix}btn-padding-x:#{$btn-padding-x};--#{$prefix}btn-padding-y:#{$btn-padding-y};--#{$prefix}btn-font-family:#{$btn-font-family};@include rfs($btn-font-size,--#{$prefix}btn-font-size);--#{$prefix}btn-font-weight:#{$btn-font-weight};--#{$prefix}btn-line-height:#{$btn-line-height};--#{$prefix}btn-color:#{$btn-color};--#{$prefix}btn-bg:transparent;--#{$prefix}btn-border-width:#{$btn-border-width};--#{$prefix}btn-border-color:transparent;--#{$prefix}btn-border-radius:#{$btn-border-radius};--#{$prefix}btn-hover-border-color:transparent;--#{$prefix}btn-box-shadow:#{$btn-box-shadow};--#{$prefix}btn-disabled-opacity:#{$btn-disabled-opacity};--#{$prefix}btn-focus-box-shadow:0 0 0 #{$btn-focus-width}rgba(var(--#{$prefix}btn-focus-shadow-rgb),.5);display:inline-block;padding:var(--#{$prefix}btn-padding-y) var(--#{$prefix}btn-padding-x);font-family:var(--#{$prefix}btn-font-family);@include font-size(var(--#{$prefix}btn-font-size));font-weight:var(--#{$prefix}btn-font-weight);line-height:var(--#{$prefix}btn-line-height);color:var(--#{$prefix}btn-color);text-align:center;text-decoration:if($link-decoration == none,null,none);white-space:$btn-white-space;vertical-align:middle;cursor:if($enable-button-pointer

... [truncated 3611 chars] ...

r-color:transparent;--#{$prefix}btn-box-shadow:0 0 0 #000;--#{$prefix}btn-focus-shadow-rgb:#{$btn-link-focus-shadow-rgb};text-decoration:$link-decoration;@if $enable-gradients{background-image:none;}&:hover,&:focus-visible{text-decoration:$link-hover-decoration;}&:focus-visible{color:var(--#{$prefix}btn-color);}&:hover{color:var(--#{$prefix}btn-hover-color);}}.btn-lg{@include button-size($btn-padding-y-lg,$btn-padding-x-lg,$btn-font-size-lg,$btn-border-radius-lg);}.btn-sm{@include button-size($btn-padding-y-sm,$btn-padding-x-sm,$btn-font-size-sm,$btn-border-radius-sm);}
```

## Sync Minify Excerpt

```scss
.btn{--#{$prefix}btn-padding-x:#{$btn-padding-x};--#{$prefix}btn-padding-y:#{$btn-padding-y};--#{$prefix}btn-font-family:#{$btn-font-family};@include rfs($btn-font-size,--#{$prefix}btn-font-size);--#{$prefix}btn-font-weight:#{$btn-font-weight};--#{$prefix}btn-line-height:#{$btn-line-height};--#{$prefix}btn-color:#{$btn-color};--#{$prefix}btn-bg:transparent;--#{$prefix}btn-border-width:#{$btn-border-width};--#{$prefix}btn-border-color:transparent;--#{$prefix}btn-border-radius:#{$btn-border-radius};--#{$prefix}btn-hover-border-color:transparent;--#{$prefix}btn-box-shadow:#{$btn-box-shadow};--#{$prefix}btn-disabled-opacity:#{$btn-disabled-opacity};--#{$prefix}btn-focus-box-shadow:0 0 0 #{$btn-focus-width}rgba(var(--#{$prefix}btn-focus-shadow-rgb),.5);display:inline-block;padding:var(--#{$prefix}btn-padding-y) var(--#{$prefix}btn-padding-x);font-family:var(--#{$prefix}btn-font-family);@include font-size(var(--#{$prefix}btn-font-size));font-weight:var(--#{$prefix}btn-font-weight);line-height:var(--#{$prefix}btn-line-height);color:var(--#{$prefix}btn-color);text-align:center;text-decoration:if($link-decoration == none,null,none);white-space:$btn-white-space;vertical-align:middle;cursor:if($enable-button-pointer

... [truncated 3611 chars] ...

r-color:transparent;--#{$prefix}btn-box-shadow:0 0 0 #000;--#{$prefix}btn-focus-shadow-rgb:#{$btn-link-focus-shadow-rgb};text-decoration:$link-decoration;@if $enable-gradients{background-image:none;}&:hover,&:focus-visible{text-decoration:$link-hover-decoration;}&:focus-visible{color:var(--#{$prefix}btn-color);}&:hover{color:var(--#{$prefix}btn-hover-color);}}.btn-lg{@include button-size($btn-padding-y-lg,$btn-padding-x-lg,$btn-font-size-lg,$btn-border-radius-lg);}.btn-sm{@include button-size($btn-padding-y-sm,$btn-padding-x-sm,$btn-font-size-sm,$btn-border-radius-sm);}
```

## Async Minify Excerpt

```scss
.btn{--#{$prefix}btn-padding-x:#{$btn-padding-x};--#{$prefix}btn-padding-y:#{$btn-padding-y};--#{$prefix}btn-font-family:#{$btn-font-family};@include rfs($btn-font-size,--#{$prefix}btn-font-size);--#{$prefix}btn-font-weight:#{$btn-font-weight};--#{$prefix}btn-line-height:#{$btn-line-height};--#{$prefix}btn-color:#{$btn-color};--#{$prefix}btn-bg:transparent;--#{$prefix}btn-border-width:#{$btn-border-width};--#{$prefix}btn-border-color:transparent;--#{$prefix}btn-border-radius:#{$btn-border-radius};--#{$prefix}btn-hover-border-color:transparent;--#{$prefix}btn-box-shadow:#{$btn-box-shadow};--#{$prefix}btn-disabled-opacity:#{$btn-disabled-opacity};--#{$prefix}btn-focus-box-shadow:0 0 0 #{$btn-focus-width}rgba(var(--#{$prefix}btn-focus-shadow-rgb),.5);display:inline-block;padding:var(--#{$prefix}btn-padding-y) var(--#{$prefix}btn-padding-x);font-family:var(--#{$prefix}btn-font-family);@include font-size(var(--#{$prefix}btn-font-size));font-weight:var(--#{$prefix}btn-font-weight);line-height:var(--#{$prefix}btn-line-height);color:var(--#{$prefix}btn-color);text-align:center;text-decoration:if($link-decoration == none,null,none);white-space:$btn-white-space;vertical-align:middle;cursor:if($enable-button-pointer

... [truncated 3611 chars] ...

r-color:transparent;--#{$prefix}btn-box-shadow:0 0 0 #000;--#{$prefix}btn-focus-shadow-rgb:#{$btn-link-focus-shadow-rgb};text-decoration:$link-decoration;@if $enable-gradients{background-image:none;}&:hover,&:focus-visible{text-decoration:$link-hover-decoration;}&:focus-visible{color:var(--#{$prefix}btn-color);}&:hover{color:var(--#{$prefix}btn-hover-color);}}.btn-lg{@include button-size($btn-padding-y-lg,$btn-padding-x-lg,$btn-font-size-lg,$btn-border-radius-lg);}.btn-sm{@include button-size($btn-padding-y-sm,$btn-padding-x-sm,$btn-font-size-sm,$btn-border-radius-sm);}
```

## Symbols

```txt
  5| .btn {
 10|   @include rfs($btn-font-size, --#{$prefix}btn-font-size);
 27|   @include font-size(var(--#{$prefix}btn-font-size));
 38|   @include border-radius(var(--#{$prefix}btn-border-radius));
 39|   @include gradient-bg(var(--#{$prefix}btn-bg));
 40|   @include box-shadow(var(--#{$prefix}btn-box-shadow));
 41|   @include transition($btn-transition);
 43|   &:hover {
 50|   .btn-check + &:hover {
 57|   &:focus-visible {
 59|     @include gradient-bg(var(--#{$prefix}btn-hover-bg));
 63|     @if $enable-shadows {
 70|   .btn-check:focus-visible + & {
 74|     @if $enable-shadows {
 85|   &.show {
 91|     @include box-shadow(var(--#{$prefix}btn-active-shadow));
 93|     &:focus-visible {
 95|       @if $enable-shadows {
103|   .btn-check:checked:focus-visible + & {
105|     @if $enable-shadows {
114|   fieldset:disabled & {
121|     @include box-shadow(none);
131| @each $color, $value in $theme-colors {
132|   .btn-#{$color} {
133|     @if $color == "light" {
152|       @include button-variant($value, $value);
157| @each $color, $value in $theme-colors {
158|   .btn-outline-#{$color} {
159|     @include button-outline-variant($value);
170| .btn-link {
185|   @if $enable-gradients {
190|   &:focus-visible {
194|   &:focus-visible {
198|   &:hover {
210| .btn-lg {
211|   @include button-size($btn-padding-y-lg, $btn-padding-x-lg, $btn-font-size-lg, $btn-border-radius-lg);
214| .btn-sm {
215|   @include button-size($btn-padding-y-sm, $btn-padding-x-sm, $btn-font-size-sm, $btn-border-radius-sm);
```
