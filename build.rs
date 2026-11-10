fn main() {
    println!("cargo:rerun-if-changed=abi");
    println!("cargo:rerun-if-changed=proto");

    substreams_ethereum::Abigen::new("Factory", "abi/factory.json")
        .expect("Failed to load factory ABI")
        .generate()
        .expect("Failed to generate factory bindings")
        .write_to_file("src/abi/factory.rs")
        .expect("Failed to write factory bindings");

    substreams_ethereum::Abigen::new("Pair", "abi/pair.json")
        .expect("Failed to load pair ABI")
        .generate()
        .expect("Failed to generate pair bindings")
        .write_to_file("src/abi/pair.rs")
        .expect("Failed to write pair bindings");

    buffa_build::Config::new()
        .files(&["proto/uniswapv2.proto"])
        .includes(&["proto/"])
        .preserve_unknown_fields(false)
        .compile()
        .expect("compiling protos");
}
