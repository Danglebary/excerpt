{
  description = "The excerpt development shell, every tool the gate runs, and the excerpt binary as a package, pinned by the lock.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
      # The toolchain is the one `rust-toolchain.toml` pins, so CI's rustup and this
      # shell resolve the same compiler from one file.
      toolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      shellFor =
        system:
        let
          pkgs = pkgsFor system;
        in
        pkgs.mkShell {
          # Set inside this shell alone, so a recipe can tell it already stands in it
          # and need not enter it again.
          EXCERPT_SHELL = "1";
          # Cargo fetches git dependencies through `git`, which authenticates the way the
          # user's git configuration says, an `insteadOf` rewriting GitHub's https URLs to
          # ssh among it; cargo's built-in client authenticates over ssh through an agent
          # alone.
          # https://doc.rust-lang.org/cargo/reference/config.html#netgit-fetch-with-cli
          # — checked 2026-10-30
          CARGO_NET_GIT_FETCH_WITH_CLI = "true";
          # samply records a benchmark under the profiling profile and serves what it
          # recorded to the browser's profiler view on the local machine.
          packages = [
            (toolchainFor pkgs)
            pkgs.git
            pkgs.just
            pkgs.samply
          ];
        };
      # The binary, built by the pinned toolchain from the manifest, its lock, the crate's
      # sources, the benches, whose files cargo reads to parse a manifest declaring them,
      # and the workspace's other member, whose manifest and targets cargo reads to load
      # the workspace. The tests are the gate's, so the build runs none.
      packageFor =
        system:
        let
          pkgs = pkgsFor system;
          toolchain = toolchainFor pkgs;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
          fileset = pkgs.lib.fileset;
          manifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        in
        rustPlatform.buildRustPackage {
          pname = "excerpt";
          version = manifest.package.version;
          src = fileset.toSource {
            root = ./.;
            fileset = fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./benches
              ./src
              ./measure/Cargo.toml
              ./measure/src
            ];
          };
          # A git dependency's sources are fetched by their hash, which a build refuses to
          # start without; the first build after a pin moves fails printing the new one.
          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes."claude-code-transcript-0.1.0" = "sha256-CXa2m1mGRM3NZNuNFvCW4zK34nmokJDXQY/jARmSf2U=";
          };
          doCheck = false;
          meta.mainProgram = "excerpt";
        };
    in
    {
      devShells = nixpkgs.lib.genAttrs systems (system: {
        default = shellFor system;
      });
      packages = nixpkgs.lib.genAttrs systems (system: {
        default = packageFor system;
      });
    };
}
