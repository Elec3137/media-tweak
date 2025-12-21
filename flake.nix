{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { self, nixpkgs, ... }:
    let
      pkgs = nixpkgs.legacyPackages."x86_64-linux";
      cargoToml = (builtins.fromTOML (builtins.readFile ./Cargo.toml));
    in
    with pkgs;
    {
      packages."x86_64-linux".default = rustPlatform.buildRustPackage rec {
        pname = cargoToml.package.name;
        version = cargoToml.package.version;

        src = ./.;

        cargoDeps = rustPlatform.importCargoLock {
          lockFile = "${src}/Cargo.lock";
          outputHashes = {
            "accesskit-0.16.0" = "sha256-uoLcd116WXQTu1ZTfJDEl9+3UPpGBN/QuJpkkGyRADQ=";
            "atomicwrites-0.4.2" = "sha256-QZSuGPrJXh+svMeFWqAXoqZQxLq/WfIiamqvjJNVhxA=";
            "clipboard_macos-0.1.0" = "sha256-+8CGmBf1Gl9gnBDtuKtkzUE5rySebhH7Bsq/kNlJofY=";
            "cosmic-client-toolkit-0.1.0" = "sha256-KvXQJ/EIRyrlmi80WKl2T9Bn+j7GCfQlcjgcEVUxPkc=";
            "cosmic-config-0.1.0" = "sha256-rNgyjty6kY7/pNXwKEU41fzRX+MyRmYwICgWWGiLmhc=";
            "cosmic-freedesktop-icons-0.4.0" = "sha256-D4bWHQ4Dp8UGiZjc6geh2c2SGYhB7mX13THpCUie1c4=";
            "cosmic-settings-daemon-0.1.0" = "sha256-3QCkl2/kof0l8S3zAppEWL88uaXAH43NL4UJA0xVCPI=";
            "cosmic-text-0.15.0" = "sha256-g9OCXlr6+WQ5cIg37pGPmIpVJLZ40lke4SmU5SBwXGo=";
            "dpi-0.1.1" = "sha256-PeHUUvJpntEhmAy8PSkXponc9OZ3YcQgpEe9sV4l8ig=";
            "iced_glyphon-0.6.0" = "sha256-u1vnsOjP8npQ57NNSikotuHxpi4Mp/rV9038vAgCsfQ=";
            "smithay-clipboard-0.8.0" = "sha256-4InFXm0ahrqFrtNLeqIuE3yeOpxKZJZx+Bc0yQDtv34=";
           "softbuffer-0.4.1" = "sha256-/ocK79Lr5ywP/bb5mrcm7eTzeBbwpOazojvFUsAjMKM=";
          };
        };

        nativeBuildInputs = [
          rustPlatform.bindgenHook
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
          patchelf --set-rpath ${lib.makeLibraryPath buildInputs} $out/bin/${pname}
        '';
      };

      devShells."x86_64-linux".default = mkShell {
        inputsFrom = [ self.packages."x86_64-linux".default ];
        LD_LIBRARY_PATH = lib.makeLibraryPath [
          wayland
          libxkbcommon
        ];
      };
    };
}
