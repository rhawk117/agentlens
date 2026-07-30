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
    case(
        "callers_method",
        &[
            "callers",
            "src/api/users.py#UserService.create_user",
            "--no-cache",
        ],
    ),
    case(
        "callers_same_file",
        &["callers", "src/cli.py#main", "--no-cache"],
    ),
    case(
        "callers_no_tests",
        &[
            "callers",
            "src/api/users.py#UserService.create_user",
            "--no-tests",
            "--no-cache",
        ],
    ),
    case(
        "callers_none",
        &["callers", "src/core/config.py#banner", "--no-cache"],
    ),
    case(
        "callers_missing",
        &["callers", "src/core/config.py#nope", "--no-cache"],
    ),
    case(
        "callers_json",
        &[
            "callers",
            "src/api/users.py#UserService.create_user",
            "--json",
            "--no-cache",
        ],
    ),
    case(
        "callers_budget",
        &[
            "callers",
            "src/api/users.py#UserService.create_user",
            "--budget",
            "40",
            "--no-cache",
        ],
    ),
    case(
        "packet_method",
        &[
            "packet",
            "src/api/users.py#UserService.create_user",
            "--no-cache",
        ],
    ),
    case(
        "packet_json",
        &[
            "packet",
            "src/core/config.py#describe",
            "--json",
            "--no-cache",
        ],
    ),
    case(
        "packet_budget",
        &[
            "packet",
            "src/api/users.py#UserService.create_user",
            "--budget",
            "120",
            "--no-cache",
        ],
    ),
    case("dead_repo", &["dead", "--no-cache"]),
    case("dead_json", &["dead", "--json", "--no-cache"]),
    case("dead_budget", &["dead", "--budget", "40", "--no-cache"]),
    case(
        "coerce_bare_file_to_outline",
        &["slice", "src/api/users.py"],
    ),
    case(
        "coerce_positional_line_range",
        &["slice", "src/api/users.py", "1", "20"],
    ),
    case(
        "coerce_positional_single_line",
        &["slice", "src/api/users.py", "21"],
    ),
    case(
        "coerce_positional_symbol",
        &["slice", "src/api/users.py", "UserService"],
    ),
    case(
        "coerce_colon_line_range",
        &["slice", "src/api/users.py:1-20"],
    ),
    case(
        "coerce_colon_single_line",
        &["slice", "src/api/users.py:21"],
    ),
    case(
        "coerce_colon_range_then_symbol",
        &["slice", "src/api/users.py:1-20#UserService"],
    ),
    case(
        "coerce_colon_range_then_outline",
        &["slice", "src/api/users.py:1-20#"],
    ),
    case(
        "coerce_double_colon_symbol",
        &["slice", "src/api/users.py::UserService"],
    ),
    case(
        "coerce_symbol_beats_placeholder",
        &["slice", "src/api/users.py::UserService#Symbol"],
    ),
    case(
        "coerce_is_silenced_by_quiet",
        &["slice", "src/api/users.py:1-20", "--quiet"],
    ),
    case(
        "map_rooted_at_a_symbol",
        &["map", "src/api/users.py#UserService"],
    ),
    case("error_no_hash_and_no_file", &["slice", "nowhere"]),
    case(
        "error_unresolved_extra_arguments",
        &["slice", "nowhere", "1", "20"],
    ),
    case(
        "error_bad_line_span",
        &["slice", "src/api/users.py#L20-L10"],
    ),
    case("error_missing_symbol", &["slice", "src/api/users.py#Nope"]),
    case("error_unknown_kind", &["map", ".", "--kind", "nonsense"]),
    case("error_unknown_help_topic", &["help", "nonsense"]),
    case("help_addresses", &["help", "addresses"]),
    case("json_error_slice", &["slice", "nowhere#X", "--json"]),
    case("json_error_map", &["map", "nowhere", "--json"]),
    case("json_error_find", &["find", "[unclosed", "--json"]),
    case(
        "json_error_literals",
        &["literals", "--kind", "nonsense", "--json"],
    ),
    case(
        "json_error_callers",
        &["callers", "nowhere#X", "--json", "--no-cache"],
    ),
    case(
        "json_error_packet",
        &["packet", "nowhere#X", "--json", "--no-cache"],
    ),
    case("json_error_help", &["help", "nonsense", "--json"]),
    // --no-cache pins from_cache to false. Without it the snapshot depends on
    // whether another test wrote .agentlens-cache first, which is a race.
    case(
        "json_provenance_callers",
        &[
            "callers",
            "src/api/users.py#UserService.create_user",
            "--json",
            "--no-cache",
        ],
    ),
    case(
        "json_error_with_suggestion",
        &["slice", "src/api/users.py#L20-L10", "--json"],
    ),
    case(
        "json_bare_name_absent",
        &["slice", "nohash", "--json", "--no-cache"],
    ),
    case("sym_constant", &["sym", "MAX_RETRIES", "--no-cache"]),
    case("sym_method", &["sym", "create_user", "--no-cache"]),
    case(
        "sym_overloads_share_one_address",
        &["sym", "normalise", "--no-cache"],
    ),
    case("sym_absent", &["sym", "NoSuchThing", "--no-cache"]),
    case(
        "sym_dotted",
        &["sym", "UserService.create_user", "--no-cache"],
    ),
    case(
        "bare_name_resolves_for_slice",
        &["slice", "UserService", "--no-cache"],
    ),
    case(
        "bare_name_resolves_for_packet",
        &["packet", "UserService", "--no-cache"],
    ),
    case("bare_name_absent", &["slice", "NoSuchThing", "--no-cache"]),
];

