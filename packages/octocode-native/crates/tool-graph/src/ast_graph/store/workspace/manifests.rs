//! Reading ecosystem manifests (npm, Cargo, Python, Go) into components.
use super::*;

pub(super) fn read_json(read: &dyn Fn(&Path) -> Option<String>, path: &Path) -> Option<Value> {
    serde_json::from_str(&read(path)?).ok()
}

pub(super) fn read_toml(
    read: &dyn Fn(&Path) -> Option<String>,
    path: &Path,
) -> Option<toml::Table> {
    read(path)?.parse::<toml::Table>().ok()
}

pub(super) fn join(dir: &str, file: &str) -> String {
    let file = file.trim_start_matches("./");
    if dir.is_empty() {
        file.to_owned()
    } else {
        format!("{dir}/{file}")
    }
}

/// PEP 508 requirement → import-ish name (`Django>=4` → `django`).
pub(super) fn python_requirement(spec: &str) -> Option<String> {
    let name = spec
        .trim()
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .next()?
        .to_ascii_lowercase()
        .replace('-', "_");
    (!name.is_empty() && !name.starts_with('#')).then_some(name)
}

pub(super) fn npm_component(dir: &str, manifest: &Value) -> Component {
    let mut component = Component {
        dir: dir.to_owned(),
        name: manifest["name"].as_str().unwrap_or(dir).to_owned(),
        ecosystem: "npm",
        complete: true,
        ..Default::default()
    };
    const TEMPLATE_DEPS: &[&str] = &[
        "@docusaurus/core",
        "@mdx-js/react",
        "@mdx-js/loader",
        "@next/mdx",
        "vue",
        "svelte",
        "astro",
        "vitepress",
        "nuxt",
        "@nuxt/content",
        "@storybook/react",
    ];
    component.templates = ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .any(|field| {
            manifest[*field]
                .as_object()
                .is_some_and(|deps| TEMPLATE_DEPS.iter().any(|d| deps.contains_key(*d)))
        });
    for (field, dev) in [
        ("dependencies", false),
        ("peerDependencies", false),
        ("optionalDependencies", false),
        ("devDependencies", true),
    ] {
        for name in manifest[field]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, _)| k)
        {
            let set = if dev {
                &mut component.dev_deps
            } else {
                &mut component.deps
            };
            set.insert(name.clone());
            // `@types/x` provides types for `x`.
            if let Some(typed) = name.strip_prefix("@types/") {
                let target = match typed.split_once("__") {
                    Some((scope, pkg)) => format!("@{scope}/{pkg}"),
                    None => typed.to_owned(),
                };
                component.dev_deps.insert(target);
            }
        }
    }
    component
}

pub(super) fn cargo_component(dir: &str, manifest: &toml::Table) -> Component {
    let name = manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or(dir)
        .replace('-', "_");
    let mut component = Component {
        dir: dir.to_owned(),
        name,
        ecosystem: "cargo",
        complete: manifest.contains_key("package"),
        ..Default::default()
    };
    let mut tables = vec![(manifest.clone(), false)];
    if let Some(targets) = manifest.get("target").and_then(|t| t.as_table()) {
        for target in targets.values().filter_map(|t| t.as_table()) {
            tables.push((target.clone(), false));
        }
    }
    for (table, _) in tables {
        for (field, dev) in [
            ("dependencies", false),
            ("build-dependencies", false),
            ("dev-dependencies", true),
        ] {
            // `foo = { package = "real-name" }` imports as `foo`: keys suffice.
            for key in table
                .get(field)
                .and_then(|d| d.as_table())
                .into_iter()
                .flat_map(|t| t.keys())
            {
                let set = if dev {
                    &mut component.dev_deps
                } else {
                    &mut component.deps
                };
                set.insert(key.replace('-', "_"));
            }
        }
    }
    component
}

pub(super) fn python_component(
    dir: &str,
    root: &Path,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Option<Component> {
    let base = root.join(dir);
    let mut component = Component {
        dir: dir.to_owned(),
        name: dir.rsplit('/').next().unwrap_or(dir).to_owned(),
        ecosystem: "pypi",
        ..Default::default()
    };
    let mut found = false;
    let pyproject = read_toml(read, &base.join("pyproject.toml"));
    if let Some(pyproject) = &pyproject {
        found = true;
        let project = pyproject.get("project");
        if let Some(name) = project.and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
            component.name = name.to_owned();
        }
        for spec in project
            .and_then(|p| p.get("dependencies"))
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
        {
            component.deps.extend(python_requirement(spec));
        }
        for group in project
            .and_then(|p| p.get("optional-dependencies"))
            .and_then(|d| d.as_table())
            .into_iter()
            .flat_map(|t| t.values())
            .filter_map(|v| v.as_array())
        {
            for spec in group.iter().filter_map(|v| v.as_str()) {
                component.dev_deps.extend(python_requirement(spec));
            }
        }
        let poetry = pyproject.get("tool").and_then(|t| t.get("poetry"));
        for key in poetry
            .and_then(|p| p.get("dependencies"))
            .and_then(|d| d.as_table())
            .into_iter()
            .flat_map(|t| t.keys())
        {
            component.deps.extend(python_requirement(key));
        }
    }
    for requirements in [
        "requirements.txt",
        "requirements-dev.txt",
        "dev-requirements.txt",
    ] {
        if let Some(text) = read(&base.join(requirements)) {
            found = true;
            let dev = requirements.contains("dev");
            for line in text
                .lines()
                .filter(|l| !l.trim_start().starts_with(['#', '-']))
            {
                let set = if dev {
                    &mut component.dev_deps
                } else {
                    &mut component.deps
                };
                set.extend(python_requirement(line));
            }
        }
    }
    let distributable = base.join("setup.py").is_file()
        || base.join("setup.cfg").is_file()
        || base.join("pyproject.toml").is_file();
    found |= distributable;
    let app = [
        "manage.py",
        "__main__.py",
        "main.py",
        "app.py",
        "wsgi.py",
        "asgi.py",
    ]
    .iter()
    .any(|marker| base.join(marker).is_file())
        || pyproject.as_ref().is_some_and(|p| {
            p.get("project").and_then(|x| x.get("scripts")).is_some()
                || p.get("tool")
                    .and_then(|t| t.get("poetry"))
                    .and_then(|x| x.get("scripts"))
                    .is_some()
        });
    component.library = distributable && !app;
    // Python distribution names rarely equal import names (`PyYAML` →
    // `yaml`), so a miss is never claimed as undeclared.
    component.complete = false;
    found.then_some(component)
}

pub(super) fn go_component(
    dir: &str,
    root: &Path,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Option<Component> {
    let text = read(&root.join(dir).join("go.mod"))?;
    let mut component = Component {
        dir: dir.to_owned(),
        ecosystem: "go",
        complete: true,
        ..Default::default()
    };
    let mut in_require = false;
    for line in text.lines().map(str::trim) {
        if let Some(module) = line.strip_prefix("module ") {
            component.name = module.trim().to_owned();
        } else if line.starts_with("require (") {
            in_require = true;
        } else if in_require && line == ")" {
            in_require = false;
        } else if let Some(spec) = line.strip_prefix("require ").or(in_require.then_some(line))
            && let Some(path) = spec.split_whitespace().next().filter(|p| !p.is_empty())
        {
            component.deps.insert(path.to_owned());
        }
    }
    Some(component)
}
