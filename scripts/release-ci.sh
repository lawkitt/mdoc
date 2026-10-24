# CI eligibility for release.sh. Sourced, never executed; bash 3.2 compatible.
# Failure is deliberately conservative: the caller runs local checks.

release_ci_green() {
  local patterns pattern sha run status conclusion url head event workflow extra
  local excludes=()

  if ! type -P gh >/dev/null; then
    echo "local checks required: gh is unavailable"
    return 1
  fi
  if [ "$(git rev-parse --is-shallow-repository)" != false ]; then
    echo "local checks required: main's complete history is unavailable"
    return 1
  fi

  # Read the push paths-ignore block, including its anchor, from ci.yml.
  # Accept only the simple quoted glob list used there. YAML format changes
  # or unsupported patterns fail closed rather than broaden the skip.
  if ! patterns=$(awk '
    /^  push:$/ { push = 1; next }
    push && /^    paths-ignore: &ignored$/ { grab = 1; next }
    grab && /^      - "[A-Za-z0-9_.\/*-]+"$/ {
      sub(/^      - "/, ""); sub(/"$/, ""); print; count++; next
    }
    grab && /^      / { bad = 1; exit }
    grab { exit }
    push && /^  [^ ]/ { exit }
    END { if (!count || bad) exit 1 }
  ' .github/workflows/ci.yml); then
    echo "local checks required: cannot read push paths-ignore from ci.yml"
    return 1
  fi
  while IFS= read -r pattern; do
    excludes+=(":(exclude,glob)$pattern")
  done <<< "$patterns"

  # Glob pathspecs make **/*.md include root Markdown too. First-parent
  # diffs include a merge's net changes against main, including resolutions.
  if ! sha=$(git log --first-parent --diff-merges=first-parent --no-patch -1 --format=%H \
    origin/main -- . "${excludes[@]}") || [ -z "$sha" ]; then
    echo "local checks required: cannot identify main's latest CI-relevant commit"
    return 1
  fi
  if ! run=$(gh run list --workflow ci.yml --commit "$sha" --branch main \
    --event push --limit 1 --json status,conclusion,url,headSha,event,workflowName \
    --template '{{range .}}{{.status}}|{{.conclusion}}|{{.url}}|{{.headSha}}|{{.event}}|{{.workflowName}}{{"\n"}}{{end}}'); then
    echo "local checks required: gh could not query CI for $sha (check authentication and connectivity)"
    return 1
  fi
  if [ -z "$run" ]; then
    echo "local checks required: no CI push run for $sha"
    return 1
  fi
  IFS='|' read -r status conclusion url head event workflow extra <<< "$run"
  if [ "$status" != completed ] || [ "$conclusion" != success ] || \
    [ "$head" != "$sha" ] || [ "$event" != push ] || [ "$workflow" != CI ] || \
    [ -z "$url" ] || [ -n "$extra" ] || [[ $run == *$'\n'* ]]; then
    echo "local checks required: CI for $sha is not clearly green (status=$status, conclusion=$conclusion)"
    return 1
  fi
  echo "skipping local checks: CI is green for $sha ($url)"
}
