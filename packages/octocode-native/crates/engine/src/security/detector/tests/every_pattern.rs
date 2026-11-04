//! Every pattern catches an example of what it names.

use super::sampling::{Lcg, sample};
use super::*;

/// Sampled texts tried per pattern before calling it dead.
const TRIES: usize = 400;

/// A file `context` admits: its own regex sampled, under a directory.
fn path_for(context: Option<&str>, rng: &mut Lcg) -> String {
    let Some(context) = context else {
        return "conf/app.conf".to_owned();
    };
    let hir = regex_syntax::parse(context).expect(context);
    let mut name = String::new();
    sample(&hir, rng, &mut name);
    format!("conf/{name}")
}

/// Every pattern fires somewhere: a text sampled from its own syntax, alone on
/// its line in a file its context admits, is redacted. An edge a real line
/// cannot satisfy (a `\b` beside a dash at a line start) or a file context
/// nothing can name leaves a pattern that never runs, and this names it.
#[test]
fn every_pattern_redacts_a_sample_of_itself_on_its_own_line() {
    let mut rng = Lcg(0x5eed);
    let mut dead = Vec::new();
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        let hir = regex_syntax::parse(pattern.regex).expect(pattern.name);
        let alive = (0..TRIES).any(|_| {
            let path = path_for(pattern.file_context, &mut rng);
            let mut text = String::from("\n");
            sample(&hir, &mut rng, &mut text);
            text.push('\n');
            let scope = Scope::File(&path);
            should_apply(idx, scope, &text)
                && pattern_regex(idx).is_match(&text)
                && redact(&text, scope).is_some()
        });
        if !alive {
            dead.push(pattern.name);
        }
    }
    assert_eq!(dead, Vec::<&str>::new());
}

/// The value the patterns behind a file context must hide, in the file and the
/// spelling that file really uses. A pattern's own sample cannot show a context
/// or a key shape that misses the real thing.
#[test]
fn file_context_patterns_hide_values_in_their_own_files() {
    let value = "Fake0Value9Not4Real";
    let cases: &[(&str, &str, String)] = &[
        (
            "dotnetConnectionStrings",
            "web.config",
            format!(r#"<add name="db" connectionString="Server=x;Password={value};" />"#),
        ),
        (
            "dotnetConnectionStrings",
            "site/Web.config",
            format!(r#"<add name="db" connectionString="Server=x;Pwd={value};" />"#),
        ),
        (
            "dotnetConnectionStrings",
            "site/Web.Release.config",
            format!(r#"<add name="db" connectionString="Server=x;Password={value};" />"#),
        ),
        (
            "dotnetConnectionStrings",
            "App.config",
            format!(r#"<add name="db" connectionString="Server=x;Password={value};" />"#),
        ),
        (
            "dotnetConnectionStrings",
            "appsettings.json",
            format!(r#""ConnectionStrings": "Server=x;Password={value};""#),
        ),
        (
            "dotnetConnectionStrings",
            "appsettings.json",
            format!("\"ConnectionStrings\": {{\n  \"Default\": \"Server=x;Password={value};\"\n}}"),
        ),
        (
            "dotnetConnectionStrings",
            "src/appsettings.Production.json",
            format!(r#""Main": "Server=x;User Id=u;Pwd={value}""#),
        ),
        (
            "dockerComposeSecrets",
            "docker-compose.yml",
            format!(r#"POSTGRES_PASSWORD: "{value}""#),
        ),
        (
            "dockerComposeSecrets",
            "docker-compose.yaml",
            format!("      - POSTGRES_PASSWORD={value}"),
        ),
        (
            "dockerComposeSecrets",
            "docker-compose.yml",
            format!("      MYSQL_ROOT_PASSWORD: {value}"),
        ),
        (
            "springBootSecrets",
            "application.properties",
            format!("spring.datasource.password={value}"),
        ),
        (
            "springBootSecrets",
            "bootstrap-dev.yml",
            format!(r#"spring.datasource.password: "{value}""#),
        ),
        (
            "springBootSecrets",
            "src/main/resources/application-prod.properties",
            format!("spring.security.oauth2.client.registration.x.client-secret = {value}"),
        ),
        (
            "kubernetesSecrets",
            "deploy/secret.yaml",
            format!("kind: Secret\ndata:\n  tls.crt: {value}\n"),
        ),
        (
            "jdbcConnectionStringWithCredentials",
            "conf/settings.xml",
            format!("jdbc:postgresql://user:{value}@db.example.test"),
        ),
    ];
    let mut leaks = Vec::new();
    for (pattern, path, text) in cases {
        let found = detect(text, Some(path));
        if found.sanitized.contains(value) {
            leaks.push(format!("{pattern} in {path}: {text}"));
        }
    }
    assert_eq!(leaks, Vec::<String>::new());
}

/// DH parameters are public: redacting them costs evidence and hides nothing.
#[test]
fn public_dh_parameters_stay_readable() {
    let text = format!(
        "-----BEGIN DH PARAMETERS-----\nMIIBCAKCAQEA{}\n-----END DH PARAMETERS-----\n",
        "Ab3".repeat(20)
    );
    assert_eq!(detect(&text, Some("conf/dhparam.conf")).sanitized, text);
}
