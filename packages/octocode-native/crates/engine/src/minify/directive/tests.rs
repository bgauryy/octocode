use super::is_directive;

#[test]
fn comments_a_tool_reads_are_directives() {
    for comment in [
        "//go:build linux",
        "// +build linux",
        "//go:embed data.txt",
        "//nolint:errcheck",
        "//export Add",
        "// @ts-ignore",
        "/// <reference path=\"./types.d.ts\" />",
        "// eslint-disable-next-line no-console",
        "/* webpackChunkName: \"foo\" */",
        "/* @__PURE__ */",
        "# type: ignore",
        "# noqa: F401",
        "# -*- coding: utf-8 -*-",
        "// #include <stdio.h>",
        "/*\n#include <stdio.h>\n*/",
        "/** @type {number} */",
    ] {
        assert!(is_directive(comment), "{comment}");
    }
}

#[test]
fn prose_is_not_a_directive() {
    for comment in [
        "// a comment",
        "// SAFETY: the pointer is valid",
        "// Package p docs.",
        "/// doc",
        "//! Crate docs",
        "/* block\n   foo();\n*/",
        "/**\n * Service number 1.\n * @param x\n */",
        "# comment between statements",
        "// TODO: later",
        "/*! license keep? */",
    ] {
        assert!(!is_directive(comment), "{comment}");
    }
}
