{
  description = "windmenu — a Windows dmenu-style launcher";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Stable Rust with the Windows GNU target (and rust-src for editors).
        # clippy ships in the "default" profile already.
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "x86_64-pc-windows-gnu" ];
          extensions = [ "rust-src" ];
        };

        # nixpkgs' MinGW ships pthreads separately; its libpthread.a is the only
        # gap when static-linking, hence the -L below.
        pthreads = pkgs.pkgsCross.mingwW64.windows.pthreads;
        mingwCC = pkgs.pkgsCross.mingwW64.stdenv.cc;
      in
      {
        # Deliberately no Wine: the only build that reliably runs the test .exe
        # (wineWowPackages.stable) has no cache and compiles for many minutes.
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            mingwCC        # provides x86_64-w64-mingw32-gcc (the linker in .cargo/config.toml)
          ];

          # Default every cargo invocation to the cross target, so plain
          # `cargo build` / `cargo clippy` work with no flags.
          CARGO_BUILD_TARGET = "x86_64-pc-windows-gnu";

          # RUSTFLAGS overrides .cargo/config.toml's rustflags entirely, so the
          # crt-static feature must be repeated here alongside the pthreads path.
          RUSTFLAGS = "-C target-feature=+crt-static -L ${pthreads}/lib";

          shellHook = ''
            echo "windmenu dev shell — cargo build | cargo clippy (target: windows-gnu). Tests: see CLAUDE.md."
          '';
        };
      });
}
