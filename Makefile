.PHONY: install build test serve mcp dashboard clean

install:
	cargo install --path crates/brainmesh-cli --force
	pip install -e sdks/python

build:
	cargo build --workspace

test:
	cargo test --workspace
	PYTHONPATH=sdks/python python3 -m unittest discover -s sdks/python/tests -v
	cd sdks/typescript && npm test

serve:
	cargo run -p brainmesh-cli -- server start --addr 127.0.0.1:8080

mcp:
	cargo run -p brainmesh-cli -- mcp --transport stdio

dashboard:
	cd apps/dashboard && npm run dev

clean:
	cargo clean
