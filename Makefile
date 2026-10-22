.PHONY: all linux macos windows clean

all:
	@echo "Specify target: make macos | make linux | make windows"

macos:
	./build_macos.sh

linux:
	./build_linux.sh

windows:
	cargo build --release --manifest-path gprix_recomp/Cargo.toml

clean:
	cargo clean --manifest-path gprix_recomp/Cargo.toml
	rm -rf GrandPrixCircuit_macOS GrandPrixCircuit_Linux
