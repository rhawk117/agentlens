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
    case("map_dir", &["map", "."]),
    case("map_dir_depth1", &["map", ".", "--depth", "1"]),
    case("map_dir_json", &["map", ".", "--json"]),
    case("map_dir_budget", &["map", ".", "--budget", "60"]),
    case("find_default", &["find", "create_user"]),
    case(
        "find_exact_calls",
        &["find", "create_user", "--exact", "--kind", "call"],
    ),
    case(
        "find_definitions",
        &["find", "normalise", "--kind", "definition"],
    ),
    case(
        "find_context_none",
        &["find", "create_user", "--context", "none"],
    ),
    case("find_scoped_path", &["find", "user", "src/api"]),
    case(
        "find_comments",
        &["find", "never appear", "--include-comments"],
    ),
    case("find_strings", &["find", "refused", "--include-strings"]),
    case("find_miss", &["find", "zzz_no_such_symbol"]),
    case("find_json", &["find", "delete_user", "--json"]),
    case("find_budget", &["find", "user", "--budget", "40"]),
    case("literals_file", &["literals", "src/core/config.py"]),
    case("literals_repo_strings", &["literals", "--kind", "string"]),
    case(
        "literals_numbers",
        &["literals", "--kind", "number", "src/api"],
    ),
    case("literals_regex", &["literals", "--kind", "regex"]),
    case("literals_match", &["literals", "--match", "connection"]),
    case(
        "literals_in_symbol",
        &[
            "literals",
            "--in",
            "src/api/users.py#UserService.create_user",
        ],
    ),
    case(
        "literals_ungrouped",
        &["literals", "src/core/config.py", "--no-group"],
    ),
    case(
        "literals_no_skeleton",
        &["literals", "src/core/config.py", "--no-skeleton"],
    ),
    case(
        "literals_docstrings",
        &["literals", "src/api/users.py", "--include-docstrings"],
    ),
    case(
        "literals_min_len",
        &["literals", "src/api", "--min-len", "12"],
    ),
    case(
        "literals_json",
        &["literals", "src/core/config.py", "--json"],
    ),
    case("literals_budget", &["literals", "--budget", "40"]),
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
fn find_excludes_comments_and_strings_by_default() {
    let bare = run(&["find", "never appear"]);
    assert!(bare.contains("exit: 1"));
    let with_comments = run(&["find", "never appear", "--include-comments"]);
    assert!(with_comments.contains("exit: 0"));
    assert!(with_comments.contains("comment"));
}

#[test]
fn find_interns_the_path_once_per_file() {
    let output = run(&["find", "create_user"]);
    assert_eq!(output.matches("src/api/routes.py").count(), 1);
}

#[test]
fn literals_group_identical_values_with_counts() {
    let output = run(&[
        "literals",
        "--kind",
        "string",
        "--match",
        "connection refused",
    ]);
    assert!(output.contains("connection refused: <>"));
}

#[test]
fn literals_addresses_feed_back_into_slice() {
    let listing = run(&[
        "literals",
        "--in",
        "src/api/users.py#UserService.create_user",
    ]);
    assert!(listing.contains("src/api/users.py#UserService.create_user"));
    let sliced = run(&["slice", "src/api/users.py#UserService.create_user"]);
    assert!(sliced.contains("exit: 0"));
}

#[test]
fn directory_map_finds_entry_points() {
    let output = run(&["map", "."]);
    assert!(output.contains("__main__ guard"));
    assert!(output.contains("src/cli.py#main"));
    assert!(output.contains("@app.get(\"/health\")"));
    assert!(output.contains("console script `fixture`"));
}

#[test]
fn budget_degrades_instead_of_truncating() {
    let output = run(&["find", "user", "--budget", "40"]);
    assert!(output.contains("raise --budget") || output.contains("budget reached"));
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
