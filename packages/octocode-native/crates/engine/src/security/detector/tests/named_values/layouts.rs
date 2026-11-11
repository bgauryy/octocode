//! The layouts a credential's name holds its value in, file kind by file
//! kind: each loses its values and nothing else, and its readable twin stays.

use super::*;

/// Build and package files: a Dockerfile's `ARG` and `ENV`, a token inside a
/// `RUN`, an npmrc's registry lines, a netrc's words.
#[test]
fn build_and_registry_files_lose_only_the_value() {
    let arg = format!("ARG NPM_TOKEN={TOKEN}\n");
    let env_eq = format!("ENV API_SECRET={TOKEN}\n");
    let env_blank = format!("ENV DB_PASSWORD {TOKEN}\n");
    let run = format!("RUN echo \"//registry.npmjs.org/:_authToken={TOKEN}\" > ~/.npmrc\n");
    let header = format!(
        "RUN --mount=type=secret,id=k curl -H \"PRIVATE-TOKEN: {TOKEN}\" https://gitlab.example.com\n"
    );
    let npm_token = format!("//registry.npmjs.org/:_authToken={TOKEN}\n");
    let npm_auth = format!("_auth={ENCODED}\n");
    let npm_password = format!("//npm.pkg.github.com/:_password={PADDED}\n");
    let netrc = format!("machine api.example.com login svc password {PASSWORD}\n");
    let netrc_next = format!("machine b.example.com\n  login svc\n  password {TOKEN}\n");
    hides_only(&[
        (&arg, "app.Dockerfile", &[TOKEN]),
        (&env_eq, "app.Dockerfile", &[TOKEN]),
        (&env_blank, "app.Dockerfile", &[TOKEN]),
        (&run, "app.Dockerfile", &[TOKEN]),
        (&header, "app.Dockerfile", &[TOKEN]),
        (&npm_token, "npmrc.txt", &[TOKEN]),
        (&npm_auth, "npmrc.txt", &[ENCODED]),
        (&npm_password, "npmrc.txt", &[PADDED]),
        (&netrc, "netrc.txt", &[PASSWORD]),
        (&netrc_next, "netrc.txt", &[TOKEN]),
    ]);
}

/// YAML: block scalars under a credential key, `name`/`value` pairs, an
/// anchor, a tag, a quoted dotted key, flags in `args` and `command`, and a
/// block list under a plural name.
#[test]
fn yaml_layouts_lose_only_the_value() {
    let literal = format!("token: |\n  {TOKEN}\nnext: 1\n");
    let folded = format!("secret: >\n  {TOKEN}\n  {OTHER}\nnext: 1\n");
    let strip = format!("password: |-\n  {PASSWORD}\nnext: 1\n");
    let pair = format!("env:\n  - name: API_KEY\n    value: {TOKEN}\n");
    let pair_quoted = format!("env:\n  - name: DATABASE_PASSWORD\n    value: \"{PASSWORD}\"\n");
    let anchor = format!("anchors:\n  secret_key: &sk {TOKEN}\n  copy: *sk\n");
    let tagged = format!("auth: !!str {TOKEN}\n");
    let dotted = format!("\"private.key\": {ENCODED}\n");
    let args = format!("args: [\"--token\", \"{TOKEN}\", \"--password={PASSWORD}\"]\n");
    let command = format!("command: app --api-key {TOKEN} --db-pass={OTHER}\n");
    let list = format!("passwords:\n  - {PASSWORD}\n  - {TOKEN}\nnext: 1\n");
    hides_only(&[
        (&literal, "lay.yaml", &[TOKEN]),
        (&folded, "lay.yaml", &[TOKEN, OTHER]),
        (&strip, "lay.yaml", &[PASSWORD]),
        (&pair, "lay.yaml", &[TOKEN]),
        (&pair_quoted, "lay.yaml", &[PASSWORD]),
        (&anchor, "lay.yaml", &[TOKEN]),
        (&tagged, "lay.yaml", &[TOKEN]),
        (&dotted, "lay.yaml", &[ENCODED]),
        (&args, "lay.yaml", &[TOKEN, PASSWORD]),
        (&command, "lay.yaml", &[TOKEN, OTHER]),
        (&list, "lay.yaml", &[PASSWORD, TOKEN]),
    ]);
    stays(&[
        (
            "description: |\n  The token is read from the keychain.\n  See docs/auth.md.\n",
            "lay.yaml",
        ),
        (
            "env:\n  - name: API_KEY\n    valueFrom:\n      secretKeyRef:\n        name: app\n        key: api-key\n",
            "lay.yaml",
        ),
        (
            "env:\n  - name: LOG_LEVEL\n    value: debug\n  - name: TOKEN_FILE\n    value: /run/secrets/token\n",
            "lay.yaml",
        ),
        ("keys:\n  - ctrl+shift+p\n  - cmd+k\n", "lay.yaml"),
        (
            "signing_key: |\n  -----BEGIN PGP PUBLIC KEY BLOCK-----\n  mQINBGRkZm9vYmFyYmF6cXV4Zm9vYmFyYmF6cXV4Zm9vYmFyYmF6cXV4Zm9v\n  -----END PGP PUBLIC KEY BLOCK-----\n",
            "lay.yaml",
        ),
        (
            "ssh_key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGZvb2JhcmJhenF1eGZvb2JhcmJhenF1eA deploy@host\n",
            "lay.yaml",
        ),
        (
            "args: [\"--token-file\", \"/run/secrets/token\", \"--verbose\"]\n",
            "lay.yaml",
        ),
    ]);
}

