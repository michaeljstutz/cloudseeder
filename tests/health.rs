use std::net::SocketAddr;
use tokio::sync::oneshot;

const TEST_PREFIX: &str = "test01";

async fn spawn_test_server() -> (SocketAddr, oneshot::Sender<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let (tx, rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        cloudseeder::serve_with_shutdown(listener, TEST_PREFIX, async {
            let _ = rx.await;
        })
        .await
        .expect("server error");
    });

    (addr, tx)
}

#[tokio::test]
async fn healthz_returns_ok() {
    let (addr, shutdown) = spawn_test_server().await;

    let resp = reqwest::get(format!("http://{addr}/healthz"))
        .await
        .expect("request");
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("json body");
    assert_eq!(body["status"], "ok");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn root_returns_unauthorized() {
    let (addr, shutdown) = spawn_test_server().await;

    let resp = reqwest::get(format!("http://{addr}/"))
        .await
        .expect("request");
    assert_eq!(resp.status(), 401);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn prefix_root_returns_ok() {
    let (addr, shutdown) = spawn_test_server().await;

    let resp = reqwest::get(format!("http://{addr}/{TEST_PREFIX}/"))
        .await
        .expect("request");
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.expect("body"), "");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn unknown_path_returns_unauthorized() {
    let (addr, shutdown) = spawn_test_server().await;

    let resp = reqwest::get(format!("http://{addr}/nope"))
        .await
        .expect("request");
    assert_eq!(resp.status(), 401);

    let resp = reqwest::get(format!("http://{addr}/wrong-prefix/"))
        .await
        .expect("request");
    assert_eq!(resp.status(), 401);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn server_stops_on_shutdown_signal() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let (tx, rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        cloudseeder::serve_with_shutdown(listener, TEST_PREFIX, async {
            let _ = rx.await;
        })
        .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    tx.send(()).expect("send shutdown");

    tokio::time::timeout(std::time::Duration::from_secs(2), handle)
        .await
        .expect("server exited within timeout")
        .expect("join")
        .expect("serve ok");
}
