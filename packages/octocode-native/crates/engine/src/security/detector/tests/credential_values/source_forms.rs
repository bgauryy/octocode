//! More forms in which source code carries a credential (B165, B190): typed
//! declarations with a lifetime or a Go type, chained fallbacks, `<-`
//! assignments, literals split over lines in parentheses, comment markers of
//! grammarless languages and a word of a comment that is the secret. Every
//! value is fake; each leak has a twin that must stay readable.

use super::config_entries::{hides, keeps};
use super::*;

fn hidden_in_files(cases: &[(&str, &str)], secrets: &[&str]) {
    for (path, content) in cases {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            hides(content, scope, secrets);
        }
    }
}

fn kept_in_files(cases: &[(&str, &str)]) {
    for (path, content) in cases {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            keeps(content, scope);
        }
    }
}

#[test]
fn a_typed_declaration_with_a_lifetime_or_a_go_type_is_redacted() {
    hidden_in_files(
        &[
            (
                "a.rs",
                "static API_PASSWORD: &'static str = \"Fake0Moo9Zed\";\n",
            ),
            (
                "a.rs",
                "pub const PASSWORD: &'static str = \"Fake0Moo9Zed\";\n",
            ),
            ("a.rs", "const PASSWORD: &'a [u8] = b\"Fake0Moo9Zed\";\n"),
            ("a.go", "var password string = \"Fake0Moo9Zed\"\n"),
            ("a.go", "const apiSecret string = \"Fake0Moo9Zed\"\n"),
        ],
        &["Fake0Moo9Zed"],
    );
    kept_in_files(&[
        (
            "a.rs",
            "static PASSWORD_FILE: &'static str = \"/run/secrets/db\";\n",
        ),
        ("a.go", "var password string\n"),
        ("a.go", "var passwordFile string = \"/run/secrets/db\"\n"),
    ]);
}

#[test]
fn a_chain_of_fallbacks_ends_in_the_credential() {
    hidden_in_files(
        &[
            (
                "a.ts",
                "const password = process.env.PASSWORD || process.env.PW || \"Fake0Moo9Zed\";\n",
            ),
            ("a.ts", "password = a || b || \"Fake0Moo9Zed\";\n"),
            ("a.ts", "const passwd = a ?? b ?? `Fake0Moo9Zed`;\n"),
            ("a.py", "password = a or b or \"Fake0Moo9Zed\"\n"),
        ],
        &["Fake0Moo9Zed"],
    );
    kept_in_files(&[
        ("a.ts", "const host = a || b || \"localhost:8080\";\n"),
        ("a.ts", "const password = a || b || \"\";\n"),
    ]);
}

#[test]
fn an_arrow_assignment_is_redacted() {
    hidden_in_files(
        &[
            ("a.r", "password <- \"Fake0Moo9Zed\"\n"),
            ("a.r", "secret <- 'Fake0Moo9Zed'\n"),
        ],
        &["Fake0Moo9Zed"],
    );
    kept_in_files(&[("a.r", "total <- \"some long label\"\n")]);
}

#[test]
fn a_literal_split_over_lines_in_parentheses_is_redacted() {
    hidden_in_files(
        &[
            ("a.py", "password = (\"Fake\"\n  \"0Moo9Zed\")\n"),
            ("a.py", "password = (\n    \"Fake\"\n    \"0Moo9Zed\"\n)\n"),
            ("a.py", "password = (\n    'Fake'\n    + '0Moo9Zed'\n)\n"),
        ],
        &["Fake0Moo9Zed", "0Moo9Zed"],
    );
    kept_in_files(&[("a.py", "name = (\n    \"Fake\"\n    \"0Moo9Zed\"\n)\n")]);
}

#[test]
fn comments_of_grammarless_languages_carry_credentials() {
    hidden_in_files(
        &[
            ("a.lua", "-- password: Fake0Moo9Zed\n"),
            ("a.sql", "-- password: Fake0Moo9Zed\n"),
            ("a.sql", "/* password: Fake0Moo9Zed */\n"),
            ("a.ini", "; password = Fake0Moo9Zed\n"),
            ("a.xml", "<!-- password: Fake0Moo9Zed -->\n"),
        ],
        &["Fake0Moo9Zed"],
    );
    kept_in_files(&[
        ("a.sql", "-- password policy applies to every role\n"),
        ("a.sql", "-- ------------------------------\n"),
    ]);
}

