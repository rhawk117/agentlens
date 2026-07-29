#![allow(
    unreachable_pub,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use support::{Case, case, check_snapshot, run};

const CASES: &[Case] = &[
    case(
        "slice_method",
        &["slice", "src/api/users.py#UserService.create_user"],
    ),
    case(
        "slice_method_no_decorators",
        &[
            "slice",
            "src/api/users.py#UserService.create_user",
            "--no-decorators",
        ],
    ),
    case(
        "slice_method_signature_only",
        &[
            "slice",
            "src/api/users.py#UserService.create_user",
            "--signature-only",
        ],
    ),
    case("slice_overloads", &["slice", "src/api/users.py#normalise"]),
    case(
        "slice_nested_class",
        &["slice", "src/api/users.py#UserService.Cursor.next"],
    ),
    case(
        "slice_conditional_def",
        &["slice", "src/api/users.py#only_when_typing"],
    ),
    case("slice_constant", &["slice", "src/api/users.py#MAX_RETRIES"]),
    case("slice_line_span", &["slice", "src/api/users.py#L20-L24"]),
    case("slice_outline", &["slice", "src/api/users.py#"]),
    case(
        "slice_missing",
        &["slice", "src/api/users.py#UserService.nope"],
    ),
    case("slice_no_hash", &["slice", "src/api/users.py"]),
    case("slice_missing_file", &["slice", "src/api/ghost.py#Thing"]),
    case(
        "slice_json",
        &[
            "slice",
            "src/api/users.py#UserService.delete_user",
            "--json",
        ],
    ),
    case(
        "slice_budget_degrades",
        &[
            "slice",
            "src/api/users.py#UserService.create_user",
            "--budget",
            "20",
        ],
    ),
    case(
        "slice_budget_counts",
        &["slice", "src/api/users.py#normalise", "--budget", "4"],
    ),
    case("map_file", &["map", "src/api/users.py"]),
    case(
        "map_file_depth1",
        &["map", "src/api/users.py", "--depth", "1"],
    ),
    case(
        "map_file_classes",
        &["map", "src/api/users.py", "--kind", "class"],
    ),
    case("map_file_quiet", &["map", "src/api/users.py", "--quiet"]),
    case("map_file_json", &["map", "src/core/config.py", "--json"]),
];

#[test]
fn snapshots_match() {
    for item in CASES {
        check_snapshot(item.name, &run(item.args));
    }
}

#[test]
fn output_is_byte_identical_across_runs() {
    for item in CASES {
        let first = run(item.args);
        let second = run(item.args);
        assert_eq!(first, second, "non-deterministic output for {}", item.name);
    }
}

#[test]
fn exit_codes_branch_without_parsing() {
    assert!(run(&["slice", "src/api/users.py#UserService.create_user"]).contains("exit: 0"));
    assert!(run(&["slice", "src/api/users.py#UserService.nope"]).contains("exit: 1"));
    assert!(run(&["slice", "src/api/users.py"]).contains("exit: 2"));
}

#[test]
fn slice_excludes_the_leading_comment() {
    let output = run(&["slice", "src/api/users.py#UserService.create_user"]);
    assert!(!output.contains("must never appear"));
    assert!(output.contains("@transaction.atomic"));
}

#[test]
fn every_overload_is_returned_in_source_order() {
    let output = run(&["slice", "src/api/users.py#normalise"]);
    assert!(output.contains("(1 of 4)"));
    assert!(output.contains("(4 of 4)"));
    let first = output.find("(1 of 4)").expect("first");
    let last = output.find("(4 of 4)").expect("last");
    assert!(first < last);
}
