#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::net::{TcpListener, TcpStream};
    use tokio_rustls::{TlsAcceptor, TlsConnector};

    use crate::{
        identity::{DeviceIdentity, certificate_fingerprint},
        protocol::{PROTOCOL_MAJOR, PROTOCOL_MINOR, read_envelope, wire, write_envelope},
    };

    #[tokio::test]
    async fn mutual_tls_exposes_peer_fingerprints_and_carries_control_messages() {
        let server_identity = DeviceIdentity::generate().expect("server identity");
        let client_identity = DeviceIdentity::generate().expect("client identity");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("listener address");
        let expected_client = client_identity.fingerprint();
        let expected_server = server_identity.fingerprint();
        let acceptor = TlsAcceptor::from(Arc::new(
            server_identity.server_config().expect("server config"),
        ));

        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.expect("accept tcp");
            let mut tls = acceptor.accept(socket).await.expect("accept tls");
            let peer = certificate_fingerprint(
                tls.get_ref()
                    .1
                    .peer_certificates()
                    .expect("client certificate")
                    .first()
                    .expect("client leaf")
                    .as_ref(),
            );
            let hello = read_envelope(&mut tls).await.expect("client hello");
            write_envelope(&mut tls, &hello).await.expect("echo hello");
            peer
        });

        let connector = TlsConnector::from(Arc::new(
            client_identity.client_config().expect("client config"),
        ));
        let socket = TcpStream::connect(address).await.expect("connect tcp");
        let server_name = "transassist.local".try_into().expect("server name");
        let mut tls = connector
            .connect(server_name, socket)
            .await
            .expect("connect tls");
        let seen_server = certificate_fingerprint(
            tls.get_ref()
                .1
                .peer_certificates()
                .expect("server certificate")
                .first()
                .expect("server leaf")
                .as_ref(),
        );
        let hello = wire::Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            payload: Some(wire::envelope::Payload::Hello(wire::Hello {
                device_id: client_identity.device_id().to_owned(),
                display_name: "测试手机".to_owned(),
                certificate_fingerprint: client_identity.fingerprint().to_vec(),
                capabilities: 1,
            })),
        };
        write_envelope(&mut tls, &hello).await.expect("send hello");
        assert_eq!(read_envelope(&mut tls).await.expect("echo"), hello);

        assert_eq!(seen_server, expected_server);
        assert_eq!(server.await.expect("server task"), expected_client);
    }
}
