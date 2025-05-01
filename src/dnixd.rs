use std::path::{Path, PathBuf};

use axum::body::Body;
use color_eyre::eyre::eyre;
use color_eyre::eyre::WrapErr;
use hyper::client::conn::http1::SendRequest;
use hyper::{Method, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;

use crate::{DETERMINATE_NIXD_SOCKET_NAME, DETERMINATE_STATE_DIR};

pub async fn dnixd_uds() -> color_eyre::Result<SendRequest<axum::body::Body>> {
    let dnixd_state_dir = Path::new(&DETERMINATE_STATE_DIR);
    let dnixd_uds_socket_path: PathBuf = dnixd_state_dir.join(DETERMINATE_NIXD_SOCKET_NAME);

    let stream = TokioIo::new(
        UnixStream::connect(dnixd_uds_socket_path)
            .await
            .wrap_err("Connecting to the determinate-nixd socket")?,
    );
    let (mut sender, conn): (SendRequest<Body>, _) = hyper::client::conn::http1::handshake(stream)
        .await
        .wrap_err("Completing the http1 handshake with determinate-nixd")?;

    // NOTE(colemickens): for now we just drop the joinhandle and let it keep running
    let _join_handle = tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            tracing::error!("Connection failed: {:?}", err);
        }
    });

    let request = http::Request::builder()
        .method(Method::GET)
        .uri("http://localhost/info")
        .body(axum::body::Body::empty())?;

    let response = sender
        .send_request(request)
        .await
        .wrap_err("Querying information about determinate-nixd")?;

    if response.status() != StatusCode::OK {
        tracing::error!("failed to connect to determinate-nixd socket");
        return Err(eyre!("failed to connect to determinate-nixd socket"));
    }

    Ok(sender)
}
