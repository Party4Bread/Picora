use super::{MAX_ASSET_BYTES, read_response};
use std::io::{Read, Write};

async fn response(status: u16, headers: &str, body: Vec<u8>) -> reqwest::Response {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let header = format!("HTTP/1.1 {status} Fixture\r\n{headers}Connection: close\r\n\r\n");
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        // Size/status rejection may intentionally close the client before the body is written.
        let _ = stream
            .write_all(header.as_bytes())
            .and_then(|()| stream.write_all(&body));
    });
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap()
        .get(format!("http://{address}/sticker"))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn failed_and_stopped_redirect_responses_are_not_assets() {
    for (status, message) in [
        (401, "Access denied"),
        (403, "Access denied"),
        (429, "rate limit"),
        (302, "Unexpected repository response"),
        (500, "500"),
    ] {
        let result = response(
            status,
            "Content-Length: 0\r\nLocation: https://unsupported.invalid/\r\n",
            vec![],
        )
        .await;
        let error = read_response(result).await.unwrap_err();
        assert!(
            format!("{error:#}").contains(message),
            "{status}: {error:#}"
        );
    }
    let result = response(200, "Content-Length: 3\r\n", b"png".to_vec()).await;
    assert_eq!(read_response(result).await.unwrap(), b"png");
}

#[tokio::test]
async fn declared_and_streamed_lengths_enforce_the_same_limit() {
    let headers = format!("Content-Length: {}\r\n", MAX_ASSET_BYTES + 1);
    let declared = response(200, &headers, vec![]).await;
    assert!(
        read_response(declared)
            .await
            .unwrap_err()
            .to_string()
            .contains("25 MiB")
    );

    let mut body = format!("{:x}\r\n", MAX_ASSET_BYTES + 1).into_bytes();
    body.resize(body.len() + MAX_ASSET_BYTES + 1, b'x');
    body.extend_from_slice(b"\r\n0\r\n\r\n");
    let streamed = response(200, "Transfer-Encoding: chunked\r\n", body).await;
    assert!(
        read_response(streamed)
            .await
            .unwrap_err()
            .to_string()
            .contains("25 MiB")
    );
}
