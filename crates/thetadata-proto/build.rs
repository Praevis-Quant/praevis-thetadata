use prost::Message;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=schema/thetadata.bin");
    let descriptors =
        prost_types::FileDescriptorSet::decode(include_bytes!("schema/thetadata.bin").as_slice())?;
    tonic_prost_build::configure().compile_fds(descriptors)?;
    Ok(())
}
