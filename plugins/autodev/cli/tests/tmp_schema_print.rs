#[test]
fn print_schema() {
    println!(
        "ANALYSIS:{}",
        autodev::infrastructure::claude::output::ANALYSIS_SCHEMA.as_str()
    );
    println!(
        "REVIEW:{}",
        autodev::infrastructure::claude::output::REVIEW_SCHEMA.as_str()
    );
}
