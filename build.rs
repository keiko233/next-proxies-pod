const OUT_DIR: &str = "src/proto-gen";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = ["./proto/v2fly.proto"];

    std::fs::create_dir_all(OUT_DIR).unwrap();

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(OUT_DIR)
        .type_attribute(".", "#[derive(Hash)]")
        .compile_protos(&protos, &["proto/"])?;

    rerun(&protos);

    Ok(())
}

fn rerun(proto_files: &[&str]) {
    for proto_file in proto_files {
        println!("cargo:rerun-if-changed={}", proto_file);
    }
}
