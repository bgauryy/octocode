use serde_json::{Map, Value, json};
use std::{
    env,
    fs::File,
    io::{self, Read},
    process, thread,
    time::{Duration, Instant, SystemTime},
};

const LIMIT: u64 = 4 * 1024 * 1024;
const HELP: &str = "octocode-jev 0.1.0 — TypeSafe Jev typed decisions

Usage:
  octocode-jev evaluate [--input FILE|-] [--dry-run] [--model MODEL]
  octocode-jev models
  octocode-jev --help | --version

Options:
  --base-url URL      API root (default https://api.typesafe.ai; appends /v1/...)
  --timeout-ms N      Total HTTP/retry budget, 100..300000 (default 30000)
  --retries N         Extra attempts for HTTP 429/503/529, 0..10 (native default 2)
  --pretty           Pretty-print JSON

Input: JSON {state, questions, model?}; stdin by default. Max 4 MiB.
Model: --model > input.model > OCTOCODE_JEV_MODEL > jev-latest.
Auth: OCTOCODE_JEV_KEY; no secret flags or credential logging.
The Node launcher loads <HOME>/.octocode/.env automatically (or OCTOCODE_HOME).
It also adds .octocoderc, --project-env and shared Octocode network settings.
--dry-run validates and prints the resolved payload without network or key.
Success JSON goes to stdout; JSON errors go to stderr. No action execution.
Exit: 0 success; 2 input/config; 3 HTTP/transport; 4 invalid response; 5 output.

Examples:
  octocode-jev evaluate --input request.json --dry-run
  octocode-jev evaluate --input request.json
  octocode-jev models
";

struct Failure {
    code: i32,
    message: String,
}
type Result<T> = std::result::Result<T, Failure>;
fn fail(code: i32, message: impl Into<String>) -> Failure {
    Failure {
        code,
        message: message.into(),
    }
}
fn ensure(ok: bool, code: i32, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(code, message)) }
}
fn variable(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|x| x.trim().to_owned())
        .filter(|x| !x.is_empty())
}
struct Options {
    command: String,
    input: String,
    model: Option<String>,
    base: String,
    timeout: Duration,
    retries: u32,
    dry: bool,
    pretty: bool,
}
fn options(args: Vec<String>) -> Result<Options> {
    let mut args = args.into_iter();
    let command = args.next().unwrap_or_default();
    ensure(
        matches!(command.as_str(), "evaluate" | "models"),
        2,
        "Expected evaluate or models; run --help.",
    )?;
    let mut o = Options {
        command,
        input: "-".into(),
        model: None,
        base: variable("OCTOCODE_JEV_BASE_URL").unwrap_or_else(|| "https://api.typesafe.ai".into()),
        timeout: Duration::from_secs(30),
        retries: 2,
        dry: false,
        pretty: false,
    };
    let mut seen = std::collections::HashSet::new();
    while let Some(arg) = args.next() {
        ensure(seen.insert(arg.clone()), 2, "Duplicate option; run --help.")?;
        match arg.as_str() {
            "--dry-run" => o.dry = true,
            "--pretty" => o.pretty = true,
            "--input" | "--model" | "--base-url" | "--timeout-ms" | "--retries" => {
                let value = args
                    .next()
                    .ok_or_else(|| fail(2, "Option requires a value; run --help."))?;
                match arg.as_str() {
                    "--input" => o.input = value,
                    "--model" => o.model = Some(value),
                    "--base-url" => o.base = value,
                    "--timeout-ms" => {
                        let n = value
                            .parse::<u64>()
                            .map_err(|_| fail(2, "--timeout-ms must be 100..300000."))?;
                        ensure(
                            (100..=300000).contains(&n),
                            2,
                            "--timeout-ms must be 100..300000.",
                        )?;
                        o.timeout = Duration::from_millis(n);
                    }
                    _ => {
                        o.retries = value
                            .parse()
                            .map_err(|_| fail(2, "--retries must be 0..10."))?;
                        ensure(o.retries <= 10, 2, "--retries must be 0..10.")?;
                    }
                }
            }
            _ => return Err(fail(2, "Unknown option; run --help.")),
        }
    }
    if o.command == "models" {
        ensure(
            !seen.contains("--input") && !seen.contains("--model") && !o.dry,
            2,
            "models does not accept --input, --model or --dry-run.",
        )?;
    }
    o.base = o.base.trim_end_matches('/').to_owned();
    let uri: ureq::http::Uri = o
        .base
        .parse()
        .map_err(|_| fail(2, "Invalid --base-url API root."))?;
    let host = uri.host().unwrap_or("");
    let local = matches!(host, "127.0.0.1" | "localhost" | "[::1]");
    ensure(
        uri.scheme_str() == Some("https") || (uri.scheme_str() == Some("http") && local),
        2,
        "API root must use HTTPS (HTTP is allowed only on loopback for tests).",
    )?;
    ensure(
        !host.is_empty()
            && !o.base.contains('@')
            && !o.base.contains('#')
            && uri.query().is_none()
            && matches!(uri.path(), "" | "/"),
        2,
        "API root must contain only scheme, host and optional port; no credentials, path, query or fragment.",
    )?;
    Ok(o)
}
fn entry(value: &Value) -> bool {
    matches!(
        value,
        Value::String(_) | Value::Object(_) | Value::Array(_) | Value::Null
    )
}
fn object<'a>(value: &'a Value, code: i32, message: &str) -> Result<&'a Map<String, Value>> {
    value.as_object().ok_or_else(|| fail(code, message))
}
fn payload(o: &Options) -> Result<Value> {
    let reader: Box<dyn Read> = if o.input == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(File::open(&o.input).map_err(|_| {
            fail(
                2,
                "Cannot read --input file; check its path and permissions.",
            )
        })?)
    };
    let mut bytes = Vec::new();
    reader
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| fail(2, "Cannot read request JSON."))?;
    ensure(
        bytes.len() as u64 <= LIMIT,
        2,
        "Input exceeds the client limit of 4 MiB; reduce state/questions.",
    )?;
    let mut body: Value = serde_json::from_slice(&bytes).map_err(|e| {
        fail(
            2,
            format!(
                "Invalid JSON at line {}, column {}; check input syntax.",
                e.line(),
                e.column()
            ),
        )
    })?;
    let map = body
        .as_object_mut()
        .ok_or_else(|| fail(2, "Request must be a JSON object."))?;
    ensure(
        map.keys()
            .all(|x| matches!(x.as_str(), "state" | "model" | "questions")),
        2,
        "Request only supports state, model, questions. Jev does not accept chat, image, action or streaming fields.",
    )?;
    ensure(
        map.get("state").is_some_and(entry),
        2,
        "state must be text, an object, an array or null.",
    )?;
    if let Some(value) = map.get("model") {
        ensure(
            value.as_str().is_some_and(|s| !s.trim().is_empty()),
            2,
            "model must be a nonempty string.",
        )?;
    }
    let model = o
        .model
        .clone()
        .or_else(|| map.get("model").and_then(Value::as_str).map(str::to_owned))
        .or_else(|| variable("OCTOCODE_JEV_MODEL"))
        .unwrap_or_else(|| "jev-latest".into());
    ensure(
        !model.trim().is_empty(),
        2,
        "model must be a nonempty string.",
    )?;
    map.insert("model".into(), Value::String(model));
    let questions = object(
        &body["questions"],
        2,
        "questions must be a nonempty object.",
    )?;
    ensure(
        !questions.is_empty(),
        2,
        "questions must be a nonempty object.",
    )?;
    for question in questions.values() {
        let q = object(question, 2, "Each question must be an object.")?;
        ensure(
            q.keys()
                .all(|x| matches!(x.as_str(), "type" | "instructions" | "criteria")),
            2,
            "Unknown question field; use type, instructions, criteria.",
        )?;
        ensure(
            q.get("instructions").is_none_or(entry),
            2,
            "instructions must be text, an object, an array or null.",
        )?;
        match question["type"].as_str() {
            Some("noul") => {
                if let Some(criteria) = q.get("criteria").filter(|x| !x.is_null()) {
                    let c = object(criteria, 2, "noul criteria must be an object or null.")?;
                    ensure(
                        c.iter()
                            .all(|(k, v)| matches!(k.as_str(), "true" | "false") && entry(v)),
                        2,
                        "noul criteria allows only true/false descriptions.",
                    )?;
                }
            }
            Some("choice") => {
                let c = object(
                    &question["criteria"],
                    2,
                    "choice criteria must map labels to descriptions.",
                )?;
                ensure(
                    (1..=255).contains(&c.len()) && c.values().all(entry),
                    2,
                    "choice needs 1..255 labels with text/object/array/null descriptions.",
                )?;
            }
            Some("score") => {
                let c = question["criteria"]
                    .as_array()
                    .ok_or_else(|| fail(2, "score criteria must be an ordered array."))?;
                ensure(
                    c.len() >= 2 && c.iter().all(entry),
                    2,
                    "score needs at least two text/object/array/null levels.",
                )?;
            }
            _ => return Err(fail(2, "Question type must be choice, score or noul.")),
        }
    }
    Ok(body)
}
fn number(v: &Value) -> Result<f64> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| fail(4, "Response contains a missing or invalid number."))
}
fn probability(v: &Value) -> Result<f64> {
    let n = number(v)?;
    ensure(
        (0.0..=1.0).contains(&n),
        4,
        "Response probability/confidence must be between 0 and 1.",
    )?;
    Ok(n)
}
fn distribution(answer: &Value, labels: &[String]) -> Result<Vec<f64>> {
    let p = object(
        &answer["probabilities"],
        4,
        "Missing probability distribution.",
    )?;
    ensure(
        p.len() == labels.len() && labels.iter().all(|k| p.contains_key(k)),
        4,
        "Response probability keys differ from requested criteria.",
    )?;
    let values = labels
        .iter()
        .map(|k| probability(&p[k]))
        .collect::<Result<Vec<_>>>()?;
    ensure(
        (values.iter().sum::<f64>() - 1.0).abs() <= 0.02,
        4,
        "Response probabilities do not sum to 1 (tolerance 0.02).",
    )?;
    probability(&answer["confidence"])?;
    Ok(values)
}
fn validate(body: &Value, request: Option<&Value>) -> Result<()> {
    if let Some(request) = request {
        ensure(
            body["model"].as_str().is_some_and(|x| !x.is_empty()),
            4,
            "Response is missing model.",
        )?;
        let usage = object(&body["usage"], 4, "Response is missing usage.")?;
        ensure(
            ["input_tokens", "output_tokens"]
                .iter()
                .all(|k| usage.get(*k).is_some_and(|x| x.as_u64().is_some())),
            4,
            "Response token counts must be nonnegative integers.",
        )?;
        let answers = object(&body["answers"], 4, "Response is missing answers.")?;
        let questions = request["questions"]
            .as_object()
            .expect("validated questions");
        ensure(
            answers.len() == questions.len() && questions.keys().all(|k| answers.contains_key(k)),
            4,
            "Response answer IDs differ from requested question IDs.",
        )?;
        for (id, q) in questions {
            let a = &answers[id];
            ensure(
                a["type"] == q["type"],
                4,
                "Answer type differs from question type.",
            )?;
            match q["type"].as_str().expect("validated type") {
                "noul" => {
                    probability(&a["noul"])?;
                }
                "choice" => {
                    let labels: Vec<_> = q["criteria"]
                        .as_object()
                        .expect("validated criteria")
                        .keys()
                        .cloned()
                        .collect();
                    let values = distribution(a, &labels)?;
                    let choice = a["choice"]
                        .as_str()
                        .ok_or_else(|| fail(4, "Missing choice label."))?;
                    let index = labels
                        .iter()
                        .position(|x| x == choice)
                        .ok_or_else(|| fail(4, "Response chose an unknown label."))?;
                    ensure(
                        values.iter().all(|p| *p <= values[index] + 1e-6),
                        4,
                        "Chosen label is not a highest-probability option.",
                    )?;
                }
                "score" => {
                    let levels = q["criteria"].as_array().expect("validated criteria");
                    let labels: Vec<_> = (0..levels.len()).map(|i| i.to_string()).collect();
                    let values = distribution(a, &labels)?;
                    let legend = object(&a["legend"], 4, "Response is missing score legend.")?;
                    ensure(
                        legend.len() == levels.len()
                            && levels
                                .iter()
                                .enumerate()
                                .all(|(i, v)| legend.get(&i.to_string()) == Some(v)),
                        4,
                        "Response legend differs from requested score levels.",
                    )?;
                    let score = number(&a["score"])?;
                    let expected = values
                        .iter()
                        .enumerate()
                        .map(|(i, p)| i as f64 * p)
                        .sum::<f64>();
                    let max = (levels.len() - 1) as f64;
                    ensure(
                        (0.0..=max).contains(&score)
                            && (score - expected).abs() <= 0.02 * max.max(1.0),
                        4,
                        "Score differs from its probability-weighted rubric (2% of range tolerance).",
                    )?;
                }
                _ => unreachable!(),
            }
        }
    } else {
        let models = body["models"]
            .as_array()
            .ok_or_else(|| fail(4, "Expected a models array."))?;
        ensure(
            models.iter().all(|m| {
                ["name", "description", "release_date"]
                    .iter()
                    .all(|k| m[*k].is_string())
            }),
            4,
            "Malformed model metadata.",
        )?;
    }
    Ok(())
}
fn retry_delay(headers: &ureq::http::HeaderMap, attempt: u32) -> Duration {
    if let Some(ms) = headers
        .get("retry-after-ms")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
    {
        return Duration::from_millis(ms);
    }
    if let Some(s) = headers.get("retry-after").and_then(|v| v.to_str().ok()) {
        if let Ok(secs) = s.parse::<u64>() {
            return Duration::from_secs(secs);
        }
        if let Ok(date) = httpdate::parse_http_date(s) {
            return date.duration_since(SystemTime::now()).unwrap_or_default();
        }
    }
    Duration::from_millis(500 * (1u64 << attempt))
}
fn request(o: &Options, payload: Option<&Value>) -> Result<Value> {
    let key = variable("OCTOCODE_JEV_KEY").ok_or_else(|| fail(2, "Set OCTOCODE_JEV_KEY in the process, or use scripts/jev.mjs for .octocoderc/.env configuration."))?;
    ensure(
        !key.chars().any(char::is_control),
        2,
        "OCTOCODE_JEV_KEY contains invalid control characters.",
    )?;
    let url = format!(
        "{}/v1/{}",
        o.base,
        if payload.is_some() {
            "systemone"
        } else {
            "models"
        }
    );
    let started = Instant::now();
    let encoded = payload.map(Value::to_string);
    for attempt in 0..=o.retries {
        let remaining = o
            .timeout
            .checked_sub(started.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| fail(3, "Total HTTP/retry timeout exceeded."))?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_global(Some(remaining))
            .build()
            .into();
        let auth = format!("Bearer {key}");
        let response = if let Some(body) = &encoded {
            agent
                .post(&url)
                .header("Authorization", &auth)
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .send(body)
        } else {
            agent
                .get(&url)
                .header("Authorization", &auth)
                .header("Accept", "application/json")
                .call()
        };
        let mut response = response.map_err(|_| fail(3, "HTTPS request failed or timed out; check connectivity, certificates, proxy and API root. No automatic transport retry."))?;
        let status = response.status().as_u16();
        if matches!(status, 429 | 503 | 529) && attempt < o.retries {
            let delay = retry_delay(response.headers(), attempt);
            let remaining = o.timeout.saturating_sub(started.elapsed());
            if delay >= remaining {
                return Err(fail(
                    3,
                    format!("HTTP {status}: retry delay exceeds remaining timeout; retry later."),
                ));
            }
            drop(response);
            thread::sleep(delay);
            continue;
        }
        if !(200..300).contains(&status) {
            let hint = match status {
                401 | 403 => "check OCTOCODE_JEV_KEY and account access",
                422 | 400 => "check request fields, criteria, context size and model",
                429 | 503 | 529 => "retry later or reduce request volume",
                300..400 => "redirects are disabled; check the configured API root",
                _ => "check provider availability and API root",
            };
            return Err(fail(
                3,
                format!(
                    "HTTP {status}: {hint}. Response body omitted to avoid echoing private data."
                ),
            ));
        }
        let text = response
            .body_mut()
            .with_config()
            .limit(LIMIT)
            .read_to_string()
            .map_err(|_| {
                fail(
                    4,
                    "Could not read response within the 4 MiB limit and timeout.",
                )
            })?;
        let body: Value =
            serde_json::from_str(&text).map_err(|_| fail(4, "Provider returned invalid JSON."))?;
        validate(&body, payload)?;
        return Ok(body);
    }
    Err(fail(3, "Retry limit exhausted."))
}
fn run() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] || args.is_empty() {
        print!("{HELP}");
        return Ok(());
    }
    if args == ["--version"] {
        println!("octocode-jev {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let o = options(args)?;
    let body = if o.command == "evaluate" {
        Some(payload(&o)?)
    } else {
        None
    };
    let output = if o.dry {
        body.expect("evaluate dry run")
    } else {
        request(&o, body.as_ref())?
    };
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    if o.pretty {
        serde_json::to_writer_pretty(&mut handle, &output)
    } else {
        serde_json::to_writer(&mut handle, &output)
    }
    .map_err(|_| fail(5, "Cannot write JSON to stdout."))?;
    use io::Write;
    writeln!(handle).map_err(|_| fail(5, "Cannot write JSON to stdout."))?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!(
            "{}",
            json!({"error": {"code": error.code, "message": error.message}})
        );
        process::exit(error.code);
    }
}
