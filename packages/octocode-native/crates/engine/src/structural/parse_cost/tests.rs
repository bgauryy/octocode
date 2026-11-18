use super::*;

fn minified(calls: usize) -> Vec<u8> {
    format!(
        "function f(x){{return x}}var a=[{}eval(zzz)];\n",
        "f(1),".repeat(calls)
    )
    .into_bytes()
}

#[test]
fn a_dense_minified_file_costs_more_than_a_long_plain_file_of_its_size() {
    let dense = minified(20_000);
    let plain = "let value = compute_something(argument);\n"
        .repeat(dense.len() / 40)
        .into_bytes();
    assert!(parse_cost(&dense) > 2 * parse_cost(&plain));
}

#[test]
fn a_pathological_single_line_is_over_a_small_budget_and_a_normal_file_is_not() {
    let budget = 64 * 1024 * 1024;
    // 9.4 MB of minified calls, as measured at about 2 GB.
    assert!(parse_cost(&minified(1_886_000)) > budget * 8);
    let normal = "fn main() { run(); }\n".repeat(2_000).into_bytes();
    assert!(parse_cost(&normal) < budget);
}
