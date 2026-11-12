{
  description = "ET54 Rust library and native egui development environment";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forSystems = f: builtins.listToAttrs (map (system: { name = system; value = f system; }) systems);
    in {
      devShells = forSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          libraries = with pkgs; [
            libGL libxkbcommon wayland
            libx11 libxcursor libxi libxrandr libxcb
          ];
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [ cargo rustc rustfmt clippy pkg-config xorg-server xauth xvfb-run ];
            buildInputs = libraries;
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath libraries;
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };
        });
    };
}
