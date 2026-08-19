#!/usr/bin/env bash
# deletes the latest tag and its GitHub release, then re-tags and pushes HEAD to retrigger CI
set -euo pipefail

remote="origin"

latest_tag=$(git tag --sort=-v:refname | head -n1)

if [[ -z "$latest_tag" ]]; then
    echo "error: no tags to redo" >&2
    exit 1
fi

head_sha=$(git rev-parse --short HEAD)
head_subject=$(git log -1 --format=%s)

echo "latest tag: $latest_tag"
echo "HEAD: $head_sha $head_subject"

read -rp "delete $latest_tag (and its GitHub release) and re-push it on HEAD? [y/N] " confirm
if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
    echo "aborted"
    exit 1
fi

if gh release view "$latest_tag" >/dev/null 2>&1; then
    gh release delete "$latest_tag" -y
fi

git tag -d "$latest_tag" 2>/dev/null || true
git push "$remote" ":refs/tags/$latest_tag" 2>/dev/null || true

git tag -a "$latest_tag" -m "$latest_tag"
git push "$remote" "$latest_tag"

echo "re-pushed $latest_tag on $head_sha"
