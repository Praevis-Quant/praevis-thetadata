//! Wire definitions recovered from the ThetaData Python 1.0.12 distribution.
//! Prefer `praevis-thetadata-client` for authenticated requests and decoded results.

pub mod endpoints {
    tonic::include_proto!("endpoints");
}

pub mod beta_endpoints {
    tonic::include_proto!("beta_endpoints");
}

pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("../schema/thetadata.bin");
