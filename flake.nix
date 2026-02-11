{
  description = "Development shell for portable-pty wrapper with Bun + Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system;
          overlays = overlays;
        };

        # Extend the default Rust profile by adding extensions (override appends to defaults)
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src" # for rust-analyzer / IDE support
            "rust-analyzer" # LSP
            # clippy and rustfmt are already included in the default profile
            # add more here if needed
          ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          # Use `packages` for tools/executables — modern & cleaner
          packages = with pkgs; [
            rustToolchain # includes cargo, rustc, rustfmt, clippy, rust-analyzer, ...
            pkg-config # helpful for FFI / native lib discovery
            bashInteractive # for a better shell experience
          ];

          # Optional: extra environment variables if needed
          shellHook = ''
            echo "Rust dev environment loaded"
            echo "Rust version: $(rustc --version)"
            echo "Cargo version: $(cargo --version)"
            echo ""
            echo "Commands you might want:"
            echo "  cargo build --release     # build the shared library"
          '';
        };

        # Optional: you can expose the toolchain separately if you want
        # packages.rust-toolchain = rustToolchain;
      }
    );
}
