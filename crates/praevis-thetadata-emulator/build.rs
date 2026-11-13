//! Implements the generated gRPC server trait for every RPC in the checked-in
//! descriptor, each forwarding to one generic handler (ADR-0022).
use heck::ToSnakeCase;
use prost::Message;
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let descriptors =
        prost_types::FileDescriptorSet::decode(praevis_thetadata_proto::FILE_DESCRIPTOR_SET)?;
    let mut service = String::from(
        "#[tonic::async_trait]\nimpl api::beta_theta_terminal_server::BetaThetaTerminal for Service {\n",
    );
    let mut names =
        String::from("/// Every RPC in the descriptor.\npub const RPCS: &[&str] = &[\n");
    for file in descriptors.file {
        for svc in file.service {
            for method in svc.method {
                let rpc = method.name.unwrap();
                let request = method.input_type.unwrap();
                let request = request.rsplit('.').next().unwrap();
                let stub = rpc.to_snake_case();
                service.push_str(&format!(
                    "type {rpc}Stream = Responses;\n\
                     async fn {stub}(&self, request: tonic::Request<api::{request}>) -> Result<tonic::Response<Responses>, tonic::Status> {{\n\
                     self.serve(\"{rpc}\", prost::Message::encode_to_vec(request.get_ref())).await\n}}\n"
                ));
                names.push_str(&format!("    \"{rpc}\",\n"));
            }
        }
    }
    service.push_str("}\n");
    names.push_str("];\n");
    let out = PathBuf::from(env::var("OUT_DIR")?);
    fs::write(out.join("service.rs"), service)?;
    fs::write(out.join("rpcs.rs"), names)?;
    Ok(())
}
