#!/bin/sh
# Generate the README badge block from the files that own each number.
#
# ONE script, TWO modes, because `hk.pkl`'s `check` and `fix` would otherwise
# be two copies of the same generator -- which is exactly the duplication V7
# is about, and the shape that let a ported spec checker drift out of sync
# before microlith took the format over.
#
#   --check   render, diff against README, fail loudly if stale
#   --write   render and splice into README between the markers
#
# Lives in a file rather than inline in `hk.pkl` for a second reason: Pkl
# reads a backslash inside a multi-line string as an escape, and `\(` as
# interpolation, so any real sed or awk gets mangled on the way through. Here
# it is ordinary shell that `shellcheck` can lint.
set -eu

usage() {
	echo "usage: badges.sh --check | --write" >&2
	exit 2
}

[ $# -eq 1 ] || usage
case "$1" in
--check | --write) mode="$1" ;;
*) usage ;;
esac

[ -f .coverage ] || {
	echo "badges: .coverage is missing -- run \`hk run refresh\` to measure and cache it." >&2
	exit 1
}

# Each value comes from the file that OWNS it, never from a second copy.
ed=$(sed -n 's/^edition = //p' Cargo.toml | tr -d '"')
msrv=$(sed -n 's/^rust-version = //p' Cargo.toml | tr -d '"')
floor=$(grep -oE 'fail-under-lines [0-9]+' hk.pkl | head -1 | grep -oE '[0-9]+')
cov=$(awk '/^lines /{print $2}' .coverage)
# The PINNED REV, not a release number. This flake pins nixpkgs by rev, so
# there is no `26.11` anywhere in the lock to read -- microlith's badge
# hardcodes that string inside its generated block, which is precisely the
# hand-maintained number a generator exists to remove. The rev is what the
# build actually uses, and it is in the file.
#
# Anchored to the `nixpkgs` node on purpose: `flake.lock` also carries the
# `microlith` input, and taking the first `rev` in the file would badge
# whichever node happens to sort first.
nixpkgs=$(awk '/"nixpkgs": [{]/{f=1} f&&/"rev":/{gsub(/[",]/,"");print $2;exit}' flake.lock | cut -c1-7)

# Direct dependencies. `!/^#/` matters: without it a comment inside the
# section is counted as a dependency, which is what microlith's version does
# -- invisible there because the count is zero.
deps=$(awk '/^[[]dependencies[]]/{f=1;next} /^[[]/{f=0} f&&NF&&!/^#/' Cargo.toml | wc -l | tr -d ' ')

# One badge per platform the flake actually builds, so adding a system to
# `flake.nix` adds its badge and nothing has to remember to.
plat=$(sed -n '/systems = [[]/,/[]];/p' flake.nix |
	grep -oE '"[a-z0-9_-]+"' | tr -d '"' |
	while read -r t; do
		o=${t##*-}
		[ "$o" = darwin ] && o=macos
		case ${t%-*} in
		x86_64) echo "1 intel $o" && echo "2 amd $o" ;;
		aarch64) echo "3 arm $o" ;;
		*) echo "9 ${t%-*} $o" ;;
		esac
	done | sort |
	while read -r _ v o; do
		echo "[![$v $o](https://img.shields.io/badge/$o-5277C3?logo=$v&logoColor=white)](flake.nix)"
	done)

R=https://img.shields.io/badge
mkdir -p target

cat >target/badges.txt <<EOF
[![CI](https://github.com/pr0d1r2/itok/actions/workflows/ci.yml/badge.svg)](https://github.com/pr0d1r2/itok/actions/workflows/ci.yml)
[![License: MIT]($R/license-MIT-blue.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/itok.svg)](https://crates.io/crates/itok)
[![docs.rs](https://docs.rs/itok/badge.svg)](https://docs.rs/itok)
[![edition $ed]($R/edition-$ed-000000?logo=rust&logoColor=white)](Cargo.toml)
[![MSRV $msrv]($R/MSRV-$msrv-000000?logo=rust&logoColor=white)](Cargo.toml)
[![direct dependencies $deps]($R/direct_dependencies-$deps-brightgreen)](docs/THIRD-PARTY-NOTICES.md)
[![minimal tier 0 dependencies]($R/minimal_tier-0_dependencies-brightgreen)](docs/THIRD-PARTY-NOTICES.md)
[![unsafe forbidden]($R/unsafe-forbidden-brightgreen)](Cargo.toml)
[![gate hk]($R/gate-hk-6E4AFF)](hk.pkl)
[![coverage $cov%]($R/coverage-$cov%25-brightgreen)](hk.pkl)
[![floor $floor%]($R/floor-%E2%89%A5$floor%25-brightgreen)](hk.pkl)

[![nix flake]($R/nix-flake-5277C3?logo=nixos&logoColor=white)](flake.nix)
[![nixpkgs $nixpkgs]($R/nixpkgs-$nixpkgs-5277C3?logo=nixos&logoColor=white)](flake.lock)
$plat

[![built with Claude Code]($R/built_with-Claude_Code-D97757)](https://claude.com/claude-code)
[![built with Opus 5]($R/built_with-Opus_5-D97757)](https://www.anthropic.com/claude)
[![built with SDD]($R/built_with-spec--driven_development-D97757)](SPEC.md)
EOF

if [ "$mode" = --check ]; then
	awk '/^<!-- BEGIN badges -->$/{f=1;next} /^<!-- END badges -->$/{f=0} f' \
		README.md >target/badges-current.txt
	diff -q target/badges-current.txt target/badges.txt >/dev/null && exit 0
	# shellcheck disable=SC2016  # the backticks are markdown in a message, not substitution
	echo 'hk: the README badge block is stale. Run `hk fix` (or `hk run refresh`) to regenerate it from Cargo.toml, flake.nix, flake.lock, hk.pkl and .coverage.' >&2
	diff target/badges-current.txt target/badges.txt >&2 || true
	exit 1
fi

awk 'BEGIN{while((getline l < "target/badges.txt")>0) b=b l "\n"}
     /^<!-- BEGIN badges -->$/{print; printf "%s", b; skip=1; next}
     /^<!-- END badges -->$/{skip=0}
     !skip' README.md >target/README.new
mv target/README.new README.md
