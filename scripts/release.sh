#!/usr/bin/env bash
# Publish only the main commit that passed every required CI job.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${GITHUB_SHA:?}" "${GITHUB_EVENT_NAME:?}" "${GH_REPO:?}" "${GH_TOKEN:?}"
git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
git fetch origin main --tags
remote=$(git rev-parse origin/main)
if [[ "$remote" == "$GITHUB_SHA" ]]; then
  # Weekly, or at once for a security fix; see due() in release.py.
  if ! python3 scripts/release.py --due "$GITHUB_EVENT_NAME"; then
    echo "${GITHUB_EVENT_NAME}: no release due (nothing to release, or not due before the weekly run)."
    exit 0
  fi
  release=$(python3 scripts/release.py)
  version=${release%%$'\n'*}
  codename=${release#*$'\n'}
  # Homebrew and makepkg build from the tag, which lands with this commit, so
  # the formula and PKGBUILD go in it too: a release pushes to main exactly once.
  python3 scripts/release.py --formula "$version"
  python3 scripts/release.py --pkgbuild "$version"
  git add Cargo.toml Cargo.lock CHANGELOG.md Formula/poopterm.rb packaging/arch/PKGBUILD
  # Pushed with the deploy key, which triggers workflows: [skip ci] stops
  # this commit from releasing again.
  git commit -m "chore(release): ${version} after green CI" -m "[skip ci]" -m "Poopterm-Release-For: ${GITHUB_SHA}"
  git tag "v${version}"
  # Atomic publication plus a lease closes the race between the fetch and push.
  # This is a fast-forward; the lease also rejects a newer main in the meantime.
  if ! git push --atomic --force-with-lease="refs/heads/main:${GITHUB_SHA}" origin HEAD:main "refs/tags/v${version}"; then
    # Losing the race to a merge is a skip: that commit's own run releases.
    git fetch origin main
    if [[ "$(git rev-parse origin/main)" != "$GITHUB_SHA" ]]; then
      echo 'main moved while releasing; skipping release.'
      exit 0
    fi
    exit 1
  fi
elif [[ "$(git log -1 --format=%B origin/main)" == *"Poopterm-Release-For: ${GITHUB_SHA}"* ]]; then
  # A rerun after a publishing failure resumes this release, without another bump.
  git checkout --detach origin/main
  version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
  codename=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["metadata"]["poopterm"]["codename"])')
  [[ "$(git rev-parse "v${version}")" == "$(git rev-parse HEAD)" ]]
else
  echo 'main moved since this CI run; skipping release.'
  exit 0
fi
# Look the release up first, drafts included (the tag endpoint omits them);
# API failures must stop publication. State is none, draft or published,
# then whether this version's Debian and RPM packages are attached.
gh api "repos/${GH_REPO}/releases?per_page=100" > /tmp/poopterm-releases.json
read -r state deb rpm < <(python3 - "v${version}" "poopterm_${version}-1_" "poopterm-${version}-1." <<'PY'
import json, sys
tag, *packages = sys.argv[1:]
release = next((r for r in json.load(open('/tmp/poopterm-releases.json')) if r['tag_name'] == tag), None)
names = [a['name'] for a in release['assets']] if release else []
state = 'none' if release is None else 'draft' if release['draft'] else 'published'
print(state, *('yes' if any(n.startswith(p) for n in names) else 'no' for p in packages))
PY
)
# A rerun of a finished release has nothing left to do.
if [[ $state == published && $deb == yes && $rpm == yes ]]; then
  echo "v${version} is already published."
  exit 0
fi
if [[ $deb == no || $rpm == no ]]; then
  # Package the release commit itself, so the packages and the binary in them carry
  # the new version; a failed build stops before the GitHub release exists.
  # Releases ship the PGO build, bolted on Linux when llvm-bolt is
  # installed, or a plain one if training fails.
  bash scripts/pgo.sh dist
  bash scripts/package-deb.sh
  bash scripts/package-rpm.sh
  shopt -s nullglob
  debs=(dist/poopterm_"${version}"-1_*.deb)
  rpms=(dist/poopterm-"${version}"-1.*.rpm)
  if ((${#debs[@]} != 1 || ${#rpms[@]} != 1)); then
    echo "expected exactly one Debian and one RPM package of ${version} in dist/, found ${#debs[@]} and ${#rpms[@]}" >&2
    exit 1
  fi
fi
# Releases start as drafts and are published once the package is attached,
# since immutable releases refuse new assets after publication.
if [[ $state == none ]]; then
  python3 -c 'from pathlib import Path; s=Path("CHANGELOG.md").read_text(); print("## " + s.split("\n## ", 2)[1])' > /tmp/poopterm-release-notes.md
  gh release create "v${version}" --draft --verify-tag --title "Poopterm ${version} \"${codename}\"" --notes-file /tmp/poopterm-release-notes.md
fi
# The APT feed polls this release asset and republishes it from GitHub Pages.
if [[ $deb == no ]]; then
  gh release upload "v${version}" "${debs[0]}" --clobber
fi
if [[ $rpm == no ]]; then
  gh release upload "v${version}" "${rpms[0]}" --clobber
fi
if [[ $state != published ]]; then
  gh release edit "v${version}" --draft=false
fi
