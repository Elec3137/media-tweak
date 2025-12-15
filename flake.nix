{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { self, nixpkgs, ... }:
    let
      pkgs = nixpkgs.legacyPackages."x86_64-linux";
    in
    with pkgs;
    {
      packages."x86_64-linux".default = rustPlatform.buildRustPackage {
        pname = "media-tweak";
        version = self.shortRev or self.dirtyShortRev;
        src = ./.;
        cargoLock = {
          lockFile = ./Cargo.lock;
        };
        nativeBuildInputs = [ pkg-config ];
        buildInputs = [ ffmpeg ];
      };

      devShells."x86_64-linux".default = mkShell {
        inputsFrom = [ self.packages."x86_64-linux".default ffmpeg ];
        LD_LIBRARY_PATH = lib.makeLibraryPath [
          wayland
          libxkbcommon
          ffmpeg
          libclang
        ];
      };
    };
}
