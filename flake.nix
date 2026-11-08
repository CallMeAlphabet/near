{
  description = "near - LAN-only file transfer hub";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = f: builtins.listToAttrs (map (system: { name = system; value = f system; }) systems);
    in
    {
      packages = forAll (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "near";
            version = "0.1.0";
            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = path: type:
                let base = baseNameOf path;
                in (base != "target" && base != "result")
                   && pkgs.lib.cleanSourceFilter path type;
            };
            cargoLock.lockFile = ./Cargo.lock;
            meta = with pkgs.lib; {
              description = "LAN-only file transfer hub";
              homepage = "https://github.com/CallMeAlphabet/near";
              license = licenses.asl20;
              mainProgram = "near";
              platforms = platforms.unix;
            };
          };
        });

      apps = forAll (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/near";
        };
      });

      devShells = forAll (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.mkShell {
            packages = [ pkgs.rustc pkgs.cargo ];
          };
        });
    };
}
