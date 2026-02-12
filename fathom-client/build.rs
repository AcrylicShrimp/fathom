fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = "../proto/fathom/v1/agent.proto";
    println!("cargo:rerun-if-changed={proto}");

    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&[proto], &["../proto"])?;

    Ok(())
}
