#!/bin/sh
# scripts/release.sh <version>
#
# Sets the version in Cargo.toml and Cargo.lock, commits it together with the
# two changelogs, tags the commit `v<version>` and pushes both. The changelog
# sections `## [<version>]` must be written already; nothing else may be
# uncommitted.
set -eu

version="${1:-}"
if ! printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'; then
    echo "usage: scripts/release.sh <version>    (1.2.3 or 1.2.3-beta.1)" >&2
    exit 2
fi
cd "$(dirname "$0")/.."

fail() {
    echo "release: $1" >&2
    exit 1
}

[ "$(git symbolic-ref --short HEAD)" = main ] || fail "not on main"
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
    fail "tag v$version exists"
fi
dirty=$(git status --porcelain | grep -Ev '^ M CHANGELOG(\.zh)?\.md$' || true)
[ -z "$dirty" ] || fail "uncommitted changes besides the changelogs:
$dirty"
git fetch --quiet origin main
[ -z "$(git rev-list HEAD..FETCH_HEAD)" ] || fail "origin/main has commits this branch lacks"

for changelog in CHANGELOG.md CHANGELOG.zh.md; do
    body=$(awk -v heading="## [$version]" '
        index($0, heading) == 1 { found = 1; next }
        found && /^## \[/ { exit }
        found { print }
    ' "$changelog" | tr -d '[:space:]')
    [ -n "$body" ] || fail "$changelog has no section ## [$version]"
done

rewrite() {
    awk -v version="$version" "$2" "$1" > "$1.tmp"
    mv "$1.tmp" "$1"
}
rewrite Cargo.toml '
    !done && /^version = / { print "version = \"" version "\""; done = 1; next }
    { print }
'
rewrite Cargo.lock '
    /^name = / { ours = ($0 == "name = \"shellrs\"") }
    ours && /^version = / { print "version = \"" version "\""; ours = 0; next }
    { print }
'
git diff --quiet -- Cargo.toml && fail "Cargo.toml is already at $version"
git diff --quiet -- Cargo.lock && fail "Cargo.lock has no shellrs package"

case "$version" in
    *-*) title="发布 $version 测试版" ;;
    *) title="发布 $version 正式版" ;;
esac
git commit --quiet -m "$title" -- Cargo.toml Cargo.lock CHANGELOG.md CHANGELOG.zh.md
git tag "v$version"
git push --atomic origin main "v$version"
echo "release: pushed v$version"
