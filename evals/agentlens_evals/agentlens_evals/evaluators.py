"""Graders as pydantic-evals evaluators.

Thin wrappers: all scoring arithmetic lives in grading.py, which is the
verbatim grade.py port. An evaluator that recomputed anything would be a
second implementation to keep in sync -- exactly what this module exists to
avoid.

Navigation is reported as call index 1..25; a run that never retrieved gold
is charged one past the call cap (NAVIGATION_CAP_SENTINEL), matching
grade.py's aggregation.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from pydantic_evals.evaluators import Evaluator, EvaluatorContext

from agentlens_evals.grading import RunArtifacts, grade_artifacts

NAVIGATION_CAP_SENTINEL = 26.0


@dataclass
class BenchmarkScores(Evaluator[str, RunArtifacts, dict[str, Any]]):
    def evaluate(
        self, ctx: EvaluatorContext[str, RunArtifacts, dict[str, Any]]
    ) -> dict:
        graded = grade_artifacts(ctx.metadata, ctx.output)
        fact_score = graded["fact_score"]
        navigation = graded["navigation"]
        return {
            "score": graded["score"],
            "address_score": graded["address_score"],
            "fact_score": -1.0 if fact_score is None else float(fact_score),
            "penalty": graded["penalty"],
            "navigation": (
                NAVIGATION_CAP_SENTINEL if navigation is None else float(navigation)
            ),
            "tool_calls": float(graded["tool_calls"]),
            "tool_result_tokens": float(graded["tool_result_tokens"]),
            "capped": graded["capped"],
        }
