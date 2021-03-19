fn main() {
	// Tell Cargo that if the given file changes, to rerun this build script.
	println!("cargo:rerun-if-changed=compression_cpp/compress.cpp");
	println!("cargo:rerun-if-changed=compression_cpp/compress.h");
	// Use the `cc` crate to build a C file and statically link it.
	cc::Build::new().file("compression_cpp/compress.cpp").compile("compress");
}
