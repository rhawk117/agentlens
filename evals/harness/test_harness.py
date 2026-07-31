from __future__ import annotations

import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import bench_tool
import detect_leaks
from bench_tool import cap_result, parse_run_id, validate_baseline, validate_linerange
from grade import address_found, contains_asserted
from matching import phrase_matches

GOLD = {
    "address": "django/core/handlers/base.py#BaseHandler.load_middleware",
    "start_line": 26,
    "end_line": 103,
}


class AddressMatchingTests(unittest.TestCase):
    def test_symbolic_address_matches(self) -> None:
        self.assertTrue(
            address_found(
                "Edit django/core/handlers/base.py#BaseHandler.load_middleware.",
                GOLD,
            )
        )

    def test_overlapping_line_span_matches(self) -> None:
        self.assertTrue(address_found("django/core/handlers/base.py:L40-L50", GOLD))

    def test_unrelated_number_near_path_does_not_match(self) -> None:
        self.assertFalse(
            address_found(
                "Django 6.0.7 uses django/core/handlers/base.py for handlers.",
                GOLD,
            )
        )

    def test_symbol_substring_does_not_match(self) -> None:
        self.assertFalse(
            address_found(
                "django/core/handlers/base.py has reload_middleware_factory.",
                GOLD,
            )
        )

    def test_symbolic_address_with_longer_selector_does_not_match(self) -> None:
        self.assertFalse(
            address_found(
                "django/core/handlers/base.py#BaseHandler.load_middleware_extra",
                GOLD,
            )
        )

    def test_prose_line_reference_matches(self) -> None:
        # `L40-L50` and `:40-50` are how agentlens prints an address. An arm
        # reading with `sed -n '40,50p'` or `cat` has no reason to adopt that
        # notation and writes what a person writes. Requiring the tool's own
        # spelling would score the controls down for not sounding like the tool
        # under test, which is the one thing address matching must never do.
        for phrasing in (
            "**django/core/handlers/base.py** (lines 40-50)",
            "django/core/handlers/base.py, lines 40–50",
            "django/core/handlers/base.py at line 40",
            "See django/core/handlers/base.py lines 40 to 50.",
        ):
            with self.subTest(phrasing=phrasing):
                self.assertTrue(address_found(phrasing, GOLD))

    def test_prose_line_reference_outside_the_span_does_not_match(self) -> None:
        self.assertFalse(address_found("django/core/handlers/base.py (lines 400-450)", GOLD))

    def test_prose_line_word_is_required(self) -> None:
        # A bare number beside a path is prose, not a citation -- "Django 6.0.7"
        # and "10 middleware" would both otherwise resolve to a line number.
        self.assertFalse(address_found("django/core/handlers/base.py defines 40 things", GOLD))


class AssertionMatchingTests(unittest.TestCase):
    def test_negated_fact_is_not_asserted(self) -> None:
        text = "it does not silently continue"
        self.assertFalse(contains_asserted(text, "silently continue"))

    def test_positive_fact_is_asserted(self) -> None:
        self.assertTrue(contains_asserted("it silently continues", "silently continues"))


class BaselineValidationTests(unittest.TestCase):
    def test_preprocessor_is_rejected(self) -> None:
        with self.assertRaises(SystemExit):
            validate_baseline("rg", ["--pre=cat", "pattern", "."])

    def test_hidden_cache_is_rejected(self) -> None:
        with self.assertRaises(SystemExit):
            validate_baseline("rg", ["pattern", ".agentlens-cache/index.json"])

    def test_common_rg_usage_is_allowed(self) -> None:
        validate_baseline("rg", ["-n", "-g", "*.py", "pattern", "django"])