/// TOML, JSON and Terraform: a `"""` value, arrays, inline tables, a member
/// named `value` under a credential's object, an `Authorization` scheme, a
/// cookie, and a variable's `default`.
#[test]
fn data_file_layouts_lose_only_the_value() {
    let multiline = format!("[api]\ntoken = \"\"\"\n{TOKEN}\"\"\"\n");
    let array = format!("keys = [\"{TOKEN}\", \"{OTHER}\"]\n");
    let inline = format!("creds = {{ user = \"svc\", pass = \"{PASSWORD}\" }}\n");
    let json_array = format!("{{\n  \"tokens\": [\n    \"{TOKEN}\",\n    \"{OTHER}\"\n  ]\n}}\n");
    let json_objects = format!(
        "{{\n  \"credentials\": [\n    {{\n      \"user\": \"svc\",\n      \"pass\": \"{PASSWORD}\"\n    }}\n  ]\n}}\n"
    );
    let wrapper = format!("{{\n  \"secret\": {{\n    \"value\": \"{TOKEN}\"\n  }}\n}}\n");
    let argv = format!("{{\n  \"args\": [\n    \"--password\",\n    \"{PASSWORD}\"\n  ]\n}}\n");
    let bearer =
        format!("{{\n  \"headers\": {{\n    \"Authorization\": \"Bearer {TOKEN}\"\n  }}\n}}\n");
    let cookie = format!("{{\n  \"cookie\": \"session={HEX}\"\n}}\n");
    let session = format!("{{\n  \"sessionId\": \"{HEX}\"\n}}\n");
    let default = format!("variable \"db_password\" {{\n  default = \"{PASSWORD}\"\n}}\n");
    let heredoc = format!("resource \"x\" \"y\" {{\n  token = <<-EOT\n    {TOKEN}\n  EOT\n}}\n");
    hides_only(&[
        (&multiline, "lay.toml", &[TOKEN]),
        (&array, "lay.toml", &[TOKEN, OTHER]),
        (&inline, "lay.toml", &[PASSWORD]),
        (&json_array, "lay.json", &[TOKEN, OTHER]),
        (&json_objects, "lay.json", &[PASSWORD]),
        (&wrapper, "lay.json", &[TOKEN]),
        (&argv, "lay.json", &[PASSWORD]),
        (&bearer, "lay.json", &[TOKEN]),
        (&cookie, "lay.json", &[HEX]),
        (&session, "lay.json", &[HEX]),
        (&default, "lay.tf", &[PASSWORD]),
        (&heredoc, "lay.tf", &[TOKEN]),
    ]);
    stays(&[
        (
            "keys = [\"ctrl+shift+p\", \"alt+enter\", \"F12\"]\n",
            "lay.toml",
        ),
        (
            "{\n  \"keywords\": [\n    \"a8F3kL9pQ2zX7vB4\"\n  ]\n}\n",
            "lay.json",
        ),
        (
            "{\n  \"tokens\": [\"identifier\", \"number\", \"string\", \"punctuation\"]\n}\n",
            "lay.json",
        ),
        (
            "{\"keys\":[{\"kty\":\"EC\",\"crv\":\"P-256\",\"x\":\"Public0Coordinate9Stays8Readable7Always6Ab\",\"y\":\"Public0Coordinate9Stays8Readable7Always6Ab\"}]}\n",
            "jwks.json",
        ),
        (
            "{\n  \"signatures\": [\n    {\n      \"id\": \"CVE-2023-25193-cb610f4e\",\n      \"file\": \"src/hb-ot-layout-gsubgpos.hh\"\n    }\n  ]\n}\n",
            "osv.json",
        ),
        (
            "variable \"region\" {\n  default = \"eu-west-1\"\n}\nvariable \"db_password\" {\n  type = string\n}\n",
            "lay.tf",
        ),
    ]);
}