#[test]
fn every_json_case_emits_parseable_json_on_stdout() {
    for item in CASES {
        if !item.args.contains(&"--json") {
            continue;
        }
        let rendered = run(item.args);
        let stdout = rendered
            .split_once("--- stdout ---")
            .and_then(|(_, rest)| rest.split_once("--- stderr ---"))
            .map(|(body, _)| body)
            .expect("rendered run has both streams");
        let value: serde_json::Value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|err| panic!("{} did not emit json: {err}\n{stdout}", item.name));
        assert_eq!(
            value["schema_version"], 1,
            "{} lacks schema_version",
            item.name
        );
        assert!(
            value["tool_version"].is_string(),
            "{} lacks tool_version",
            item.name
        );
    }
}

#[test]
fn index_provenance_travels_with_index_backed_commands() {
    let rendered = run(&[
        "callers",
        "src/api/users.py#UserService.create_user",
        "--json",
    ]);
    let stdout = rendered
        .split_once("--- stdout ---")
        .and_then(|(_, rest)| rest.split_once("--- stderr ---"))
        .map(|(body, _)| body)
        .expect("rendered run has both streams");
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json");
    assert!(value["index"]["files"].is_u64(), "no index.files");
    assert!(
        value["index"]["from_cache"].is_boolean(),
        "no index.from_cache"
    );
    // slice builds no index, so it must not claim one.
    let plain = run(&["slice", "src/api/users.py#User", "--json"]);
    assert!(!plain.contains("\"index\""));
}

#[test]
fn the_next_call_footer_is_dropped_under_json() {
    let text = run(&["map", "src/api/users.py"]);
    assert!(text.contains("for a body"));
    let json = run(&["map", "src/api/users.py", "--json"]);
    assert!(!json.contains("for a body"));
}

// The shared fixture has no literal long enough to collapse, and adding one
// would churn every map, dead and literals snapshot. The corpus fixture
// already carries a settings-shaped file, so these cases run there.
const SETTINGS_CASES: &[Case] = &[
    case(
        "map_collapses_a_large_literal",
        &["map", "django/conf/global_settings.py", "--no-cache"],
    ),
    case(
        "map_expand_shows_the_whole_literal",
        &[
            "map",
            "django/conf/global_settings.py",
            "--expand",
            "--budget",
            "100000",
            "--no-cache",
        ],
    ),
    case(
        "map_match_filters_the_outline",
        &[
            "map",
            "django/conf/global_settings.py",
            "--match",
            "^SECURE",
            "--no-cache",
        ],
    ),
    case(
        "map_match_that_matches_nothing",
        &[
            "map",
            "django/conf/global_settings.py",
            "--match",
            "zzz_nothing",
            "--no-cache",
        ],
    ),
];

