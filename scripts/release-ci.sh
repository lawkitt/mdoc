# CI eligibility for release.sh. Sourced, never executed; bash 3.2 compatible.
# shellcheck shell=bash
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
  # ci.yml skips the release commit on main: the tag's Release run tests it
  # (it calls ci.yml). Accept that run instead, for the tag on this commit.
  if [ "$status" = completed ] && [ "$conclusion" = skipped ] && [ -z "$extra" ] && \
    [[ $run != *$'\n'* ]]; then
    release_ci_tag_green "$sha"
    return
  fi
  if [ "$status" != completed ] || [ "$conclusion" != success ] || \
    [ "$head" != "$sha" ] || [ "$event" != push ] || [ "$workflow" != CI ] || \
    [ -z "$url" ] || [ -n "$extra" ] || [[ $run == *$'\n'* ]]; then
    echo "local checks required: CI for $sha is not clearly green (status=$status, conclusion=$conclusion)"
    return 1
  fi
  echo "skipping local checks: CI is green for $sha ($url)"
}

# The release commit $1 passes if a tag `v<version>` from its subject points at
# it and that tag's Release push run completed successfully.
release_ci_tag_green() {
  local sha=$1 subject tag run status conclusion url head event workflow branch extra
  subject=$(git log -1 --format=%s "$sha")
  case "$subject" in
    'release: v'*) tag=${subject#release: } ;;
    *)
      echo "local checks required: CI for $sha was skipped"
      return 1
      ;;
  esac
  if [ "$(git rev-parse -q --verify "refs/tags/$tag^{commit}" 2>/dev/null)" != "$sha" ]; then
    echo "local checks required: CI for $sha was skipped and tag $tag does not point at it"
    return 1
  fi
  if ! run=$(gh run list --workflow release.yml --commit "$sha" --branch "$tag" \
    --event push --limit 1 --json status,conclusion,url,headSha,event,workflowName,headBranch \
    --template '{{range .}}{{.status}}|{{.conclusion}}|{{.url}}|{{.headSha}}|{{.event}}|{{.workflowName}}|{{.headBranch}}{{"\n"}}{{end}}'); then
    echo "local checks required: gh could not query the Release run for $tag (check authentication and connectivity)"
    return 1
  fi
  if [ -z "$run" ]; then
    echo "local checks required: no Release run for $tag"
    return 1
  fi
  IFS='|' read -r status conclusion url head event workflow branch extra <<< "$run"
  if [ "$status" != completed ] || [ "$conclusion" != success ] || \
    [ "$head" != "$sha" ] || [ "$event" != push ] || [ "$workflow" != Release ] || \
    [ "$branch" != "$tag" ] || [ -z "$url" ] || [ -n "$extra" ] || [[ $run == *$'\n'* ]]; then
    echo "local checks required: the Release run for $tag is not clearly green (status=$status, conclusion=$conclusion)"
    return 1
  fi
  echo "skipping local checks: $sha is release commit $tag, and its Release run is green ($url)"
}
