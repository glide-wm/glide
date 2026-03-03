{
  description = "Glide is a tiling window manager for macOS.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      lib = nixpkgs.lib;
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      packagerMeta = cargoToml.package.metadata.packager;
      productName = packagerMeta.productName;
      bundleName = "${productName}.app";
      bundleIdentifier = packagerMeta.identifier;
      version = cargoToml.package.version;
      supportedSystems = [
        "aarch64-darwin"
        "x86_64-darwin"
      ];
      forAllSystems = f:
        lib.genAttrs supportedSystems (
          system: f (import nixpkgs { inherit system; })
        );
      mkGlide = pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          inherit version;

          src = pkgs.lib.cleanSource ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            allowBuiltinFetchGit = true;
          };

          doCheck = false;

          meta = with pkgs.lib; {
            description = cargoToml.package.description;
            homepage = cargoToml.package.homepage;
            license = [
              licenses.asl20
              licenses.mit
            ];
            mainProgram = "glide";
            platforms = platforms.darwin;
          };
        };
      mkGlideBundle = pkgs: unbundled:
        pkgs.runCommand "${cargoToml.package.name}-app-${version}" {
          meta = with pkgs.lib; {
            description = "${cargoToml.package.description} (${bundleName})";
            homepage = cargoToml.package.homepage;
            license = [
              licenses.asl20
              licenses.mit
            ];
            mainProgram = "glide";
            platforms = platforms.darwin;
          };
        } ''
          bundle="$out/Applications/${bundleName}"

          mkdir -p "$bundle/Contents/MacOS" "$out/bin"

          install -m755 ${unbundled}/bin/glide "$bundle/Contents/MacOS/glide"
          install -m755 ${unbundled}/bin/glide_server "$bundle/Contents/MacOS/glide_server"

          printf '%s\n' \
            '<?xml version="1.0" encoding="UTF-8"?>' \
            '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
            '<plist version="1.0">' \
            '  <dict>' \
            '    <key>CFBundleDevelopmentRegion</key>' \
            '    <string>en</string>' \
            '    <key>CFBundleExecutable</key>' \
            '    <string>glide_server</string>' \
            '    <key>CFBundleIdentifier</key>' \
            '    <string>${bundleIdentifier}</string>' \
            '    <key>CFBundleInfoDictionaryVersion</key>' \
            '    <string>6.0</string>' \
            '    <key>CFBundleName</key>' \
            '    <string>${productName}</string>' \
            '    <key>CFBundlePackageType</key>' \
            '    <string>APPL</string>' \
            '    <key>CFBundleShortVersionString</key>' \
            '    <string>${version}</string>' \
            '    <key>CFBundleVersion</key>' \
            '    <string>${version}</string>' \
            '    <key>NSPrincipalClass</key>' \
            '    <string>NSApplication</string>' \
            '  </dict>' \
            '</plist>' \
            > "$bundle/Contents/Info.plist"

          printf 'APPL????' > "$bundle/Contents/PkgInfo"

          ln -s "../Applications/${bundleName}/Contents/MacOS/glide" "$out/bin/glide"
          ln -s "../Applications/${bundleName}/Contents/MacOS/glide_server" "$out/bin/glide_server"

          printf '%s\n' \
            '#!/bin/sh' \
            "exec \"$out/bin/glide\" launch \"\$@\"" \
            > "$out/bin/glide-launch"
          chmod +x "$out/bin/glide-launch"
        '';
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          unbundled = mkGlide pkgs;
          glide = mkGlideBundle pkgs unbundled;
        in
        {
          default = glide;
          "glide-wm" = glide;
          bundle = glide;
          inherit unbundled;
        }
      );

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/glide-launch";
        };
      });
    };
}
