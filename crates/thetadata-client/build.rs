use heck::ToSnakeCase;
use prost::Message;
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = "../thetadata-proto/schema/thetadata.bin";
    println!("cargo:rerun-if-changed={schema}");
    let descriptors = prost_types::FileDescriptorSet::decode(fs::read(schema)?.as_slice())?;
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
                     request.set_timeout(self.config.request_timeout);\n\
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
