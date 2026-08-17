import shutil

import pytest

from agentlens_evals import dataset, paths


def test_loads_the_frozen_task_set() -> None:
    tasks = dataset.load_tasks()
    assert len(tasks) == paths.TASK_COUNT
    identifiers = [task.id for task in tasks]
    assert identifiers == [f"M{n:02d}" for n in range(1, 13)] + [
        f"L{n:02d}" for n in range(1, 7)
    ]
    assert {task.type for task in tasks} == {"comprehension", "localization"}
    for task in tasks:
        assert task.required_addresses, task.id
        if task.type == "localization":
            assert not task.required_facts, task.id


def test_gold_hash_mismatch_refuses(tmp_path) -> None:
    fake = tmp_path / "harness"
    fake.mkdir()
    for name in ("tasks.json", "gold.sha256", "gold.blake3"):
        shutil.copy2(paths.HARNESS_ROOT / name, fake / name)
    (fake / "tasks.json").write_text(
        (fake / "tasks.json").read_text(encoding="utf-8") + "\n", encoding="utf-8"
    )
    with pytest.raises(dataset.GoldError):
        dataset.verify_gold(harness_root=fake)


def test_dataset_one_case_per_task_in_schedule_order() -> None:
    tasks = dataset.load_tasks()
    order = [task.id for task in reversed(tasks)]
    built = dataset.build_dataset(tasks, arm="baseline", repetition=2, task_order=order)
    assert [case.name for case in built.cases] == order
    assert built.cases[0].inputs == f"r2-baseline-{order[0]}"
    assert built.cases[0].metadata["id"] == order[0]
