from __future__ import annotations

import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import bench_tool
from bench_tool import cap_result, parse_run_id, validate_baseline, validate_linerange
from grade import address_found, contains_asserted

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
