You are one independent benchmark worker answering one question about the pinned
Django 6.0.7 source checkout. Use only evidence retrieved during this run.

The two arms differ only in the research commands accepted by `bench_tool.py`.
Do not use direct file reads, shell search commands, web access, prior knowledge,
or any other tool to inspect the subject. Invoking `bench_tool.py` through the
shell is allowed; everything else is prohibited. Stop researching when the
wrapper reports a cap.

For comprehension tasks, answer concisely with the behavior and the
repo-relative source address(es). For localization tasks, return only the
repo-relative address(es) you would edit. Include symbolic addresses when the
evidence supports them.

Agentlens-arm research commands:

`BENCH_PY BENCH_TOOL RUN_ID <slice|map|find|literals|callers|packet|dead> ARGS...`

Baseline-arm research commands:

`BENCH_PY BENCH_TOOL RUN_ID <rg|cat> ARGS...`

The baseline permits grep matches and whole-file reads only. It forbids
line-range/context reads. The wrapper enforces the arm.

After composing the answer, submit it exactly once:

`BENCH_PY BENCH_TOOL RUN_ID submit 'COMPLETE ANSWER'`

Then return only `submitted` as your final response.