class CapTests(unittest.TestCase):
    def test_cap_never_injects_more_than_remaining(self) -> None:
        rendered, injected, full, truncated = cap_result("word " * 100, 10)
        self.assertTrue(truncated)
        self.assertGreater(full, injected)
        self.assertLessEqual(injected, 10)
        self.assertTrue(rendered)

    def test_small_result_is_unchanged(self) -> None:
        rendered, injected, full, truncated = cap_result("small", 10)
        self.assertEqual(rendered, "small")
        self.assertEqual(injected, full)
        self.assertFalse(truncated)


class LineRangeValidationTests(unittest.TestCase):
    """Arm C hands `sed` to an untrusted agent.

    `sed` is a programming language, not a pager: `w` writes files, `r` reads
    them, `e` executes shell, and `-i` edits the subject corpus in place. Any
    of those would let a worker read outside the corpus, corrupt the corpus
    for every later run, or escape the token accounting the benchmark's
    central metric depends on. Only the print-range form is accepted, so each
    test below names a specific escape the allowlist must refuse.
    """

    def setUp(self) -> None:
        # A real file must exist at the target path, or every rejection below
        # would pass for the wrong reason -- "no such file" rather than "that
        # form is not allowed" -- and the allowlist would be untested.
        self.corpus = Path(tempfile.mkdtemp())
        subject = self.corpus / "django" / "core" / "handlers" / "base.py"
        subject.parent.mkdir(parents=True)
        subject.write_text("x\n" * 200, encoding="utf-8")
        (self.corpus / "django" / "urls").mkdir(parents=True)
        (self.corpus / "django" / "urls" / "base.py").write_text("y\n", encoding="utf-8")
        patch = mock.patch.object(bench_tool, "DJANGO_ROOT", self.corpus.resolve())
        patch.start()
        self.addCleanup(patch.stop)
        self.addCleanup(shutil.rmtree, self.corpus)

    def reject(self, args: list[str]) -> None:
        with self.assertRaises(SystemExit):
            validate_linerange("sed", args)

    def test_print_range_is_allowed(self) -> None:
        validate_linerange("sed", ["-n", "1,120p", "django/core/handlers/base.py"])

    def test_single_line_print_is_allowed(self) -> None:
        validate_linerange("sed", ["-n", "26p", "django/core/handlers/base.py"])

    def test_expression_flag_is_rejected(self) -> None:
        self.reject(["-e", "1,10p", "django/core/handlers/base.py"])

    def test_in_place_edit_is_rejected(self) -> None:
        self.reject(["-i", "s/a/b/", "django/core/handlers/base.py"])

    def test_script_file_is_rejected(self) -> None:
        self.reject(["-f", "script.sed", "django/core/handlers/base.py"])

    def test_write_command_is_rejected(self) -> None:
        self.reject(["w /etc/passwd", "django/core/handlers/base.py"])

    def test_write_inside_range_is_rejected(self) -> None:
        self.reject(["-n", "1,5w out", "django/core/handlers/base.py"])

    def test_execute_command_is_rejected(self) -> None:
        self.reject(["-n", "1e whoami", "django/core/handlers/base.py"])

    def test_read_command_is_rejected(self) -> None:
        self.reject(["-n", "1r /etc/passwd", "django/core/handlers/base.py"])

    def test_absolute_target_is_rejected(self) -> None:
        self.reject(["-n", "1,5p", "/etc/passwd"])

    def test_parent_traversal_is_rejected(self) -> None:
        self.reject(["-n", "1,5p", "../../../etc/passwd"])

    def test_target_outside_corpus_is_rejected(self) -> None:
        self.reject(["-n", "1,5p", "does/not/exist.py"])

    def test_second_file_is_rejected(self) -> None:
        self.reject(
            [
                "-n",
                "1,5p",
                "django/core/handlers/base.py",
                "django/urls/base.py",
            ]
        )

    def test_missing_quiet_flag_is_rejected(self) -> None:
        # Without -n, `sed '1,5p'` prints the whole file *and* duplicates the
        # range, which would understate Arm C's true token cost.
        self.reject(["1,5p", "django/core/handlers/base.py"])

    def test_cat_is_unavailable(self) -> None:
        # `cat` would collapse Arm C into Arm B; the arm exists to measure
        # what precise line-range reads cost.
        with self.assertRaises(SystemExit):
            validate_linerange("cat", ["django/core/handlers/base.py"])

    def test_rg_is_allowed_and_shares_the_baseline_allowlist(self) -> None:
        validate_linerange("rg", ["-n", "-g", "*.py", "pattern", "django"])
        with self.assertRaises(SystemExit):
            validate_linerange("rg", ["--pre=cat", "pattern", "."])


