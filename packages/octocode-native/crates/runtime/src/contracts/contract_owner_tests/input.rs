use super::*;

#[test]
fn row_scoped_output_violations_degrade_to_row_errors() {
    let output = json!({"results":[
        {"index":0,"data":{"error":"upstream failed"}},
        {"index":1,"data":{"bogusKey":true}}
    ]});
    let error = validate_output("localSearch", &output).expect_err("row 1 violates");
    let patched = isolate_row_violations("localSearch", &output, &error).expect("isolated");
    assert_eq!(patched["results"][0]["data"]["error"], "upstream failed");
    assert!(patched["results"][0].get("status").is_none());
    assert_eq!(patched["results"][1]["status"], "error");
    assert_eq!(
        patched["results"][1]["data"]["errorCode"],
        "outputContractViolation"
    );
    assert!(validate_output("localSearch", &patched).is_ok());
    // Envelope-level violations stay whole-call failures.
    let envelope = json!({"results":"not-an-array"});
    let error = validate_output("localSearch", &envelope).expect_err("envelope violates");
    assert!(isolate_row_violations("localSearch", &envelope, &error).is_none());
}
