//! The qualitative rating of a CVSS base score, for ranking an advisory whose
//! database states no severity word. The score is the one its source
//! publishes with the vector, else computed here from a v3.x or v2 vector by
//! the specification's base equations, and serves the ranking only: a row
//! states each vector and score as its source published them, never this one.
//! A v4 vector alone, or a vector missing a base metric, rates nothing.
use serde_json::Value;

/// The rating of an advisory's CVSS entries (`{type, vector, score?}`): its
/// v3.x entries first, then v2, by the first one that rates.
pub(super) fn rating(cvss: &[Value]) -> Option<&'static str> {
    let entries = || {
        cvss.iter().filter_map(|entry| {
            let vector = entry["vector"].as_str()?;
            Some((vector, entry["score"].as_f64()))
        })
    };
    entries()
        .filter(|(vector, _)| is_v3(vector))
        .find_map(|(vector, score)| Some(v3_rating(score.or_else(|| v3_base(vector))?)))
        .or_else(|| {
            entries()
                .filter(|(vector, _)| is_v2(vector))
                .find_map(|(vector, score)| Some(v2_rating(score.or_else(|| v2_base(vector))?)))
        })
}

fn is_v3(vector: &str) -> bool {
    vector.starts_with("CVSS:3.0/") || vector.starts_with("CVSS:3.1/")
}

/// A v2 vector carries no version prefix (`AV:N/AC:L/Au:N/C:P/I:P/A:P`),
/// sometimes in parentheses or after `CVSS:2.0/`.
fn is_v2(vector: &str) -> bool {
    let vector = v2_metrics(vector);
    !vector.starts_with("CVSS:") && vector.split('/').any(|part| part.starts_with("Au:"))
}

fn v2_metrics(vector: &str) -> &str {
    let vector = vector.trim().trim_start_matches('(').trim_end_matches(')');
    vector.strip_prefix("CVSS:2.0/").unwrap_or(vector)
}

/// The value of base metric `name` in `vector`; `None` when it is absent.
fn metric<'a>(vector: &'a str, name: &str) -> Option<&'a str> {
    vector
        .split('/')
        .filter_map(|part| part.split_once(':'))
        .find_map(|(key, value)| (key == name).then_some(value))
}

/// CVSS v3.1 §5: None 0.0, Low 0.1–3.9, Medium 4.0–6.9, High 7.0–8.9,
/// Critical 9.0–10.0 (v3.0 states the same scale).
fn v3_rating(score: f64) -> &'static str {
    match score {
        score if score >= 9.0 => "critical",
        score if score >= 7.0 => "high",
        score if score >= 4.0 => "medium",
        score if score > 0.0 => "low",
        _ => "none",
    }
}

/// The v2 rating NVD publishes: Low 0.0–3.9, Medium 4.0–6.9, High 7.0–10.0.
fn v2_rating(score: f64) -> &'static str {
    match score {
        score if score >= 7.0 => "high",
        score if score >= 4.0 => "medium",
        _ => "low",
    }
}

/// CVSS v3.1 Appendix A `Roundup`: the smallest number of one decimal place
/// at or above `value`, read at five decimals so floating-point noise does
/// not lift an exact tenth.
fn roundup(value: f64) -> f64 {
    ((value * 100_000.0).round() / 10_000.0).ceil() / 10.0
}

/// The v3.x base score of `vector` (CVSS v3.1 §7.1; v3.0 uses the same
/// equations and weights).
fn v3_base(vector: &str) -> Option<f64> {
    let changed = match metric(vector, "S")? {
        "U" => false,
        "C" => true,
        _ => return None,
    };
    let attack_vector: f64 = match metric(vector, "AV")? {
        "N" => 0.85,
        "A" => 0.62,
        "L" => 0.55,
        "P" => 0.2,
        _ => return None,
    };
    let complexity: f64 = match metric(vector, "AC")? {
        "L" => 0.77,
        "H" => 0.44,
        _ => return None,
    };
    let privileges: f64 = match (metric(vector, "PR")?, changed) {
        ("N", _) => 0.85,
        ("L", false) => 0.62,
        ("L", true) => 0.68,
        ("H", false) => 0.27,
        ("H", true) => 0.5,
        _ => return None,
    };
    let interaction: f64 = match metric(vector, "UI")? {
        "N" => 0.85,
        "R" => 0.62,
        _ => return None,
    };
    let impact_of = |name: &str| -> Option<f64> {
        match metric(vector, name)? {
            "H" => Some(0.56),
            "L" => Some(0.22),
            "N" => Some(0.0),
            _ => None,
        }
    };
    let (confidentiality, integrity, availability) =
        (impact_of("C")?, impact_of("I")?, impact_of("A")?);
    let iss = 1.0 - (1.0 - confidentiality) * (1.0 - integrity) * (1.0 - availability);
    let impact = if changed {
        7.52 * (iss - 0.029) - 3.25 * (iss - 0.02).powi(15)
    } else {
        6.42 * iss
    };
    let exploitability = 8.22 * attack_vector * complexity * privileges * interaction;
    Some(match (impact <= 0.0, changed) {
        (true, _) => 0.0,
        (false, false) => roundup((impact + exploitability).min(10.0)),
        (false, true) => roundup((1.08 * (impact + exploitability)).min(10.0)),
    })
}

/// The v2 base score of `vector` (CVSS v2 guide §3.2.1), rounded to one
/// decimal.
fn v2_base(vector: &str) -> Option<f64> {
    let vector = v2_metrics(vector);
    let access_vector: f64 = match metric(vector, "AV")? {
        "L" => 0.395,
        "A" => 0.646,
        "N" => 1.0,
        _ => return None,
    };
    let complexity: f64 = match metric(vector, "AC")? {
        "H" => 0.35,
        "M" => 0.61,
        "L" => 0.71,
        _ => return None,
    };
    let authentication: f64 = match metric(vector, "Au")? {
        "M" => 0.45,
        "S" => 0.56,
        "N" => 0.704,
        _ => return None,
    };
    let impact_of = |name: &str| -> Option<f64> {
        match metric(vector, name)? {
            "N" => Some(0.0),
            "P" => Some(0.275),
            "C" => Some(0.660),
            _ => None,
        }
    };
    let (confidentiality, integrity, availability) =
        (impact_of("C")?, impact_of("I")?, impact_of("A")?);
    let impact = 10.41 * (1.0 - (1.0 - confidentiality) * (1.0 - integrity) * (1.0 - availability));
    let exploitability = 20.0 * access_vector * complexity * authentication;
    let scale = if impact > 0.0 { 1.176 } else { 0.0 };
    Some((((0.6 * impact) + (0.4 * exploitability) - 1.5) * scale * 10.0).round() / 10.0)
}

#[cfg(test)]
#[path = "cvss_tests.rs"]
mod tests;