/// Source code: Python, Rust, Go, C#, PHP, Ruby and TypeScript forms the
/// assignment rules did not read.
#[test]
fn source_layouts_lose_only_the_value() {
    let fstring = format!("hdr = {{\"Authorization\": f\"Bearer {{'{TOKEN}'}}\"}}\n");
    let triple = format!("token = \"\"\"\n{TOKEN}\n\"\"\"\n");
    let tuple = format!("TOKENS = (\"{TOKEN}\", \"{OTHER}\")\n");
    let auth_tuple = format!("requests.get(url, auth=(\"svc\", \"{PASSWORD}\"))\n");
    let dict = format!("my_secrets = {{\"prod\": \"{TOKEN}\"}}\n");
    let env_macro = format!("const KEY: &str = env!(\"API_KEY\", \"{TOKEN}\");\n");
    let option_env = format!("    let token = option_env!(\"TOKEN\").unwrap_or(\"{TOKEN}\");\n");
    let format_macro = "    let secret = format!(\"{}{}\", \"lz24OfsUehXx\", \"NiytEBdUaQnw\");\n";
    let concat_macro = "const TOKEN: &str = concat!(\"7Pgq1eCRuOJo\", \"BwoI3cWKnYG7\");\n";
    let vec_macro = format!("    let passwords = vec![\"{PASSWORD}\", \"{TOKEN}\"];\n");
    let set_var = format!("    std::env::set_var(\"AUTH_TOKEN\", \"{TOKEN}\");\n");
    let continued = format!("    let private_key = \"\\\n        {TOKEN}\";\n");
    let setenv = format!("func f() {{ os.Setenv(\"AUTH_TOKEN\", \"{TOKEN}\") }}\n");
    let cs_const = format!("  const string ApiKey = \"{TOKEN}\";\n");
    let cs_interp = format!("  string token = $\"Bearer {{\"{TOKEN}\"}}\";\n");
    let cs_credential = format!("  var cred = new NetworkCredential(\"svc\", \"{PASSWORD}\");\n");
    let php_index = format!("$config['db']['pass'] = \"{PASSWORD}\";\n");
    let php_define = format!("define('API_SECRET', '{TOKEN}');\n");
    let ruby_heredoc = format!("token = <<~EOS\n  {TOKEN}\nEOS\n");
    let ruby_percent = format!("ENV['SECRET'] ||= %q({TOKEN})\n");
    let java_property = format!("props.setProperty(\"password\", \"{PASSWORD}\");\n");
    let java_default = format!("  @Value(\"${{jwt.secret:{TOKEN}}}\") String jwt;\n");
    let ts_array = format!("const tokens = [\"{TOKEN}\", \"{OTHER}\"];\n");
    let ts_join = "const key = [\"XOnzf53Oa9K3\", \"efcVsufAttOa\"].join(\"\");\n";
    let ts_map = format!(
        "const apiKeys: Record<string, string> = {{ prod: \"{TOKEN}\", stage: \"{OTHER}\" }};\n"
    );
    let ts_nested = format!(
        "const client = new Client({{ auth: {{ user: \"svc\", pass: \"{PASSWORD}\" }} }});\n"
    );
    let ts_compare = format!("if (pass === \"{PASSWORD}\") grant();\n");
    let ts_call = format!("client.setToken(\"admin\", \"{PASSWORD}\");\n");
    let ts_raw = format!("const cfg = {{ password: String.raw`{PASSWORD}` }};\n");
    let ts_bearer = format!("const auth = `Bearer {TOKEN}`;\n");
    let ts_buffer = format!("const enc = {{ key: Buffer.from(\"{HEX}\", \"hex\") }};\n");
    let ts_comment = format!("// TODO remove: old password was {PASSWORD}\n");
    hides_only(&[
        (&fstring, "lay.py", &[TOKEN]),
        (&triple, "lay.py", &[TOKEN]),
        (&tuple, "lay.py", &[TOKEN, OTHER]),
        (&auth_tuple, "lay.py", &[PASSWORD]),
        (&dict, "lay.py", &[TOKEN]),
        (&env_macro, "lay.rs", &[TOKEN]),
        (&option_env, "lay.rs", &[TOKEN]),
        (format_macro, "lay.rs", &["lz24OfsUehXx", "NiytEBdUaQnw"]),
        (concat_macro, "lay.rs", &["7Pgq1eCRuOJo", "BwoI3cWKnYG7"]),
        (&vec_macro, "lay.rs", &[PASSWORD, TOKEN]),
        (&set_var, "lay.rs", &[TOKEN]),
        (&continued, "lay.rs", &[TOKEN]),
        (&setenv, "lay.go", &[TOKEN]),
        (&cs_const, "lay.cs", &[TOKEN]),
        (&cs_interp, "lay.cs", &[TOKEN]),
        (&cs_credential, "lay.cs", &[PASSWORD]),
        (&php_index, "lay.php", &[PASSWORD]),
        (&php_define, "lay.php", &[TOKEN]),
        (&ruby_heredoc, "lay.rb", &[TOKEN]),
        (&ruby_percent, "lay.rb", &[TOKEN]),
        (&java_property, "Lay.java", &[PASSWORD]),
        (&java_default, "Lay.java", &[TOKEN]),
        (&ts_array, "lay.ts", &[TOKEN, OTHER]),
        (ts_join, "lay.ts", &["XOnzf53Oa9K3", "efcVsufAttOa"]),
        (&ts_map, "lay.ts", &[TOKEN, OTHER]),
        (&ts_nested, "lay.ts", &[PASSWORD]),
        (&ts_compare, "lay.ts", &[PASSWORD]),
        (&ts_call, "lay.ts", &[PASSWORD]),
        (&ts_raw, "lay.ts", &[PASSWORD]),
        (&ts_bearer, "lay.ts", &[TOKEN]),
        (&ts_buffer, "lay.ts", &[HEX]),
        (&ts_comment, "lay.ts", &[PASSWORD]),
    ]);
    stays(&[
        (
            "const keys = [\"ArrowUp\", \"ArrowDown\", \"PageUp\", \"PageDown\"];\n",
            "lay.ts",
        ),
        (
            "const tokens = lexer.tokenize(source, { keepComments: true });\n",
            "lay.ts",
        ),
        (
            "export const authRoutes = { login: \"/auth/login\", logout: \"/auth/logout\" };\n",
            "lay.ts",
        ),
        (
            "if (token === expectedTokenForTheCurrentSession) grant();\n",
            "lay.ts",
        ),
        (
            "fn auth_token(&self, scope: &str) -> Option<String> {\n    self.tokens.get(scope).cloned()\n}\n",
            "lay.rs",
        ),
        (
            "let key = format!(\"{}:{}\", \"repository\", \"commitSha1\");\n",
            "lay.rs",
        ),
        ("login(\"admin\", passwordFromThePromptDialog);\n", "lay.ts"),
        (
            "password = getpass.getpass(\"Password for the registry: \")\n",
            "lay.py",
        ),
        (
            "token = \"\"\"\nThe token is read from the keychain at start.\n\"\"\"\n",
            "lay.py",
        ),
    ]);
}

