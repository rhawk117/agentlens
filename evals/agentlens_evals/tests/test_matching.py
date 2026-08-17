from agentlens_evals.matching import fact_satisfied, phrase_matches


def test_equals_notation_matches_prose() -> None:
    assert phrase_matches(
        "the defaults are `sync_capable=True`", "sync_capable defaults to true"
    )


def test_synonym_class_collapses() -> None:
    assert phrase_matches("the output is not smaller", "not shorter")


def test_negation_parity_blocks_denial() -> None:
    assert not phrase_matches("does not skip compression", "skips compression")
    assert not phrase_matches("skips compression", "does not skip compression")


def test_fact_satisfied_any_of() -> None:
    fact = {"id": "f1", "any_of": ["runs in reverse order", "iterates backward"]}
    assert fact_satisfied("middleware iterates backward over the list", fact)
    assert not fact_satisfied("middleware runs forward", fact)
