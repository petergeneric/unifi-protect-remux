.PHONY: build-ffi-debug build-ffi-release clean

build-ffi-debug:
	cd ../.. && cargo build -p remux-ffi --no-default-features
	mkdir -p lib
	cp ../../target/debug/libremux_ffi.dylib lib/

build-ffi-release:
	cd ../.. && cargo build -p remux-ffi --release
	mkdir -p lib
	cp ../../target/release/libremux_ffi.dylib lib/

clean:
	rm -rf lib
