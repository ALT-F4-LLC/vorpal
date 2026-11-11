#!/usr/bin/env bash
#
# ci-gate-check — fail when the test-secret-argv release gate is weakened.
#
# Usage: ./script/ci-gate-check.sh .github/workflows/vorpal.yaml
#
# Releases ship only after test-secret-argv succeeds. Each of these would
# quietly drop that guarantee, so each one fails the check:
#   - the test-secret-argv job is missing;
#   - it sets continue-on-error at the job level or on any step (any value,
#     since an expression can evaluate to true);
#   - it has a job-level if:, which can skip it;
#   - release, release-container-image or release-website-docs is missing,
#     or lacks test-secret-argv in its needs.
#
# Not checked: `|| true` in the job's script and a step-level if: on its
# steps. Both live in the job's script text, which this check does not pin.
#
# The workflow is read with mikefarah yq v4 after explode(.), so anchors,
# aliases and merge keys resolve the way GitHub sees them. needs is matched in
# scalar, block and flow form. Any yq error or unexpected result fails closed.

set -uo pipefail

gate_job="test-secret-argv"
release_jobs=(release release-container-image release-website-docs)

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <workflow.yaml>" >&2
  exit 2
fi
workflow="$1"

# Evaluates a yq expression against the workflow with anchors, aliases and
# merge keys resolved. Expressions are constants of this script; a looped job
# name reaches yq through the JOB environment variable, never the expression.
query() {
  yq eval "explode(.) | $1" "$workflow"
}

jobs_tag="$(query '.jobs | tag')"
if [ "$jobs_tag" != "!!map" ]; then
  echo "ci-gate-check FAILED: cannot read a jobs map from $workflow; $gate_job gating is unverified." >&2
  exit 1
fi

reasons=()

if [ "$(query '.jobs | has("test-secret-argv")')" != true ]; then
  reasons+=("job $gate_job is missing")
else
  [ "$(query '.jobs."test-secret-argv" | has("continue-on-error")')" = false ] ||
    reasons+=("job $gate_job sets continue-on-error at the job level")
  [ "$(query '[.jobs."test-secret-argv".steps[] | select(has("continue-on-error"))] | length')" = 0 ] ||
    reasons+=("job $gate_job sets continue-on-error on a step")
  [ "$(query '.jobs."test-secret-argv" | has("if")')" = false ] ||
    reasons+=("job $gate_job has a job-level if:")
fi

for job in "${release_jobs[@]}"; do
  if [ "$(JOB="$job" query '.jobs | has(strenv(JOB))')" != true ]; then
    reasons+=("release job $job is missing")
    continue
  fi
  [ "$(JOB="$job" query '[.jobs[strenv(JOB)].needs | .. | select(tag == "!!str" and . == "test-secret-argv")] | length > 0')" = true ] ||
    reasons+=("release job $job does not list $gate_job in needs")
done

if [ "${#reasons[@]}" -gt 0 ]; then
  echo "ci-gate-check FAILED: releases are no longer gated on $gate_job:" >&2
  for r in "${reasons[@]}"; do
    echo "  - $r" >&2
  done
  exit 1
fi

echo "ci-gate-check: ok ($gate_job gates ${release_jobs[*]})"
