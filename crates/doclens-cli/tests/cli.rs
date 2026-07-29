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
    case("map_yaml", &["map", "compose.yaml", "--depth", "3"]),
    case("map_json", &["map", "package.json"]),
    case("map_markdown", &["map", "README.md", "--depth", "3"]),
    case("map_dir", &["map", "."]),
    case("map_json_json", &["map", "package.json", "--json"]),
    case(
        "slice_yaml_mapping",
        &["slice", "compose.yaml#services.web"],
    ),
    case(
        "slice_yaml_index",
        &["slice", "compose.yaml#services.web.ports[1]"],
    ),
    case("slice_yaml_scalar", &["slice", "compose.yaml#version"]),
    case(
        "slice_yaml_with_key",
        &["slice", "compose.yaml#services.db", "--with-key"],
    ),
    case("slice_json_object", &["slice", "package.json#scripts"]),
    case(
        "slice_json_array_item",
        &["slice", "package.json#keywords[2]"],
    ),
    case(
        "slice_markdown_section",
        &["slice", "README.md#Fixture/Install/From source"],
    ),
    case("slice_markdown_top", &["slice", "README.md#Fixture/Usage"]),
    case("slice_outline", &["slice", "compose.yaml#"]),
    case("slice_missing", &["slice", "package.json#scripts.deploy"]),
    case("slice_no_hash", &["slice", "package.json"]),
    case("slice_unsupported", &["slice", "notes.txt#a"]),
    case(
        "slice_json_output",
        &["slice", "compose.yaml#services.db", "--json"],
    ),
    case(
        "slice_budget",
        &["slice", "compose.yaml#services", "--budget", "20"],
    ),
    case("find_plain", &["find", "postgres"]),
    case("find_keys", &["find", "image", "--keys"]),
    case("find_values", &["find", "vitest", "--values"]),
    case("find_miss", &["find", "zzz_nothing"]),
    case("find_json", &["find", "8443", "--json"]),
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
        assert_eq!(
            run(item.args),
            run(item.args),
            "non-deterministic: {}",
            item.name
        );
    }
}

#[test]
fn exit_codes_branch_without_parsing() {
    assert!(run(&["slice", "compose.yaml#version"]).contains("exit: 0"));
    assert!(run(&["slice", "compose.yaml#nope"]).contains("exit: 1"));
    assert!(run(&["slice", "compose.yaml"]).contains("exit: 2"));
}

#[test]
fn slices_are_byte_spans_so_comments_survive() {
    let output = run(&["slice", "compose.yaml#services.web.image"]);
    assert!(output.contains("nginx:1.27"));
    let block = run(&["slice", "compose.yaml#services.web"]);
    assert!(block.contains("# pinned deliberately"));
}

#[test]
fn json_key_order_and_formatting_survive() {
    let output = run(&["slice", "package.json#scripts"]);
    let build = output.find("\"build\"").expect("build key");
    let test = output.find("\"test\"").expect("test key");
    assert!(build < test, "key order must not be re-serialised");
}

#[test]
fn fenced_hashes_are_not_markdown_headings() {
    let outline = run(&["map", "README.md", "--depth", "6"]);
    assert!(!outline.contains("this hash is not a heading"));
}

#[test]
fn a_markdown_section_carries_its_subsections() {
    let output = run(&["slice", "README.md#Fixture/Install"]);
    assert!(output.contains("### From source"));
    assert!(output.contains("### From a release"));
    assert!(!output.contains("## Usage"));
}

#[test]
fn every_outline_row_is_a_valid_address() {
    let outline = run(&["map", "compose.yaml", "--depth", "3"]);
    assert!(outline.contains("environment"));
    let sliced = run(&["slice", "compose.yaml#services.web.environment.LOG_LEVEL"]);
    assert!(sliced.contains("debug"));
    assert!(sliced.contains("exit: 0"));
}
