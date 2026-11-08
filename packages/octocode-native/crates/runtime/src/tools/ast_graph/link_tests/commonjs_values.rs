//! CommonJS values whose members are written by property assignment, the UMD
//! wrapper that exports them, and the guarded loader that requires them
//! (B122, the lodash shapes): each links through identifier chains of the
//! file that writes them, at medium where a branch decides whether the load
//! or the export runs.
use super::*;

const B122: &[(&str, &str)] = &[
    (
        "m_plain.js",
        "function isObject(v) { return v != null; }\nmodule.exports = { isObject: isObject };\n",
    ),
    (
        "m_propassign.js",
        "function isObject(v) { return v != null; }\nfunction lodash() {}\nlodash.isObject = isObject;\nmodule.exports = lodash;\n",
    ),
    (
        "m_umd_return.js",
        "function runInContext() {\n  function isObject(v) { return v != null; }\n  function lodash() {}\n  lodash.isObject = isObject;\n  return lodash;\n}\nvar _ = runInContext();\nmodule.exports = _;\n",
    ),
    (
        "m_umd_free.js",
        ";(function() {\n  var freeExports = typeof exports == 'object' && exports;\n  var freeModule = freeExports && typeof module == 'object' && module && !module.nodeType && module;\n  var runInContext = (function runInContext(context) {\n    function isObject(v) { return v != null; }\n    function lodash() {}\n    lodash.isObject = isObject;\n    return lodash;\n  });\n  var _ = runInContext();\n  if (typeof define == 'function' && define.amd) { define(function() { return _; }); }\n  else if (freeModule) { (freeModule.exports = _)._ = _; freeExports._ = _; }\n}.call(this));\n",
    ),
    ("c1_plain.js", "var a = require('./m_plain');\na.isObject(1);\n"),
    (
        "c2_guard_plain.js",
        "var root = this;\nvar a = root._ || require('./m_plain');\na.isObject(1);\n",
    ),
    ("c3_propassign.js", "var a = require('./m_propassign');\na.isObject(1);\n"),
    ("c4_umd_return.js", "var a = require('./m_umd_return');\na.isObject(1);\n"),
    ("c5_umd_free.js", "var a = require('./m_umd_free');\na.isObject(1);\n"),
    (
        "c6_wrapper.js",
        "function load(id) { var r = require(id); return r; }\nvar a = load('./m_plain');\na.isObject(1);\n",
    ),
    (
        "test_fp.js",
        ";(function() {\n  var root = this;\n  var _ = root._ || require('./m_umd_free');\n  function check() { return _.isObject(1); }\n  check();\n}.call(this));\n",
    ),
];

#[test]
fn property_assigned_and_umd_exports_link_through_guarded_loads() {
    let temp = fixture(B122);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "m_plain.js#isObject"),
        [
            edge("c1_plain.js", "namespace", "high"),
            edge("c2_guard_plain.js", "namespace", "medium"),
        ]
    );
    assert_eq!(
        caller_edges(root, "m_propassign.js#isObject"),
        [edge("c3_propassign.js", "namespace", "high")]
    );
    assert_eq!(
        caller_edges(root, "m_umd_return.js#runInContext.isObject"),
        [edge("c4_umd_return.js", "namespace", "high")]
    );
    // The wrapper exports on its CommonJS branch only.
    assert_eq!(
        caller_edges(root, "m_umd_free.js#runInContext.isObject"),
        [
            edge("c5_umd_free.js", "namespace", "medium"),
            edge("test_fp.js#check", "namespace", "medium"),
        ]
    );
}
