#!/usr/bin/env bash
# creates and pushes a new release tag, after validating it against the latest one
set -euo pipefail

remote="origin"
tag_pattern='^v[0-9]+\.[0-9]+\.[0-9]+$'

latest_tag=$(git tag --sort=-v:refname | head -n1)

if [[ -n "$latest_tag" ]]; then
    echo "latest tag: $latest_tag"
else
    echo "no tags yet"
fi

read -rp "new tag: " new_tag

if [[ ! "$new_tag" =~ $tag_pattern ]]; then
    echo "error: tag must look like vX.Y.Z" >&2
    exit 1
fi

if git rev-parse -q --verify "refs/tags/$new_tag" >/dev/null; then
    echo "error: tag $new_tag already exists" >&2
    exit 1
fi

if [[ -n "$latest_tag" ]]; then
    highest=$(printf '%s\n%s\n' "$latest_tag" "$new_tag" | sort -V | tail -n1)
    if [[ "$highest" != "$new_tag" ]]; then
        echo "error: $new_tag is not greater than latest tag $latest_tag" >&2
        exit 1
    fi
fi

read -rp "create $new_tag? [y/N] " confirm
if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
    echo "aborted"
    exit 1
fi

git tag -a "$new_tag" -m "$new_tag"
git push "$remote" "$new_tag"

echo "pushed $new_tag"
