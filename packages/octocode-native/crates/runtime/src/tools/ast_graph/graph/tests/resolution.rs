//! Import resolution: Python packages, JS/TS extensions and aliases, C
//! include directories.
use super::*;
use crate::tools::test_support::Fixture;

#[test]
fn python_parent_imports_climb_the_declared_number_of_packages() {
    let known = ["pkg/shared.py", "pkg/sub/shared.py", "shared.py"]
        .iter()
        .map(|path| (*path).to_owned())
        .collect();
    assert_eq!(
        super::super::resolve_python(
            "..shared",
            "pkg/sub/worker.py",
            Some("python-relative"),
            "run",
            &known,
            &["."],
        ),
        Some("pkg/shared.py".to_owned())
    );
    assert_eq!(
        super::super::resolve_python(
            "....shared",
            "pkg/sub/worker.py",
            Some("python-relative"),
            "run",
            &known,
            &["."],
        ),
        None,
        "relative imports cannot climb above the scan root"
    );
}

#[test]
fn relative_javascript_import_cannot_reenter_after_escaping_scan_root() {
    let known = ["target.ts"]
        .iter()
        .map(|path| (*path).to_owned())
        .collect();
    assert_eq!(
        super::super::resolve(
            "../target",
            "entry.ts",
            super::super::Linker::JsTs,
            None,
            "*",
            &known,
            &Default::default(),
            &Default::default(),
        ),
        None,
    );
}

#[test]
fn extensionless_javascript_import_links_supported_module_extensions() {
    let known = ["src/module.mts", "src/legacy.cts"]
        .iter()
        .map(|path| (*path).to_owned())
        .collect();
    for (specifier, expected) in [
        ("./module", "src/module.mts"),
        ("./legacy", "src/legacy.cts"),
    ] {
        assert_eq!(
            super::super::resolve(
                specifier,
                "src/entry.ts",
                super::super::Linker::JsTs,
                None,
                "*",
                &known,
                &Default::default(),
                &Default::default(),
            ),
            Some(expected.to_owned()),
        );
    }
}

#[test]
fn unsupported_languages_do_not_use_javascript_package_resolution() {
    let known = ["src/index.ts"]
        .iter()
        .map(|path| (*path).to_owned())
        .collect();
    let packages =
        super::super::ResolveContext::default().with_package("example", ".", "src/index.ts");
    assert_eq!(
        super::super::resolve(
            "example",
            "main.go",
            super::super::Linker::Go,
            None,
            "*",
            &known,
            &Default::default(),
            &packages,
        ),
        None,
    );
}

#[test]
fn alias_looking_bare_specifiers_are_internal_gaps_not_external() {
    let mut b = super::super::BuiltGraph::default();
    let security = crate::security::ContentSecurity::new();
    let record = |b: &mut super::super::BuiltGraph, spec: &str, internal: bool| {
        super::super::record_resolution(
            b,
            "src/a.ts",
            1,
            spec,
            super::super::Linker::JsTs,
            &None,
            false,
            internal,
            &security,
        )
    };
    assert!(record(&mut b, "react", false), "third-party stays external");
    assert!(!record(&mut b, "@/missing", true));
    assert!(!record(&mut b, "@acme/core/package.json", true));
    assert_eq!(b.imports, [0, 1, 1, 0, 1]);
    assert!(
        b.diagnostics.iter().any(
            |d| d.code == DiagnosticCode::UnresolvedInternal && d.message.contains("@/missing")
        )
    );
}

#[test]
fn tsconfig_aliases_and_workspace_packages_link_in_a_built_graph() {
    let dir = Fixture::new();
    let root = dir.0.as_path().canonicalize().unwrap();
    dir.write(
        "tsconfig.json",
        "{ /* jsonc */ \"compilerOptions\": { \"paths\": { \"@app/*\": [\"./app/src/*\"], }, }, }",
    );
    dir.write("lib/package.json", r#"{ "name": "@acme/lib", "main": "./dist/index.js", "exports": { ".": { "import": "./dist/index.js" } } }"#);
    dir.write("lib/src/index.ts", "export const lib = 1;\n");
    dir.write("app/src/util.ts", "export const util = 1;\n");
    dir.write("app/src/main.ts", "import { util } from '@app/util';\nimport { lib } from '@acme/lib';\nimport { gone } from '@app/gone';\nimport React from 'react';\nexport const all = [util, lib, gone, React];\n");
    let built = fixture_graph(&root, &[]);
    let imports = &built.facts["app/src/main.ts"].imports;
    let target = |spec: &str| {
        imports
            .iter()
            .find(|import| import.specifier == spec)
            .map(|import| (import.target.clone(), import.external))
    };
    assert_eq!(
        target("@app/util"),
        Some((Some("app/src/util.ts".to_owned()), false))
    );
    assert_eq!(
        target("@acme/lib"),
        Some((Some("lib/src/index.ts".to_owned()), false))
    );
    assert_eq!(target("@app/gone"), Some((None, false)));
    assert_eq!(target("react"), Some((None, true)));
    assert_eq!(built.imports[..3], [2, 1, 1]);
}

#[test]
fn python_absolute_imports_use_src_layout_roots_and_submodules() {
    let known = known(&[
        "proj/src/pkg/__init__.py",
        "proj/src/pkg/sub.py",
        "proj/src/pkg/nested/__init__.py",
        "proj/app.py",
    ]);
    let roots = ["proj/src", "proj", "."];
    let resolve = |spec: &str, imported: &str| {
        super::super::resolve_python(
            spec,
            "proj/app.py",
            Some(if spec.starts_with('.') {
                "python-relative"
            } else {
                "python-absolute"
            }),
            imported,
            &known,
            &roots,
        )
    };
    assert_eq!(
        resolve("pkg", "*").as_deref(),
        Some("proj/src/pkg/__init__.py")
    );
    assert_eq!(
        resolve("pkg", "sub").as_deref(),
        Some("proj/src/pkg/sub.py")
    );
    assert_eq!(
        resolve("pkg", "nested").as_deref(),
        Some("proj/src/pkg/nested/__init__.py")
    );
    // A non-module name falls back to the package itself.
    assert_eq!(
        resolve("pkg", "function_name").as_deref(),
        Some("proj/src/pkg/__init__.py")
    );
    assert_eq!(resolve("missing", "*"), None);
}

#[test]
fn c_includes_search_project_include_dirs() {
    let dir = Fixture::new();
    let root = dir.0.as_path().canonicalize().unwrap();
    dir.write("include/proj/api.h", "int api(void);\n");
    dir.write("third/include/dep.h", "int dep(void);\n");
    dir.write("compile_commands.json", serde_json::json!([{"directory": root, "command": "cc -Ithird/include -c src/main.c", "file": "src/main.c"}]).to_string());
    dir.write("src/main.c", "#include \"proj/api.h\"\n#include <dep.h>\n#include <stdio.h>\nint main(void) { return api() + dep(); }\n");
    let built = fixture_graph(&root, &[]);
    let imports = &built.facts["src/main.c"].imports;
    let target = |spec: &str| {
        imports
            .iter()
            .find(|import| import.specifier == spec)
            .and_then(|import| import.target.clone())
    };
    assert_eq!(target("proj/api.h").as_deref(), Some("include/proj/api.h"));
    assert_eq!(target("dep.h").as_deref(), Some("third/include/dep.h"));
    assert_eq!(target("stdio.h"), None);
}
