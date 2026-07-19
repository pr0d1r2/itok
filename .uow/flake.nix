{
  # itok's detachable toolchain (V28). Pinned to the same nixpkgs revision
  # as the monorepo, so the toolchain is identical whether itok is built
  # in-repo or after a `git subtree split`. Standard-only (V31): the rust
  # toolchain plus nextest and llvm-cov -- the cargo-native gate. No host
  # guard bins, because those do not travel.
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
    in
    {
      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = [
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
        };
      });
    };
}
