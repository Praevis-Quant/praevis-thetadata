//! With both rustls providers compiled in and no process default installed,
//! rustls cannot choose one itself: anything building its configuration from
//! the process default panics. The auth client and the gRPC client must pick
//! a provider explicitly, so this consumer still connects.
//!
//! The test's TLS server is its own terminator in front of the plaintext
//! emulator, with an explicit provider: tonic's server side would ask for
//! the process default, and installing one would hide what is tested.
use praevis_thetadata_auth::{AuthClient, AuthConfig};
use praevis_thetadata_client::{ClientConfig, NaiveDate, StockEodRequest, ThetaClient};
use praevis_thetadata_emulator::fixture;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_providers_enabled_and_none_installed_still_connects_over_tls() {
    // The precondition: rustls alone cannot pick a provider here.
    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
    assert!(std::panic::catch_unwind(rustls::ClientConfig::builder).is_err());

    // The auth client builds its HTTPS client.
    AuthClient::new(AuthConfig::default()).expect("auth client");

    // A test CA, trusted through the native roots the client loads.
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "tls-providers test CA");
    let ca = ca_params.self_signed(&ca_key).unwrap();
    let dir = std::env::temp_dir().join(format!("tls-providers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let roots = dir.join("ca.pem");
    std::fs::write(&roots, ca.pem()).unwrap();
    // SAFETY: the only test in this binary, set before any thread reads it.
    unsafe { std::env::set_var("SSL_CERT_FILE", &roots) };

    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::new(vec!["127.0.0.1".to_owned()]).unwrap();
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let cert = params
        .signed_by(&key, &Issuer::from_params(&ca_params, &ca_key))
        .unwrap();
    let (wire, expected) = fixture::table(fixture::Shape::Mixed, 3);
    let plain = fixture::Fixture::start(fixture::Script {
        messages: vec![Ok(fixture::response(&wire, false))],
        ..Default::default()
    });
    let upstream = plain.endpoint.trim_start_matches("http://").to_owned();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert.der().to_vec())],
            PrivateKeyDer::from_pem_slice(key.serialize_pem().as_bytes()).unwrap(),
        )
        .unwrap();
    tls.alpn_protocols = vec![b"h2".to_vec()];
    let acceptor = TlsAcceptor::from(Arc::new(tls));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("https://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let (acceptor, upstream) = (acceptor.clone(), upstream.clone());
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(socket).await else {
                    return;
                };
                let Ok(mut plain) = TcpStream::connect(upstream).await else {
                    return;
                };
                let _ = tokio::io::copy_bidirectional(&mut tls, &mut plain).await;
            });
        }
    });

    // The gRPC client connects over TLS and reads a table.
    let config = ClientConfig::default().with_endpoint(Some(endpoint));
    let client = ThetaClient::with_session(config, fixture::session().await)
        .await
        .expect("TLS connect");
    let day = |d| NaiveDate::from_ymd_opt(2026, 1, d).unwrap();
    let request = StockEodRequest::new("SYNTHETIC", day(15), day(16)).unwrap();
    let mut table = client.stock_eod(request).await.expect("request");
    assert_eq!(table.next_batch().await.unwrap().unwrap(), expected);
    let _ = std::fs::remove_dir_all(dir);
}
