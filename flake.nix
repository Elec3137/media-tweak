{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { self, nixpkgs, ... }:
    let
      pkgs = nixpkgs.legacyPackages."x86_64-linux";
      cargoToml = (fromTOML (builtins.readFile ./Cargo.toml));
    in
    with pkgs;
    {
      packages."x86_64-linux".default = rustPlatform.buildRustPackage rec {
        pname = cargoToml.package.name;
        version = cargoToml.package.version;

        src = ./.;

        cargoDeps = rustPlatform.importCargoLock {
          lockFile = "${src}/Cargo.lock";
        };

        nativeBuildInputs = [
          rustPlatform.bindgenHook
          makeBinaryWrapper
          pkg-config
          ffmpeg
        ];

        buildInputs = [
          ffmpeg

          libxkbcommon

          wayland

          xorg.libX11
          xorg.libXcursor
          xorg.libXi
        ];

        desktopItem = makeDesktopItem {
          name = pname;
          desktopName = pname;
          mimeTypes = [
            "video/matroshka"
            "video/webm"
            "video/mp4"

            "audio/matroshka"
            "audio/webm"
            "audio/mp4"

            "audio/aac"
            "audio/flac"
            "audio/ogg"
          ];
          icon = "image-x-generic";
          exec = pname;
        };

        postFixup = ''
          mkdir -p "$out/share/applications"
          ln -s "${desktopItem}"/share/applications/* "$out/share/applications/"

          wrapProgram $out/bin/${pname} \
            --prefix PATH : ${lib.makeBinPath [ ffmpeg ]} \
            --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath buildInputs}
        '';
      };

      devShells."x86_64-linux".default = mkShell {
        inputsFrom = [ self.packages."x86_64-linux".default ];

        buildInputs = [
          cargo
          clippy
          rust-analyzer
          rustfmt
        ];

        LD_LIBRARY_PATH = lib.makeLibraryPath [
          wayland
          libxkbcommon
        ];
      };
    };
}
