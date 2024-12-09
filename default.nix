{
  lib,
  rustPlatform,
  versionCheckHook,
  cosmic-comp,
}:
let
  version = "1.0.0";
in
rustPlatform.buildRustPackage {
  pname = "cosmic-ctl";
  inherit version;

  src = builtins.path {
    name = "cosmic-ctl-source";
    path = ./.;
  };

  cargoHash = "sha256-kN0q6/o3ASSnx4GAb9wce0a3JP7PwC20iRG0jarr7NA=";

  doInstallCheck = true;
  nativeInstallCheckInputs = [ versionCheckHook ];
  versionCheckProgram = "${placeholder "out"}/bin/cosmic-ctl";

  meta = {
    description = "CLI for COSMIC Desktop configuration management";
    changelog = "https://github.com/HeitorAugustoLN/cosmic-ctl/releases/tag/v${version}";
    homepage = "https://github.com/HeitorAugustoLN/cosmic-ctl";
    license = lib.licenses.gpl3Only;
    maintainers = with lib.maintainers; [ HeitorAugustoLN ];
    mainProgram = "cosmic-ctl";
    inherit (cosmic-comp.meta) platforms;
  };
}