#[test]
fn snapshots_match() {
    for item in CASES {
        check_snapshot(item.name, &run(item.args));
    }
    let settings = support::workspace_root().join("tests/fixtures/django-shapes");
    for item in SETTINGS_CASES {
        check_snapshot(item.name, &support::run_in(&settings, item.args));
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
    // A bare file used to be a tool fault. It is an outline now, and a
    // mistyped address is a miss rather than a fault.
    assert!(run(&["slice", "src/api/users.py"]).contains("exit: 0"));
    assert!(run(&["slice", "nowhere"]).contains("exit: 1"));
}

#[test]
fn the_coercion_note_stays_off_stdout() {
    let plain = run(&["slice", "src/api/users.py:1-20"]);
    let (stdout, stderr) = plain
        .split_once("--- stderr ---")
        .expect("rendered run has both streams");
    assert!(!stdout.contains("note:"), "note leaked onto stdout");
    assert!(stderr.contains("note: read `src/api/users.py:1-20`"));
}

#[test]
fn json_stdout_is_identical_with_and_without_coercion() {
    let coerced = run(&["slice", "src/api/users.py:1-20", "--json"]);
    let canonical = run(&["slice", "src/api/users.py#L1-L20", "--json"]);
    let body = |rendered: &str| {
        rendered
            .split_once("--- stdout ---")
            .and_then(|(_, rest)| rest.split_once("--- stderr ---"))
            .map(|(stdout, _)| stdout.to_string())
            .expect("rendered run has both streams")
    };
    assert_eq!(body(&coerced), body(&canonical));
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
fn callers_are_graded_and_tagged() {
    let output = run(&[
        "callers",
        "src/api/users.py#UserService.create_user",
        "--no-cache",
    ]);
    assert!(output.contains("likely"));
    assert!(output.contains("[test]"));
    assert!(output.contains("1 test"));
}

#[test]
fn a_bare_same_file_call_is_certain() {
    let output = run(&["callers", "src/cli.py#main", "--no-cache"]);
    assert!(output.contains("certain"));
}

#[test]
fn packet_carries_body_plus_signatures_only() {
    let output = run(&[
        "packet",
        "src/api/users.py#UserService.create_user",
        "--no-cache",
    ]);
    assert!(output.contains("def create_user(self, email: str, name: str) -> User:"));
    assert!(output.contains("raise ValueError"));
    assert!(output.contains("types in the signature"));
    assert!(!output.contains("return {\"email\": user.email}"));
}

#[test]
fn dead_lists_candidates_never_dead_code() {
    let output = run(&["dead", "--no-cache"]);
    assert!(output.contains("candidates"));
    assert!(output.contains("src/api/routes.py#_unused_helper"));
    assert!(!output.contains("src/cli.py#main"));
    assert!(!output.contains("tests/test_users.py"));
}

#[test]
fn the_cache_is_self_concealing_and_silent() {
    let repo = support::temp_repo("cache");
    let first = support::run_in(&repo, &["dead"]);
    assert!(repo.join(".agentlens-cache/index.json").is_file());
    assert_eq!(
        std::fs::read_to_string(repo.join(".agentlens-cache/.gitignore")).expect("gitignore"),
        "*\n"
    );
    let second = support::run_in(&repo, &["dead"]);
    assert_eq!(first, second, "cached run must match the cold run");
}

#[test]
fn a_corrupt_cache_rebuilds_silently() {
    let repo = support::temp_repo("corrupt");
    let cold = support::run_in(&repo, &["dead"]);
    std::fs::write(repo.join(".agentlens-cache/index.json"), "{ not json").expect("corrupt");
    let after = support::run_in(&repo, &["dead"]);
    assert_eq!(cold, after);
}

#[test]
fn edits_invalidate_the_cached_entry() {
    let repo = support::temp_repo("invalidate");
    let before = support::run_in(&repo, &["map", "src/core/config.py"]);
    assert!(before.contains("def banner() -> str:"));
    let path = repo.join("src/core/config.py");
    let text = std::fs::read_to_string(&path).expect("read");
    std::fs::write(&path, text.replace("def banner()", "def masthead()")).expect("write");
    let after = support::run_in(&repo, &["callers", "src/core/config.py#masthead"]);
    assert!(after.contains("src/core/config.py#masthead"));
    assert!(!after.contains("no symbol"));
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
