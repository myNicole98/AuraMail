{
  description = "AuraMail development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "x86_64-linux";

      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
        config = {
          # The Android SDK packages are unfree and need their licence accepted
          # at evaluation time, otherwise `nix develop` refuses to build them.
          allowUnfree = true;
          android_sdk.accept_license = true;
        };
      };

      # Every ABI `tauri android build` packages into an APK.
      androidTargets = [
        "aarch64-linux-android"
        "armv7-linux-androideabi"
        "i686-linux-android"
        "x86_64-linux-android"
      ];

      # nixpkgs' rustc only carries the host standard library, so the Android
      # targets come from the overlay toolchain instead of `pkgs.rustc`.
      rust = pkgs.rust-bin.stable.latest.default.override {
        targets = androidTargets;
        extensions = [ "rustfmt" "clippy" "rust-src" ];
      };

      # Versions follow what the Tauri 2.12 Android template expects
      # (compileSdk 37, minSdk 24, AGP 9.3, Gradle 9.6). Pre-seeding the SDK
      # matters: the CLI would otherwise call sdkmanager, which cannot write
      # into the read-only Nix store.
      android = pkgs.androidenv.composeAndroidPackages {
        cmdLineToolsVersion = "22.0";
        platformToolsVersion = "37.0.1";
        # AGP 9.3.1 targets 37.0.0; 36.1.0 is kept because the platform
        # default build-tools version can lag behind the requested one.
        buildToolsVersions = [ "36.1.0" "37.0.0" ];
        # Platform 37.0 is the one `compileSdk = 37` resolves to; 37.1 is the
        # current minor release and costs nothing extra to have around.
        platformVersions = [ "37.0" "37.1" ];
        includeNDK = true;
        ndkVersions = [ "29.0.14206865" ];
        cmakeVersions = [ "4.1.2" ];
        includeEmulator = false;
        includeSystemImages = false;
      };

      androidSdk = "${android.androidsdk}/libexec/android-sdk";
      androidNdk = "${android.ndk-bundle}/libexec/android-sdk/ndk-bundle";
      # Patched for NixOS by nixpkgs' build-tools derivation.
      aapt2 = "${androidSdk}/build-tools/37.0.0/aapt2";

      libraryPath = pkgs.lib.makeLibraryPath [
        pkgs.libayatana-appindicator pkgs.gtk3 pkgs.webkitgtk_4_1
        pkgs.libsoup_3 pkgs.glib pkgs.cairo pkgs.pango pkgs.gdk-pixbuf
        pkgs.atk pkgs.librsvg pkgs.glib-networking
      ];
    in {
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          rust
          nodejs_22 pkg-config openssl go_1_26
          gtk3 webkitgtk_4_1 libsoup_3 glib cairo pango
          gdk-pixbuf atk librsvg glib-networking
          gsettings-desktop-schemas libayatana-appindicator
          # Android toolchain
          jdk17
          android.androidsdk
          unzip
          zip
          which
          file
        ];
        shellHook = ''
          export LD_LIBRARY_PATH="${libraryPath}:''${LD_LIBRARY_PATH:-}"

          export ANDROID_HOME="${androidSdk}"
          export ANDROID_SDK_ROOT="$ANDROID_HOME"
          export NDK_HOME="${androidNdk}"
          export ANDROID_NDK_HOME="$NDK_HOME"
          export ANDROID_NDK_ROOT="$NDK_HOME"
          export JAVA_HOME="${pkgs.jdk17.home}"
          export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/22.0/bin:$PATH"

          # AGP fetches aapt2 from Google's Maven repository, and that binary
          # expects the FHS loader, so it cannot exec on NixOS. Point Gradle at
          # the build-tools copy instead. Kept in the user's gradle.properties
          # because there is no environment-variable equivalent for it.
          gradle_props="''${HOME:-}/.gradle/gradle.properties"
          if [ -n "''${HOME:-}" ] && [ -x "${aapt2}" ] \
            && ! grep -q '^android.aapt2FromMavenOverride=' "$gradle_props" 2>/dev/null; then
            mkdir -p "$(dirname "$gradle_props")"
            echo "android.aapt2FromMavenOverride=${aapt2}" >> "$gradle_props"
          fi
        '';
      };
    };
}
