#[test]
fn path_dependencies_come_from_uv_poetry_and_file_requirements() {
    let pyproject = r#"
[project]
dependencies = ["a @ file:///abs/a ; python_version > '3'", "requests>=2", "b @ https://x.invalid/b.whl"]
[project.optional-dependencies]
dev = ["c @ file:../c"]
[dependency-groups]
test = ["d @ file:./d"]
[tool.uv.sources]
e = { path = "../e", editable = true }
f = [{ path = "../f-unix", marker = "sys_platform != 'win32'" }, { index = "pypi" }]
g = { workspace = true }
[tool.poetry.dependencies]
h = { path = "../h", develop = true }
python = "^3.12"
[tool.poetry.group.dev.dependencies]
i = { path = "vendor/i" }
"#
    .parse::<toml::Table>()
    .expect("toml");
    let mut paths = super::path_dependencies(&pyproject);
    paths.sort();
    assert_eq!(
        paths,
        [
            "../c",
            "../e",
            "../f-unix",
            "../h",
            "./d",
            "/abs/a",
            "vendor/i"
        ]
    );
}
