# SCSS (.scss)

Source sample: `scss/_buttons.scss`

Strategy: `aggressive`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 7057 | - | - |
| content-view | 5411 | 23.3% | 0.323 ms |
| applyMinification | 5411 | 23.3% | 0.347 ms |
| sync minify | 5411 | 23.3% | 0.271 ms |
| async minify | 5411 | 23.3% | 0.285 ms |
| symbols | 1523 | 78.4% | 0.032 ms |

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