class FactMatcherTests(unittest.TestCase):
    """Each case is a real v0.1.0 miss, or a false positive that must stay out."""

    def test_identifier_assignment_matches_prose(self) -> None:
        self.assertTrue(
            phrase_matches(
                "The defaults are `sync_capable=True` and `async_capable=False`.",
                "sync_capable defaults to true",
            )
        )

    def test_interposed_words_do_not_break_the_match(self) -> None:
        self.assertTrue(
            phrase_matches("Hooks run in reverse MIDDLEWARE order.", "run in reverse order")
        )

    def test_inflection_differences_match(self) -> None:
        self.assertTrue(
            phrase_matches(
                "tells Django to omit that middleware and continue loading the rest",
                "continues loading",
            )
        )

    def test_comparative_synonyms_match(self) -> None:
        self.assertTrue(
            phrase_matches(
                "skipped when the compressed content is not smaller than the original",
                "compressed content is not shorter",
            )
        )

    def test_paraphrase_without_shared_nouns_is_not_matched(self) -> None:
        """A documented limit, asserted so it cannot regress silently.

        Gold wants "compressed content is not shorter"; a v0.1.0 answer said
        "the gzip result is not smaller than the original". Those are the same
        claim, but they share no content noun, and the only way to join them is
        to stop requiring the nouns -- which is precisely what would let
        unrelated sentences satisfy arbitrary facts. The matcher accepts the
        miss rather than buy recall with false positives.
        """
        self.assertFalse(
            phrase_matches(
                "skipped when the gzip result is not smaller than the original",
                "compressed content is not shorter",
            )
        )

    def test_negation_parity_blocks_a_reversed_claim(self) -> None:
        # The whole point of tracking negation: an answer saying the opposite
        # must not satisfy the fact just because it shares the same words.
        self.assertFalse(
            phrase_matches("GZipMiddleware does not skip compression here.", "skips compression")
        )

    def test_a_negated_fact_needs_the_negation(self) -> None:
        self.assertFalse(
            phrase_matches("the compressed content is shorter", "compressed content is not shorter")
        )

    def test_unrelated_answer_does_not_match(self) -> None:
        self.assertFalse(
            phrase_matches(
                "CommonMiddleware appends a slash and redirects permanently.",
                "sync_capable defaults to true",
            )
        )

    def test_tokens_scattered_across_the_answer_do_not_match(self) -> None:
        # Every required token is present, but spread far apart and about
        # different things. A window that matched this would manufacture facts.
        scattered = (
            "The response is compressed by GZipMiddleware. " + "Filler sentence. " * 40
        ) + "A shorter path is not taken by the resolver."
        self.assertFalse(phrase_matches(scattered, "compressed content is not shorter"))

    def test_empty_phrase_never_matches(self) -> None:
        self.assertFalse(phrase_matches("anything at all", "  "))


