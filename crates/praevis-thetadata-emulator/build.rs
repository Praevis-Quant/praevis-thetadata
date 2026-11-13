//! Implements the generated gRPC server trait for every RPC in the checked-in
//! descriptor, each forwarding to one generic handler (ADR-0022), with the
//! request fields the behaviours read (symbol, dates, interval, strike).
use heck::ToSnakeCase;
use prost::Message;
use prost_types::{DescriptorProto, field_descriptor_proto::Label};
use std::{collections::HashMap, env, fs, path::PathBuf};

/// The query fields the behaviours read, by name.
const FIELDS: &[&str] = &[
    "symbol",
    "start_date",
    "end_date",
    "date",
    "interval",
    "expiration",
];

/// Code that copies `field` of `from` into `Params` (`p`).
fn copy(field: &prost_types::FieldDescriptorProto, from: &str) -> String {
    let name = field.name();
    let target = if name == "symbol" { "symbol" } else { name };
    if field.label() == Label::Repeated {
        format!("p.{target}.extend({from}.{name}.iter().cloned());\n")
    } else if field.proto3_optional() {
        if name == "symbol" {
            format!("p.symbol.extend({from}.{name}.iter().cloned());\n")
        } else {
            format!("p.{target} = {from}.{name}.clone();\n")
        }
    } else if name == "symbol" {
        format!("p.symbol.push({from}.{name}.clone());\n")
    } else {
        // An empty plain string is an unset field: never let it overwrite
        // a value already read (an option request's expiration sits in its
        // contract_spec, with an empty top-level twin).
        format!("if !{from}.{name}.is_empty() {{ p.{target} = Some({from}.{name}.clone()); }}\n")
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let descriptors =
        prost_types::FileDescriptorSet::decode(praevis_thetadata_proto::FILE_DESCRIPTOR_SET)?;
    let messages: HashMap<String, DescriptorProto> = descriptors
        .file
        .iter()
        .flat_map(|file| {
            let package = file.package().to_owned();
            file.message_type
                .iter()
                .map(move |m| (format!(".{package}.{}", m.name()), m.clone()))
        })
        .collect();
    let mut service = String::from(
        "#[tonic::async_trait]\nimpl api::beta_theta_terminal_server::BetaThetaTerminal for Service {\n",
    );
    let mut names =
        String::from("/// Every RPC in the descriptor.\npub const RPCS: &[&str] = &[\n");
    for file in &descriptors.file {
        for svc in &file.service {
            for method in &svc.method {
                let rpc = method.name();
                let request = method.input_type().rsplit('.').next().unwrap();
                let stub = rpc.to_snake_case();
                let mut extract = String::new();
                let query = messages[method.input_type()]
                    .field
                    .iter()
                    .find(|f| f.name() == "params")
                    .map(|f| &messages[f.type_name()]);
                for field in query.map(|q| q.field.as_slice()).unwrap_or_default() {
                    if FIELDS.contains(&field.name()) {
                        extract.push_str(&copy(field, "q"));
                    } else if field.name() == "contract_spec" {
                        extract.push_str(
                            "if let Some(c) = &q.contract_spec { p.symbol.push(c.symbol.clone()); p.strike = c.strike.clone(); if !c.expiration.is_empty() { p.expiration = Some(c.expiration.clone()); } }\n",
                        );
                    }
                }
                service.push_str(&format!(
                    "type {rpc}Stream = Responses;\n\
                     async fn {stub}(&self, request: tonic::Request<api::{request}>) -> Result<tonic::Response<Responses>, tonic::Status> {{\n\
                     let timeout = grpc_timeout(&request);\n\
                     #[allow(unused_mut)]\n\
                     let mut p = Params::default();\n\
                     if let Some(q) = &request.get_ref().params {{ let _ = q;\n{extract}}}\n\
                     self.serve(\"{rpc}\", prost::Message::encode_to_vec(request.get_ref()), timeout, p).await\n}}\n"
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
