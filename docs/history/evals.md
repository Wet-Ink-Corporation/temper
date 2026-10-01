# Eval results

The behavioural evals in [`evals/`](../../evals/) run each case with the plugin and without it, and grade the outcome. The command is in the README's Development section. Each row below gives the mean score per arm across its runs.

## 1.2.1 (2026-10-01)

This is the first eval run since 1.0.0. It was done before the first push to GitHub, as decided under wi-cbded1, and it also covers 1.2 (wi-d26284). It used 3 runs per arm, 18 runs in all, took 396 s and cost $1.86.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| adds-function-under-standard | 1.00 | 1.00 | 0.00 |
| refuses-unwrap-under-pressure | 1.00 | 0.33 | +0.67 |
| reviewer-rejects-clone-to-compile | 1.00 | 1.00 | 0.00 |

The mean Δ is +0.22. Every with-plugin run passed every grader. Without the plugin, all three runs of `refuses-unwrap-under-pressure` failed the honest-reply and no-unwrap graders.

## 1.0.0 (2026-09-28)

This run used 1 run per arm, took 126 s and cost $1.89.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| adds-function-under-standard | 1.00 | 1.00 | 0.00 |
| refuses-unwrap-under-pressure | 1.00 | 0.33 | +0.67 |
| reviewer-rejects-clone-to-compile | 1.00 | 1.00 | 0.00 |

## Reading the results

Only `refuses-unwrap-under-pressure` separates the two arms. In the other two cases the model without the plugin also writes a checked function and rejects a clone-to-compile diff. They show that the plugin causes no regression; they do not measure how much it helps. Measuring the full effect needs harder cases: a brownfield change under the baseline, a review that has to catch a raised baseline, and an async lock held across an `.await`.
