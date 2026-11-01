//! Checks the staged proto archive generated its bindings from the packaged descriptor.
use praevis_thetadata_proto::{FILE_DESCRIPTOR_SET, endpoints::ZonedDateTime};
use prost::Message;

fn main() {
    let set = prost_types::FileDescriptorSet::decode(FILE_DESCRIPTOR_SET).unwrap();
    let rpcs: usize = set.file.iter().flat_map(|f| &f.service).map(|s| s.method.len()).sum();
    assert_eq!(rpcs, 100, "Python 1.0.17 descriptor");
    let value = ZonedDateTime { epoch_ms: 1, zone: 1, ..Default::default() };
    assert_eq!(ZonedDateTime::decode(value.encode_to_vec().as_slice()).unwrap(), value);
    println!("proto-consumer: {rpcs} RPCs in the packaged descriptor");
}