#[test]
fn a_secret_shaped_word_opens_a_comment_sentence_it_hides() {
    hidden_in_files(
        &[
            ("a.ts", "// password: Fake0Moo9Zed and more\n"),
            (
                "a.ts",
                "let x = 1; // password: Fake0Moo9Zed and more words\n",
            ),
            ("a.py", "x = 1  # password: Fake0Moo9Zed and more\n"),
        ],
        &["Fake0Moo9Zed"],
    );
    kept_in_files(&[
        ("a.ts", "// password: must be at least 8 characters\n"),
        ("a.ts", "// password: is required\n"),
        ("a.py", "x = 1  # token: use the sha256 of the file\n"),
    ]);
}

/// A snippet written inside a code literal, its quotes escaped, is judged by
/// what the escaped quotes frame (B210): a credential in it is redacted and a
/// format placeholder is not, in a read and in storage. (A second string cut at
/// the escaped quote, `\"jwt_secret = \\\"\"`, is still redacted in storage: B210.)
#[test]
fn a_snippet_with_escaped_quotes_in_a_code_literal_is_judged_by_its_value() {
    hidden_in_files(
        &[
            (
                "a.rs",
                "let s = \"jwt_secret = \\\"q8Zr4LmT2vXw9KpN\\\";\";\n",
            ),
            (
                "a.ts",
                "const s = \"jwt_secret = \\\"q8Zr4LmT2vXw9KpN\\\";\";\n",
            ),
            (
                "a.rs",
                "(format!(\"jwt_secret = \\\"q8Zr4LmT2vXw9KpN\\\";\"), \"jwt_secret = \\\"\"),\n",
            ),
            ("app.properties", "jwt_secret = \"q8Zr4LmT2vXw9KpN\";\n"),
        ],
        &["q8Zr4LmT2vXw9KpN"],
    );
    kept_in_files(&[("a.rs", "let s = \"jwt_secret = \\\"{value}\\\";\";\n")]);
}

/// The statement that creates or alters a database user or role carries its
/// password in a clause (B190): the statement's own shape marks it, so the
/// same words elsewhere stay readable.
#[test]
fn a_create_or_alter_user_statement_hides_its_password_clause() {
    hidden_in_files(
        &[
            ("a.sql", "CREATE USER app IDENTIFIED BY 'Fake0Moo9Zed';\n"),
            ("a.sql", "create user app with password 'Fake0Moo9Zed';\n"),
            ("a.sql", "ALTER ROLE app PASSWORD 'Fake0Moo9Zed';\n"),
            (
                "a.sql",
                "ALTER USER 'app'@'%' IDENTIFIED BY 'Fake0Moo9Zed';\n",
            ),
            (
                "a.sql",
                "CREATE ROLE app\n  LOGIN\n  PASSWORD 'Fake0Moo9Zed';\n",
            ),
            (
                "a.sql",
                "CREATE USER app IDENTIFIED WITH mysql_native_password BY 'Fake0Moo9Zed';\n",
            ),
            (
                "a.py",
                "cur.execute(\"CREATE USER app IDENTIFIED BY 'Fake0Moo9Zed'\")\n",
            ),
        ],
        &["Fake0Moo9Zed"],
    );
    kept_in_files(&[
        ("a.sql", "ALTER ROLE app PASSWORD NULL;\n"),
        ("a.sql", "CREATE USER app IDENTIFIED BY :pw;\n"),
        ("a.sql", "CREATE USER app IDENTIFIED BY '<password>';\n"),
        ("a.sql", "CREATE USER app IDENTIFIED BY '${DB_PASS}';\n"),
        ("a.sql", "CREATE USER app IDENTIFIED BY '{% db_pass %}';\n"),
        (
            "a.sql",
            "SELECT password \"Enter your password here\" FROM users;\n",
        ),
        ("a.rb", "password \"Enter your password here, please\"\n"),
        ("a.sql", "CREATE TABLE users (name text, password text);\n"),
    ]);
}

