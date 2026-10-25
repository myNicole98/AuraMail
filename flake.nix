{
  description = "AuraMail development shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
    in {
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          rustc cargo rustfmt clippy
          nodejs_22 pkg-config openssl
          gtk3 webkitgtk_4_1 libsoup_3 glib cairo pango
          gdk-pixbuf atk librsvg glib-networking
          gsettings-desktop-schemas libayatana-appindicator
        ];
        shellHook = ''
          export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [
            pkgs.libayatana-appindicator pkgs.gtk3 pkgs.webkitgtk_4_1
            pkgs.libsoup_3 pkgs.glib pkgs.cairo pkgs.pango pkgs.gdk-pixbuf
            pkgs.atk pkgs.librsvg pkgs.glib-networking
          ]}:''${LD_LIBRARY_PATH:-}"
        '';
      };
    };
}
