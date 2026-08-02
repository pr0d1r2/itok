{
  # itok's pinned toolchain. Lives at the repo ROOT: this is a standalone
  # crate headed for opensource, so it carries an ordinary root flake like
  # any Rust project -- a unit-of-work declaration belongs to the monorepo
  # home, not here (V31's two-tier split). Pinned to the same nixpkgs
  # revision as the monorepo, so the toolchain is identical whether itok is
  # built in-repo or standalone. Standard-only (V31): the rust toolchain
  # plus nextest and llvm-cov -- the cargo-native gate. No host guard bins,
  # because those do not travel.
  description = "itok -- context-cost estimator (dev shell)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/241313f4e8e508cb9b13278c2b0fa25b9ca27163";

  outputs =
    { nixpkgs, ... }:
    let
      # Same tier-1 systems as the host.
      systems = [
        "aarch64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});

      # itok dogfoods itself (V15) -- so the dev shell must PROVIDE `itok`,
      # not merely the toolchain to build it. Deliberately a SHIM and not a
      # nix package in the shell's closure: a package would have to build
      # the crate in order to ENTER the shell, so a compile error would
      # lock you out of the very shell you need to fix it. `cargo run` is a
      # no-op when the build is fresh and rebuilds only on change, so the
      # shim always matches the working tree, which a pinned package never
      # does. (A real `packages.default` for consumers is a separate
      # output and needs `Cargo.lock` committed -- it is not wired yet.)
      # ITOK_MANIFEST is exported by the shellHook; ITOK_PROFILE=release
      # is the escape hatch when a debug tiktoken (--bpe) is too slow on a
      # large tree.
      itokShim =
        pkgs:
        pkgs.writeShellScriptBin "itok" ''
          set -eu
          manifest="''${ITOK_MANIFEST:-}"
          if [ -z "$manifest" ] || [ ! -f "$manifest" ]; then
            echo "itok(shim): ITOK_MANIFEST unset or missing -- re-enter the dev shell from the crate root" >&2
            exit 2
          fi
          profile=""
          [ "''${ITOK_PROFILE:-debug}" = "release" ] && profile="--release"
          exec cargo run --quiet $profile \
            --manifest-path "$manifest" --bin itok \
            ''${ITOK_FEATURES:+--features "$ITOK_FEATURES"} -- "$@"
        '';
      # The FORMAT owner (V7 of cavespec): this spec's structural rules live
      # in one implementation, and itok CALLS it rather than porting it --
      # the ported copy is what drifted while both gates stayed green.
      #
      # A SHIM over a sibling checkout, not a flake input, because the two
      # crates are being polished together before a joint release: an input
      # would have to name a URL that does not exist yet, and naming a local
      # absolute path in a file headed for crates.io is the private
      # reference V39 forbids shipping. `../cavespec` is RELATIVE and the dev
      # shell is dev machinery, excluded from the packaged crate -- so this
      # travels no further than the working tree. At release it becomes an
      # ordinary `inputs.cavespec` pinned to a public rev, and nothing else
      # about the wiring changes: `hk.pkl` names the binary, never a path.
      #
      # Same shape as `itokShim` above, and for the same reason a package in
      # the shell's closure would be wrong: cavespec would have to COMPILE
      # before this shell could open, so one error over there locks you out
      # of the shell you need to fix it.
      cavespecShim =
        pkgs:
        pkgs.writeShellScriptBin "cavespec" ''
          set -eu
          manifest="''${CAVESPEC_MANIFEST:-}"
          if [ -z "$manifest" ] || [ ! -f "$manifest" ]; then
            echo "cavespec(shim): no sibling checkout -- expected ../cavespec/Cargo.toml beside this repo" >&2
            echo "cavespec(shim): clone it there, then re-enter the dev shell" >&2
            exit 2
          fi
          exec cargo run --quiet --manifest-path "$manifest" --bin cavespec -- "$@"
        '';
      # The reproducible build (V62). Possible only because the flake sits
      # at the repo root and can therefore see `Cargo.toml`/`src/`, and
      # because `Cargo.lock` is tracked -- flakes copy git-TRACKED files
      # into the store, and the build sandbox has no network, so
      # `cargoLock.lockFile` is what lets nix vendor exact crates offline.
      itokPkg =
        pkgs: features:
        pkgs.rustPlatform.buildRustPackage {
          pname = "itok";
          # Read from Cargo.toml so there is ONE version, not two that can
          # disagree (V64's rule, applied to a number).
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
          # Only what the BUILD reads. With `src = ./.` every tracked file
          # is an input, so editing SPEC.md or README rebuilt the crate
          # from scratch. Tests are excluded too -- doCheck is false, and
          # they need a `.git` the store does not have.
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./src
              ./Cargo.toml
              ./Cargo.lock
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          buildNoDefaultFeatures = features != null;
          buildFeatures = if features == null then [ ] else features;
          # The test suite shells out to git for HEAD~n (V33's `gitref`),
          # and a nix store source has no `.git` -- by design, that is what
          # makes the build reproducible. So the suite cannot run HERE; it
          # runs in the dev shell and in CI, where history exists (V37/B3
          # is the same constraint seen from the other side).
          doCheck = false;
          meta = {
            description = "context-cost estimator: token and window budgets for files and changes";
            homepage = "https://github.com/pr0d1r2/itok";
            license = pkgs.lib.licenses.mit;
            mainProgram = "itok";
          };
        };
    in
    {
      packages = forAll (pkgs: {
        # Default features: the dummy tier plus `--bpe` (V4).
        default = itokPkg pkgs null;
        # The zero-dependency core alone -- proves V23/V13 build clean.
        itok-minimal = itokPkg pkgs [ ];
        # Adds the LAN-exact rung (V22); still no TLS stack (V23).
        itok-ollama = itokPkg pkgs [
          "bpe"
          "ollama"
        ];
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (itokShim pkgs)
            (cavespecShim pkgs)
            pkgs.rustc
            pkgs.cargo
            pkgs.clippy
            pkgs.rustfmt
            pkgs.cargo-nextest
            pkgs.cargo-llvm-cov
            # llvm-cov / llvm-profdata come from the LLVM package, wired
            # via the env vars cargo-llvm-cov looks for.
            pkgs.llvmPackages.llvm
            pkgs.git # itok shells out to `git ls-files` for tracked files.
            # The gate runner (V64): `hk.pkl` holds the ops, hk runs them
            # locally on pre-commit/pre-push and in CI via `hk check`.
            # Pinned here so every contributor on nix gets the same hk as
            # the vendored pkl schema was cut from.
            pkgs.hk
            # Gate steps that need a real binary (V72). Pinned here so the
            # dev shell and CI run the same versions.
            pkgs.typos
            pkgs.actionlint
            pkgs.taplo
            pkgs.shellcheck
            # V72 again: `nixfmt --check` gates this very file.
            pkgs.nixfmt
          ];
          # Pin locale so tool output is deterministic across machines.
          LANG = "C.UTF-8";
          LLVM_COV = "${pkgs.llvmPackages.llvm}/bin/llvm-cov";
          LLVM_PROFDATA = "${pkgs.llvmPackages.llvm}/bin/llvm-profdata";
          # Resolved at ENTRY, not baked in: `.envrc` sits beside
          # `Cargo.toml`, and direnv enters with PWD = that directory --
          # in-repo (crates/itok/) and extracted (repo root) alike, so one
          # rule works in both layouts (V37: derive, never hardcode).
          shellHook = ''
            if [ -f "$PWD/Cargo.toml" ]; then
              export ITOK_MANIFEST="$PWD/Cargo.toml"
            else
              echo "itok(shell): no Cargo.toml in $PWD -- \`itok\` shim disabled" >&2
            fi
            # Derived the same way and for the same reason (V37): a sibling
            # of THIS directory, so it resolves in-repo and extracted alike.
            # Quiet when absent -- the gate step is where the failure has to
            # be loud, because that is where it costs something.
            if [ -f "$PWD/../cavespec/Cargo.toml" ]; then
              export CAVESPEC_MANIFEST="$PWD/../cavespec/Cargo.toml"
            fi
            # Entering the shell installs the hooks, so a contributor
            # cannot forget to (V71: a gate you must remember to enable is
            # not a gate). Silent and idempotent -- Unix philosophy.
            #
            # Written by hand rather than via `hk install`, because the
            # command that writes assumes hk is on PATH. It is not,
            # outside this shell -- and a hook that hard-fails when its
            # runner is missing does not gate a commit, it blocks EVERY
            # git command in the repo (B6). So the command degrades: no
            # hk, no gate, one line to stderr saying so. Loud, not silent.
            #
            # SKIPPED when a global install exists: git aggregates
            # `hook.<name>.command` across scopes, so having both fires
            # every hook twice. The global install is the better setup, so
            # it wins when present.
            if ! git config --global --get-regexp '^hook\.hk-' >/dev/null 2>&1; then
              for ev in pre-commit pre-push; do
                git config --local "hook.hk-$ev.event" "$ev"
                git config --local "hook.hk-$ev.command" \
                  "command -v hk >/dev/null 2>&1 || { echo 'hk not found -- gate skipped; enter the dev shell (direnv/nix develop) to run it' >&2; exit 0; }; test \"\''${HK:-1}\" = \"0\" || hk run $ev --from-hook \"\$@\""
              done
            fi
          '';
        };
      });
    };
}
