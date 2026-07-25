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
    in
    {
      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (itokShim pkgs)
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
          '';
        };
      });
    };
}
