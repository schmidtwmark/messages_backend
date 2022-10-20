{ pkgs }: {
	deps = [
		pkgs.sudo
  pkgs.sqlite.bin
  pkgs.rustc
		pkgs.rustfmt
		pkgs.cargo
		pkgs.cargo-edit
        pkgs.rust-analyzer
	];
}