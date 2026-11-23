//! The gRPC length-prefix framing guard on untrusted bytes, in transport
//! chunks the input chooses: tonic sees only empty frames, every parked
//! message fits the limit and is allocated exactly once.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    praevis_thetadata_client::fuzzing::grpc_framing(data);
});