/// An element named for a credential holds one as its text (B190), where the
/// credential word ends the element's name.
#[test]
fn a_credential_element_hides_its_text() {
    hidden_in_files(
        &[
            ("a.xml", "<password>Fake0Moo9Zed</password>\n"),
            (
                "settings.xml",
                "<server>\n  <id>nexus</id>\n  <username>deploy</username>\n  <password>Fake0Moo9Zed</password>\n</server>\n",
            ),
            ("pom.xml", "<dbPassword>Fake0Moo9Zed</dbPassword>\n"),
            ("a.xml", "<client-secret>Fake0Moo9Zed</client-secret>\n"),
            ("a.xml", "<apiToken kind=\"x\">Fake0Moo9Zed</apiToken>\n"),
            ("a.html", "<div><Password>Fake0Moo9Zed</Password></div>\n"),
            ("a.xml", "<password><![CDATA[Fake0Moo9Zed]]></password>\n"),
        ],
        &["Fake0Moo9Zed"],
    );
    kept_in_files(&[
        ("a.xml", "<password></password>\n"),
        ("a.xml", "<password/>\n"),
        ("a.xml", "<password>${env.NEXUS_PASS}</password>\n"),
        ("a.xml", "<password>{{ vault_pass }}</password>\n"),
        ("a.xml", "<password>true</password>\n"),
        ("a.xml", "<passwordMinLength>12</passwordMinLength>\n"),
        ("a.xml", "<tokenType>Bearer</tokenType>\n"),
        ("a.xml", "<secretName>my-tls-secret</secretName>\n"),
        ("Info.plist", "<key>CFBundleName</key>\n"),
    ]);
}

/// A backslash escape inside the password literal is the literal's own
/// character, so the whole value goes (B287).
#[test]
fn a_backslash_escape_in_a_user_password_literal_hides_the_whole_value() {
    hidden_in_files(
        &[
            (
                "a.sql",
                "CREATE USER app IDENTIFIED BY 'Fa\\'ke0Moo9Zed';\n",
            ),
            (
                "a.sql",
                "CREATE USER app IDENTIFIED BY \"Fa\\\"ke0Moo9Zed\";\n",
            ),
            ("a.sql", "ALTER ROLE app PASSWORD 'Fake0\\\\Moo\\'9Zed';\n"),
        ],
        &["ke0Moo9Zed", "Moo9Zed", "9Zed"],
    );
    kept_in_files(&[("a.sql", "CREATE USER app IDENTIFIED BY '';\n")]);
}

/// The statement forms of other databases (B287): Oracle's unquoted
/// password, SQL Server's login, MySQL's `PASSWORD('…')` and Postgres'
/// dollar quoting.
#[test]
fn the_user_statements_of_other_databases_hide_their_password() {
    hidden_in_files(
        &[
            ("a.sql", "CREATE USER app IDENTIFIED BY Fake0Moo9Zed;\n"),
            (
                "a.sql",
                "create user app identified by Fake0Moo9Zed default tablespace users;\n",
            ),
            (
                "a.sql",
                "CREATE USER app IDENTIFIED BY VALUES 'Fake0Moo9Zed';\n",
            ),
            (
                "a.sql",
                "CREATE LOGIN app WITH PASSWORD = 'Fake0Moo9Zed' MUST_CHANGE, CHECK_EXPIRATION = ON;\n",
            ),
            (
                "a.sql",
                "ALTER LOGIN app WITH PASSWORD = N'Fake0Moo9Zed' OLD_PASSWORD = 'Fake0Moo8Zed';\n",
            ),
            (
                "a.sql",
                "CREATE USER app IDENTIFIED BY PASSWORD('Fake0Moo9Zed');\n",
            ),
            (
                "a.sql",
                "CREATE USER 'app'@'%' IDENTIFIED BY PASSWORD '*2470C0C06DEE42FD1618BB99005ADCA2EC9D1E19';\n",
            ),
            (
                "a.sql",
                "CREATE ROLE app LOGIN PASSWORD $$Fake0Moo9Zed$$;\n",
            ),
            (
                "a.sql",
                "CREATE ROLE app LOGIN PASSWORD $pw$Fake0Moo9Zed$pw$;\n",
            ),
        ],
        &[
            "Fake0Moo9Zed",
            "Fake0Moo8Zed",
            "2470C0C06DEE42FD1618BB99005ADCA2EC9D1E19",
        ],
    );
    kept_in_files(&[
        ("a.sql", "CREATE USER app IDENTIFIED BY :pw;\n"),
        ("a.sql", "CREATE USER app IDENTIFIED BY ?;\n"),
        ("a.sql", "CREATE USER app IDENTIFIED BY &pw;\n"),
        ("a.sql", "CREATE USER app IDENTIFIED BY ${DB_PASS};\n"),
        ("a.sql", "CREATE USER app IDENTIFIED EXTERNALLY;\n"),
        ("a.sql", "CREATE LOGIN app FROM WINDOWS;\n"),
        ("a.sql", "CREATE LOGIN app WITH PASSWORD = @pw;\n"),
        ("a.sql", "CREATE USER app IDENTIFIED BY PASSWORD(@pw);\n"),
        ("a.sql", "CREATE ROLE app LOGIN PASSWORD NULL;\n"),
        ("a.sql", "CREATE ROLE app LOGIN PASSWORD $1;\n"),
    ]);
}

