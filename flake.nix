{
  # itok's pinned toolchain. Lives at the repo ROOT: this is a standalone
  # crate headed for opensource, so it carries an ordinary root flake like
  # any Rust project. The gate it serves is `hk.pkl`, run by hk (V64) --
  # one definition, reached the same way from a laptop and from CI.
  # Standard-only (V31): the rust toolchain plus nextest and llvm-cov --
  # the cargo-native gate. No host guard bins, because those do not travel.
  description = "itok -- context-cost estimator (dev shell)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/241313f4e8e508cb9b13278c2b0fa25b9ca27163";

  # The FORMAT owner, pinned to a released TAG rather than a branch: the
  # rules this spec is held to should change when the pin changes and at no
  # other time. `follows` because microlith pins its own nixpkgs, and two
  # nixpkgs in one closure is two toolchains to download for no benefit.
  inputs.microlith.url = "github:pr0d1r2/microlith/v0.5.0";
  inputs.microlith.inputs.nixpkgs.follows = "nixpkgs";

  outputs =
    { nixpkgs, microlith, ... }:
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
      # The FORMAT owner (V7 of microlith): this spec's structural rules
      # live in one implementation, and itok CALLS it rather than porting
      # it -- the ported copy is what drifted while both gates stayed green.
      # The PACKAGE is `microlith`; the BINARY it installs is `mth`. Those
      # are two names on purpose, so keep them apart: paths and manifests
      # take `microlith`, anything invoked takes `mth`.
      #
      # This WAS a shim over a sibling `../microlith` checkout, because
      # before the release there was no public URL to name and an absolute
      # local path is the private reference V39 forbids shipping. There is
      # a URL now, so the pin is an ordinary flake input and the sibling
      # stops being required: a fresh public clone can enter this shell and
      # commit, which is the whole point -- the `mth` gate step hard-fails
      # when the binary is absent (nothing else checks SPEC.md structure,
      # so degrading would read as a pass). `hk.pkl` did not change, and
      # could not have: it names the binary, never a path.
      #
      # The wrapper exists for the HATCH, not the default. Default is the
      # pinned build, so the rules are whatever the tag says. Setting
      # MICROLITH_MANIFEST to a sibling checkout runs THAT instead, which
      # is how a format change gets tried against a real spec before it is
      # tagged. Deliberately opt-in: an auto-detected sibling would silently
      # outrank the pin whenever the directory happened to exist, and then
      # the gate's verdict would depend on the layout of the machine.
      mthWrapper =
        pkgs:
        let
          pinned = microlith.packages.${pkgs.stdenv.hostPlatform.system}.default;
        in
        pkgs.writeShellScriptBin "mth" ''
          set -eu
          manifest="''${MICROLITH_MANIFEST:-}"
          if [ -n "$manifest" ]; then
            if [ ! -f "$manifest" ]; then
              echo "mth: MICROLITH_MANIFEST=$manifest does not exist -- unset it to use the pinned ${pinned.version}" >&2
              exit 2
            fi
            exec cargo run --quiet --manifest-path "$manifest" --bin mth -- "$@"
          fi
          exec ${pinned}/bin/mth "$@"
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
        # Adds the LAN-exact rung (V22), with TLS (V23/V25).
        itok-ollama = itokPkg pkgs [
          "bpe"
          "ollama"
        ];
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (itokShim pkgs)
            (mthWrapper pkgs)
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
            # Relative-link resolution across the docs set. Offline only --
            # it never touches the network, so it cannot fail on someone
            # else's 404.
            pkgs.lychee
            # Secret shapes `no-private-key` does not match. Here rather
            # than nowhere because a leaked token in a PUBLIC history is
            # irreversible, and this repo is headed for one.
            pkgs.ripsecrets
            # The version ladder (V70), checked instead of remembered.
            # Vacuous until the first `v*` tag, and it says so.
            pkgs.cargo-semver-checks
          ];
          # Pin locale so tool output is deterministic across machines.
          LANG = "C.UTF-8";
          # Opts a bare `cargo test` in the dev shell in to the dogfood
          # tests, matching hk. Without it a contributor's local run silently
          # skips fourteen tests that CI runs, and "green here, red in the
          # gate" is the confusion this repo spends real effort avoiding.
          # See src/testutil.rs for why the gate is an env var.
          ITOK_DOGFOOD = "1";
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
            # MICROLITH_MANIFEST is deliberately NOT exported here. `mth`
            # comes from the pinned input, and the sibling checkout is an
            # override a person chooses, not one the filesystem chooses for
            # them -- see the wrapper above.
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
