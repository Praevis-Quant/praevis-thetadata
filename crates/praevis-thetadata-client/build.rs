use heck::ToSnakeCase;
use prost::Message;
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rustc = std::process::Command::new(env::var("RUSTC")?)
        .arg("-vV")
        .output()?;
    if !rustc.status.success() {
        return Err("cannot record compiler identity".into());
    }
    println!(
        "cargo:rustc-env=EOD_BENCH_RUSTC={}",
        String::from_utf8(rustc.stdout)?
            .trim()
            .replace('\n', " | ")
            .replace('\r', "")
    );
    // Same descriptor the proto package compiles, taken from its public constant.
    let descriptors =
        prost_types::FileDescriptorSet::decode(praevis_thetadata_proto::FILE_DESCRIPTOR_SET)?;
    let mut source = String::from("impl ThetaClient {\n");
    for file in descriptors.file {
        for service in file.service {
            for method in service.method {
                let rpc = method.name.unwrap();
                let request = method
                    .input_type
                    .unwrap()
                    .rsplit('.')
                    .next()
                    .unwrap()
                    .to_string();
                let method_name = rpc.trim_start_matches("Get").to_snake_case();
                let stub_method = rpc.to_snake_case();
                source.push_str(&format!(
                    "/// Calls `{rpc}`. Query fields use wire types; set optional defaults explicitly.\n\
                     pub async fn {method_name}(&self, params: api::{request}Query) -> Result<ResponseStream, Error> {{\n\
                     let request = api::{request} {{ query_info: Some(self.query_info()), params: Some(params) }};\n\
                     let mut request = tonic::Request::new(request);\n\
                     if let Some(timeout) = self.config.request_timeout {{ request.set_timeout(timeout); }}\n\
                     let response = self.stub.clone().{stub_method}(request).await.map_err(Error::from)?;\n\
                     Ok(ResponseStream {{ inner: response.into_inner(), max_batch_bytes: self.config.max_batch_bytes, idle_timeout: self.config.idle_timeout }})\n\
                     }}\n"
                ));
            }
        }
    }
    source.push_str("}\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("methods.rs"),
        source,
    )?;
    Ok(())
}
