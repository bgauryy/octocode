//! A file indexed from decoded text (Latin-1, lossy UTF-8) is in the graph
//! and states how it was read, with the sentence localSearch uses.
use super::*;
use octocode_engine::text::{LATIN1_WARNING, LOSSY_WARNING};

#[test]
fn a_decoded_file_is_indexed_and_states_how_it_was_read() {
    let dir = git_app(&[("clean.ts", "export function clean() { return 1; }\n")]);
    let app = dir.0.join("app");
    std::fs::write(
        app.join("latin.ts"),
        b"export function caf\xe9() { return 1; }\n",
    )
    .expect("latin");
    std::fs::write(
        app.join("stray.ts"),
        b"// \xc3\xa9 \xff\nexport function gamma() { return 2; }\n",
    )
    .expect("stray");
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    assert_eq!(built.value["scan"]["filesSkipped"], 0, "{}", built.value);
    assert_eq!(built.value["diagnostics"]["decoded"], 2, "{}", built.value);
    let out = ask(dir.0.as_path(), "diagnostics", None, |options| {
        options.limit = Some(100);
    });
    let rows = out.value["rows"].as_array().cloned().unwrap_or_default();
    let said = |file: &str| {
        rows.iter()
            .find(|row| row["path"] == format!("app/{file}"))
            .map(|row| (row["code"].clone(), row["message"].clone()))
            .unwrap_or_else(|| panic!("no diagnostic names {file}: {}", out.value))
    };
    assert_eq!(said("latin.ts"), ("decoded".into(), LATIN1_WARNING.into()));
    assert_eq!(said("stray.ts"), ("decoded".into(), LOSSY_WARNING.into()));
    assert!(rows.iter().all(|row| row["path"] != "app/clean.ts"));
}