/// A key/value attribute pair whose key names a credential holds one in its
/// value (B287), as `<add key="password" value="…"/>` of a `.NET` config does.
#[test]
fn a_key_value_attribute_pair_hides_the_value_of_a_credential_key() {
    hidden_in_files(
        &[
            (
                "app.config",
                "<add key=\"password\" value=\"Fake0Moo9Zed\"/>\n",
            ),
            (
                "app.config",
                "<add name=\"DbPassword\" value=\"Fake0Moo9Zed\" />\n",
            ),
            (
                "web.config",
                "<add key=\"ApiToken\" description=\"x\" value=\"Fake0Moo9Zed\"/>\n",
            ),
            (
                "web.config",
                "<add value=\"Fake0Moo9Zed\" key=\"client-secret\"/>\n",
            ),
            ("a.xml", "<add key='password' value='Fake0Moo9Zed'/>\n"),
            ("a.xml", "<entry key=\"password\">Fake0Moo9Zed</entry>\n"),
            (
                "a.xml",
                "<appSettings>\n  <add key=\"password\"\n       value=\"Fake0Moo9Zed\"/>\n</appSettings>\n",
            ),
        ],
        &["Fake0Moo9Zed"],
    );
    kept_in_files(&[
        ("app.config", "<add key=\"password\" value=\"\"/>\n"),
        (
            "app.config",
            "<add key=\"password\" value=\"${DB_PASS}\"/>\n",
        ),
        (
            "app.config",
            "<add name=\"passwordMinLength\" value=\"12\"/>\n",
        ),
        ("app.config", "<add key=\"tokenType\" value=\"Bearer\"/>\n"),
        ("app.config", "<add key=\"Timeout\" value=\"30\"/>\n"),
        ("app.config", "<add key=\"password\"/>\n"),
        ("a.html", "<input name=\"username\" value=\"admin\"/>\n"),
        ("a.xml", "<property name=\"tokens\" value=\"EQUAL\"/>\n"),
        ("a.xml", "<entry key=\"password\"/>\n text </p>\n"),
    ]);
}

/// An element's text on the lines after its opening tag is its value too
/// (B287), and JSX or generics that look like such an element stay readable.
#[test]
fn a_credential_element_hides_text_that_starts_on_the_next_line() {
    hidden_in_files(
        &[
            ("a.xml", "<password>\n  Fake0Moo9Zed\n</password>\n"),
            (
                "a.xml",
                "<password>\n  <![CDATA[Fake0Moo9Zed]]>\n</password>\n",
            ),
            (
                "a.xml",
                "<password><![CDATA[\n  Fake0Moo9Zed\n]]></password>\n",
            ),
        ],
        &["Fake0Moo9Zed"],
    );
    kept_in_files(&[
        ("a.tsx", "const a = <Token>{v}</Token>;\n"),
        ("a.tsx", "const a = <Token>{props.value}</Token>;\n"),
        (
            "a.tsx",
            "const a = (\n  <Token>\n    {props.value}\n  </Token>\n);\n",
        ),
        (
            "a.jsx",
            "const a = <Credential>Loading your credentials...</Credential>;\n",
        ),
        ("a.html", "<password hint=\"x\"/>\n text</p>\n"),
        ("a.java", "/** Lists the List<Token> held here </p> */\n"),
    ]);
}