/// Shell and command lines: a variable set for one command, flags, a
/// parameter's default, a command substitution, a heredoc whose tag names
/// what it holds, and headers.
#[test]
fn shell_layouts_lose_only_the_value() {
    let inline_env = "PGPASSWORD=gHBlIzrlXDtpB5EB psql -h db app\n";
    let flag = format!("deploy --token {TOKEN} --region eu\n");
    let flag_eq = format!("deploy --password={PASSWORD} --region eu\n");
    let default = format!(": \"${{JWT_SECRET:={TOKEN}}}\"\n");
    let substitution = format!("TOKEN=$(echo {ENCODED} | base64 -d)\n");
    let heredoc = format!("cat <<'CREDS'\n{TOKEN}\nCREDS\necho done\n");
    let header = format!("curl -H 'X-Api-Key: {TOKEN}' https://api.example.com/v1\n");
    let scheme = format!("curl -H \"Authorization: token {TOKEN}\" https://api.example.com\n");
    let configure = format!("aws configure set aws_secret_access_key {ENCODED}\n");
    let trailing = format!("ACCESS_TOKEN={ENCODED} is the key\n");
    hides_only(&[
        (inline_env, "lay.sh", &["gHBlIzrlXDtpB5EB"]),
        (&flag, "lay.sh", &[TOKEN]),
        (&flag_eq, "lay.sh", &[PASSWORD]),
        (&default, "lay.sh", &[TOKEN]),
        (&substitution, "lay.sh", &[ENCODED]),
        (&heredoc, "lay.sh", &[TOKEN]),
        (&header, "lay.sh", &[TOKEN]),
        (&scheme, "lay.md", &[TOKEN]),
        (&configure, "lay.sh", &[ENCODED]),
        (&trailing, "notes/prose.txt", &[ENCODED]),
    ]);
    stays(&[
        (
            "deploy --token \"$DEPLOY_TOKEN\" --region eu-west-1\n",
            "lay.sh",
        ),
        ("TOKEN=$(vault kv get -field=token secret/app)\n", "lay.sh"),
        (
            "cat <<'NOTES'\nThe token is rotated every ninety days.\nNOTES\n",
            "lay.sh",
        ),
        ("export PATH=/usr/local/bin:/usr/bin:/bin\n", "lay.sh"),
    ]);
}

/// A token inside a Markdown sentence, a table cell or an inline code span
/// goes; the sentence, the table and the backticks stay.
#[test]
fn a_token_in_markdown_prose_is_hidden() {
    let sentence = format!("The API key is {TOKEN} and the secret: {OTHER}.\n");
    let code = format!("Run with the admin password `{PASSWORD}` on first boot.\n");
    let table = format!("| api_key | {TOKEN} |\n");
    hides_only(&[
        (&sentence, "lay.md", &[TOKEN, OTHER]),
        (&code, "lay.md", &[PASSWORD]),
        (&table, "lay.md", &[TOKEN]),
    ]);
    stays(&[
        (
            "The API key is read from `OCTOCODE_API_KEY` and never logged.\n",
            "lay.md",
        ),
        ("| api_key | The key the provider issued |\n", "lay.md"),
        (
            "Set the password with `octocode auth login` on first boot.\n",
            "lay.md",
        ),
    ]);
}
