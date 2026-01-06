{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      crane,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        craneLib = crane.mkLib pkgs;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;

          # workaround from https://crane.dev/faq/rebuilds-bindgen.html
          NIX_OUTPATH_USED_AS_RANDOM_SEED = "aaaaaaaaaa";

          nativeBuildInputs = with pkgs; [
            rustPlatform.bindgenHook
            makeBinaryWrapper
            pkg-config
            ffmpeg
          ];

          buildInputs = with pkgs; [
            ffmpeg

            libxkbcommon

            wayland

            xorg.libX11
            xorg.libXcursor
            xorg.libXi
          ];
        };

        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath commonArgs.buildInputs;

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Build the actual crate itself, reusing the dependency
        # artifacts from above.
        crate = craneLib.buildPackage (
          commonArgs
          // rec {
            inherit cargoArtifacts;

            pname = (fromTOML (builtins.readFile ./Cargo.toml)).package.name;
            desktopItem = pkgs.makeDesktopItem {
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
                --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.ffmpeg ]} \
                --prefix LD_LIBRARY_PATH : ${LD_LIBRARY_PATH}
            '';
          }
        );
      in
      {
        packages.default = crate;

        checks = {
          # Build the crate as part of `nix flake check` for convenience
          inherit crate;

          # Run clippy on the crate source, resuing the dependency artifacts
          # (e.g. from build scripts or proc-macros) from above.
          #
          # Note that this is done as a separate derivation so it
          # does not impact building just the crate by itself.
          crate-clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "-- --deny warnings";
            }
          );
        };

        devShells.default = craneLib.devShell {
          inherit LD_LIBRARY_PATH;

          inputsFrom = [ crate ];
          packages = [ pkgs.rust-analyzer ];
        };
      }
    );
}
