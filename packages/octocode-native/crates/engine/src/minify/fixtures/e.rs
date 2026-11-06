//! Crate docs
#![allow(dead_code)]
use std::fmt;

// a comment
/* nested /* inner */ still comment */
#[derive(Debug)]
pub struct W<'a> {
    pub s: &'a str, // field
}

impl<'a> W<'a> {
    /// doc
    pub fn get(&self) -> &'a str {
        let c = '"'; // quote char
        let url = "http://x/y  z"; // url
        let r = r#"raw // not comment "# ;
        // SAFETY: the pointer is valid
        unsafe { std::ptr::read(&self.s) }
    }
    pub fn msg(&'a self) -> &'static str {
        "it's // not a comment"
    }
}
fn label() { 'outer: loop { break 'outer; } }
fn last() -> &'static str { "tail // keep me" }
