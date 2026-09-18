{
  description = "mdoc — Markdown WYSIWYG editor and PDF viewer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs { inherit system; };

        # Linux libraries gpui links against (the README's apt list) plus the
        # Vulkan loader, which gpui dlopens at runtime rather than linking.
        runtimeLibs = with pkgs; [
          wayland
          libxkbcommon
          vulkan-loader
          fontconfig
          freetype
          xorg.libX11
          xorg.libxcb
          xorg.xcbutil
        ];

        mdoc = pkgs.rustPlatform.buildRustPackage {
          pname = "mdoc";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

          src = self;

          cargoLock = {
            lockFile = ./Cargo.lock;
            allowBuiltinFetchGit = true;
          };

          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = runtimeLibs;

          # The workspace tests are headless (CI runs them on bare runners).
          # Doc-tests need the same libs; both inherit buildInputs.
          checkFlags = [ ];

          postFixup = ''
            # gpui dlopens Vulkan and the Wayland/X11 client libs at runtime.
            patchelf --add-rpath ${pkgs.lib.makeLibraryPath runtimeLibs} \
              $out/bin/mdoc
          '';

          postInstall = ''
            install -Dm644 resources/icons/icon.png \
              $out/share/icons/hicolor/512x512/apps/mdoc.png
            mkdir -p $out/share/applications
            cat > $out/share/applications/mdoc.desktop <<EOF
            [Desktop Entry]
            Name=mdoc
            Comment=Markdown WYSIWYG editor and PDF viewer
            Exec=mdoc
            Icon=mdoc
            Type=Application
            Categories=Office;
            EOF
          '';

          meta = with pkgs.lib; {
            description = "Markdown WYSIWYG editor and PDF viewer";
            homepage = "https://github.com/lawkitt/mdoc";
            license = licenses.gpl3Plus;
            mainProgram = "mdoc";
            platforms = [ "x86_64-linux" "aarch64-linux" ];
          };
        };
      in
      {
        packages.default = mdoc;
        packages.mdoc = mdoc;

        devShells.default = pkgs.mkShell {
          inputsFrom = [ mdoc ];
          packages = with pkgs; [ rustc cargo clippy rustfmt ];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
        };
      });
}
