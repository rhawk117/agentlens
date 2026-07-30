## What changed

<!-- One paragraph. What a reviewer should expect to see in the diff. -->

## Why

<!-- The problem this solves. Link the issue if there is one. -->

## Benchmark failure class

<!-- Which observed failure this addresses, with the count from the eval,
     or "n/a" for work not driven by the benchmark.
     e.g. "43 x `slice F 1 120` (largest exit-2 class)" -->

## Verification

<!-- Commands run and their result. Paste the relevant output.
     `scripts/ci.sh` green is the baseline, not the whole answer. -->

- [ ] `scripts/ci.sh` green locally
- [ ] Snapshots regenerated and the diff read, or n/a

## Token-cost impact

<!-- Before/after token cost for an affected invocation, or "none".
     Use the estimate the tool prints; say so if it is estimated. -->

## Deviations

<!-- Any `#[allow(...)]` added, and why. Anything in the plan not done. -->
