from pathlib import Path

from agentlens_evals.grading import (
    RunArtifacts,
    address_found,
    call_retrieves,
    forbidden_penalty,
    grade_artifacts,
)

GOLD = {
    "address": "django/core/handlers/base.py#BaseHandler.get_response",
    "start_line": 140,
    "end_line": 165,
}

TASK = {
    "id": "T01",
    "type": "comprehension",
    "prompt": "irrelevant",
    "required_addresses": [GOLD],
    "supporting_addresses": [],
    "required_facts": [{"id": "f1", "any_of": ["returns the response"]}],
    "forbidden_claims": ["caches the response"],
}


def artifacts(answer: str, transcript=None, arm: str = "baseline") -> RunArtifacts:
    return RunArtifacts(
        run_id=f"r1-{arm}-T01",
        arm=arm,
        answer=answer,
        transcript=transcript or [],
        capped=False,
        run_dir=Path("/nonexistent"),
    )


def test_address_exact_and_windowed_forms() -> None:
    assert address_found(
        "see django/core/handlers/base.py#BaseHandler.get_response", GOLD
    )
    assert address_found("in django/core/handlers/base.py, get_response does it", GOLD)
    assert address_found("django/core/handlers/base.py lines 150-160", GOLD)
    assert not address_found("django/core/handlers/base.py lines 400-420", GOLD)
    assert not address_found("django/urls/resolvers.py get_response", GOLD)


def test_score_formula_and_floor() -> None:
    graded = grade_artifacts(
        TASK,
        artifacts(
            "django/core/handlers/base.py#BaseHandler.get_response returns the response"
        ),
    )
    assert graded["address_score"] == 1.0 and graded["fact_score"] == 1.0
    assert graded["score"] == 1.0
    localization = dict(TASK, type="localization", required_facts=[])
    graded = grade_artifacts(localization, artifacts("no idea, maybe somewhere"))
    assert graded["score"] == 0.0


def test_forbidden_hedged_vs_flat() -> None:
    flat, matched = forbidden_penalty(
        "it caches the response always", ["caches the response"]
    )
    assert flat == 0.25 and matched == ["caches the response"]
    hedged, _ = forbidden_penalty(
        "it possibly caches the response", ["caches the response"]
    )
    assert hedged == 0.1
    negated, matched = forbidden_penalty(
        "it does not cache anything; never caches the response", ["caches the response"]
    )
    assert negated == 0.0 and matched == []


def test_navigation_sed_span_overlap() -> None:
    record = {
        "exit_code": 0,
        "stdout": "some source text",
        "stderr": "",
        "tool": "sed",
        "args": ["-n", "150,160p", "django/core/handlers/base.py"],
        "call_index": 3,
        "result_tokens": 10,
    }
    assert call_retrieves(record, "linerange", [GOLD])
    miss = dict(record, args=["-n", "400,420p", "django/core/handlers/base.py"])
    assert not call_retrieves(miss, "linerange", [GOLD])


def test_navigation_cat_named_file() -> None:
    record = {
        "exit_code": 0,
        "stdout": "content",
        "stderr": "",
        "tool": "cat",
        "args": ["django/core/handlers/base.py"],
        "call_index": 1,
        "result_tokens": 5,
    }
    assert call_retrieves(record, "baseline", [GOLD])


def test_evaluator_wraps_grading() -> None:
    from pydantic_evals.evaluators import EvaluatorContext

    from agentlens_evals.evaluators import BenchmarkScores

    ctx = EvaluatorContext(
        name="T01",
        inputs="r1-baseline-T01",
        metadata=TASK,
        expected_output=None,
        output=artifacts(
            "django/core/handlers/base.py#BaseHandler.get_response returns the response"
        ),
        duration=0.0,
        _span_tree=None,
        attributes={},
        metrics={},
    )
    scores = BenchmarkScores().evaluate(ctx)
    assert scores["score"] == 1.0 and scores["navigation"] == 26.0
