#!/usr/bin/env bash
#
# ci-gate-check — fail when a release gate is weakened.
#
# Usage: ./script/ci-gate-check.sh .github/workflows/vorpal.yaml
#
# Releases ship only after both gate jobs succeed: test-artifact-secrets, and
# release-gate-check, which runs this script. Each of these would quietly
# drop that guarantee for a gate job, so each one fails the check:
#   - the gate job is missing;
#   - it sets continue-on-error at the job level or on any step (any value,
#     since an expression can evaluate to true);
#   - it has a job-level if:, which can skip it;
#   - release, release-container-image or release-website-docs is missing,
#     or lacks the gate job in its needs;
#   - one of those release jobs has an if: that calls a status function
#     (always(), failure(), cancelled(), matched case-insensitively), which
#     can run the job even when a gate failed, or an if: that is not a string.
#
# Not checked:
#   - `|| true` in a gate job's script, a step-level if: on a gate job's
#     steps, and whether release-gate-check's steps still run this script;
#     all live in the jobs' step text, which this check does not pin;
#   - success() in a release job's if:, which can also run the job after a
#     failed gate when negated or combined with ||, such as !success();
#   - jobs outside the three release jobs, such as
#     release-container-image-manifest or a newly added release job;
#   - the shape of a gate job: a null or scalar job, missing or empty steps,
#     or a reusable-workflow uses: job passes.
#
# The workflow is read with mikefarah yq v4 after explode(.), so anchors and
# aliases resolve the way GitHub sees them; merge keys follow yq's semantics,
# which may differ from GitHub's. needs is matched in scalar, block and flow
# form. A yq error or a missing or non-map jobs fails closed.

set -uo pipefail

gate_jobs=(test-artifact-secrets release-gate-check)
release_jobs=(release release-container-image release-website-docs)

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <workflow.yaml>" >&2
  exit 2
fi
workflow="$1"

# Evaluates a yq expression against the workflow with anchors, aliases and
# merge keys resolved. Expressions are constants of this script; a looped job
# name reaches yq through the JOB and GATE environment variables, never the
# expression.
query() {
  yq eval "explode(.) | $1" "$workflow"
}

jobs_tag="$(query '.jobs | tag')"
if [ "$jobs_tag" != "!!map" ]; then
  echo "ci-gate-check FAILED: cannot read a jobs map from $workflow; release gating is unverified." >&2
  exit 1
fi

reasons=()

for gate in "${gate_jobs[@]}"; do
  if [ "$(JOB="$gate" query '.jobs | has(strenv(JOB))')" != true ]; then
    reasons+=("job $gate is missing")
    continue
  fi
  [ "$(JOB="$gate" query '.jobs[strenv(JOB)] | has("continue-on-error")')" = false ] ||
    reasons+=("job $gate sets continue-on-error at the job level")
  [ "$(JOB="$gate" query '[.jobs[strenv(JOB)].steps[] | select(has("continue-on-error"))] | length')" = 0 ] ||
    reasons+=("job $gate sets continue-on-error on a step")
  [ "$(JOB="$gate" query '.jobs[strenv(JOB)] | has("if")')" = false ] ||
    reasons+=("job $gate has a job-level if:")
done

for job in "${release_jobs[@]}"; do
  if [ "$(JOB="$job" query '.jobs | has(strenv(JOB))')" != true ]; then
    reasons+=("release job $job is missing")
    continue
  fi
  for gate in "${gate_jobs[@]}"; do
    [ "$(JOB="$job" GATE="$gate" query '[.jobs[strenv(JOB)].needs | .. | select(tag == "!!str" and . == strenv(GATE))] | length > 0')" = true ] ||
      reasons+=("release job $job does not list $gate in needs")
  done
  if [ "$(JOB="$job" query '.jobs[strenv(JOB)] | has("if")')" != false ]; then
    if [ "$(JOB="$job" query '.jobs[strenv(JOB)].if | tag')" != "!!str" ]; then
      reasons+=("release job $job has a non-string if:")
    else
      [ "$(JOB="$job" query '.jobs[strenv(JOB)].if | test("(?i)(always|failure|cancelled)[[:space:]]*[(]")')" = false ] ||
        reasons+=("release job $job calls a status function in its if:, so it can run after a failed gate")
    fi
  fi
done

if [ "${#reasons[@]}" -gt 0 ]; then
  echo "ci-gate-check FAILED: releases are no longer gated on ${gate_jobs[*]}:" >&2
  for r in "${reasons[@]}"; do
    echo "  - $r" >&2
  done
  exit 1
fi

echo "ci-gate-check: ok (gates ${gate_jobs[*]} block ${release_jobs[*]})"