class SubprocessEnvironmentTests(unittest.TestCase):
    """The measured environment must not depend on who is running the benchmark.

    Inheriting os.environ let RIPGREP_CONFIG_PATH reach `rg` during a smoke
    test. On this machine the file was absent so rg only warned -- but the
    warning was captured as tool output and billed as result tokens against a
    control arm, and on a machine where that config exists it would silently
    change how the controls search.
    """

    def test_ripgrep_config_does_not_leak(self) -> None:
        with mock.patch.dict(os.environ, {"RIPGREP_CONFIG_PATH": "/tmp/attacker"}, clear=False):
            self.assertNotIn("RIPGREP_CONFIG_PATH", bench_tool.subject_env())

    def test_unrelated_host_variables_do_not_leak(self) -> None:
        with mock.patch.dict(os.environ, {"AGENTLENS_LOG": "trace"}, clear=False):
            self.assertNotIn("AGENTLENS_LOG", bench_tool.subject_env())

    def test_path_and_determinism_settings_are_present(self) -> None:
        env = bench_tool.subject_env()
        self.assertIn("PATH", env)
        self.assertEqual(env["NO_COLOR"], "1")
        self.assertEqual(env["LC_ALL"], "C")


class LeakDetectionTests(unittest.TestCase):
    """The check that catches a worker answering from memory rather than retrieval."""

    def setUp(self) -> None:
        self.runs = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.runs)
        patch = mock.patch.object(detect_leaks, "run_directory", self.run_directory)
        patch.start()
        self.addCleanup(patch.stop)

    def run_directory(self, run_id: str) -> Path:
        directory = self.runs / run_id
        directory.mkdir(parents=True, exist_ok=True)
        return directory

    def write(self, run_id: str, answer: str, records: list[dict]) -> None:
        directory = self.run_directory(run_id)
        (directory / "answer.txt").write_text(answer, encoding="utf-8")
        (directory / "transcript.jsonl").write_text(
            "\n".join(json.dumps(record) for record in records) + "\n", encoding="utf-8"
        )

    def test_cited_path_present_in_output_is_clean(self) -> None:
        self.write(
            "r1-agentlens-M01",
            "Edit django/core/handlers/base.py to fix it.",
            [
                {
                    "stdout": "django/core/handlers/base.py:26: def load_middleware",
                    "stderr": "",
                    "args": [],
                }
            ],
        )
        self.assertEqual(detect_leaks.unsupported_citations("r1-agentlens-M01"), [])

    def test_path_only_in_the_arguments_is_clean(self) -> None:
        # Asking about a path and being told nothing is there still means the
        # path entered this run through the wrapper, not from memory.
        self.write(
            "r1-agentlens-M02",
            "See django/urls/base.py",
            [{"stdout": "", "stderr": "no match", "args": ["slice", "django/urls/base.py"]}],
        )
        self.assertEqual(detect_leaks.unsupported_citations("r1-agentlens-M02"), [])

    def test_path_never_retrieved_is_flagged(self) -> None:
        self.write(
            "r1-baseline-M03",
            "The answer is in django/core/handlers/base.py",
            [{"stdout": "django/urls/resolvers.py:12: something else", "stderr": "", "args": []}],
        )
        self.assertEqual(
            detect_leaks.unsupported_citations("r1-baseline-M03"),
            ["django/core/handlers/base.py"],
        )

    def test_a_run_with_no_answer_is_not_judged(self) -> None:
        directory = self.run_directory("r1-agentlens-M04")
        (directory / "transcript.jsonl").write_text("", encoding="utf-8")
        self.assertIsNone(detect_leaks.unsupported_citations("r1-agentlens-M04"))


class RunIdTests(unittest.TestCase):
    def test_five_repetitions_are_accepted(self) -> None:
        self.assertEqual(parse_run_id("r5-linerange-M01"), (5, "linerange", "M01"))

    def test_sixth_repetition_is_rejected(self) -> None:
        with self.assertRaises(SystemExit):
            parse_run_id("r6-agentlens-M01")

    def test_unknown_arm_is_rejected(self) -> None:
        with self.assertRaises(SystemExit):
            parse_run_id("r1-cheating-M01")

    def test_every_arm_parses(self) -> None:
        for arm in ("agentlens", "baseline", "linerange"):
            self.assertEqual(parse_run_id(f"r1-{arm}-M01")[1], arm)


if __name__ == "__main__":
    unittest.main()
