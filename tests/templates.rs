use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::sync::oneshot;

const TEST_PREFIX: &str = "test01";

struct TestServer {
    addr: SocketAddr,
    shutdown: oneshot::Sender<()>,
    _tempdir: tempfile::TempDir,
}

async fn spawn(templates_dir: PathBuf, tempdir: tempfile::TempDir) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let (tx, rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        cloudseeder::serve_with_shutdown(listener, TEST_PREFIX, templates_dir, async {
            let _ = rx.await;
        })
        .await
        .expect("server error");
    });

    TestServer {
        addr,
        shutdown: tx,
        _tempdir: tempdir,
    }
}

async fn server_with_templates() -> TestServer {
    let dir = tempfile::tempdir().unwrap();
    let templates_dir = dir.path().to_path_buf();
    spawn(templates_dir, dir).await
}

fn write_template_file(server: &TestServer, template: &str, filename: &str, contents: &str) {
    let dir = server._tempdir.path().join(template);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(filename), contents).unwrap();
}

fn make_template_dir(server: &TestServer, template: &str) {
    std::fs::create_dir_all(server._tempdir.path().join(template)).unwrap();
}

#[tokio::test]
async fn missing_template_folder_returns_404_for_each_file() {
    let server = server_with_templates().await;

    for file in ["kickstart", "user-data", "meta-data"] {
        let resp = reqwest::get(format!("http://{}/{TEST_PREFIX}/nope/{file}", server.addr))
            .await
            .expect("request");
        assert_eq!(resp.status(), 404, "{file} should 404 when folder missing");
    }

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn missing_template_folder_returns_404_for_index() {
    let server = server_with_templates().await;

    let resp = reqwest::get(format!("http://{}/{TEST_PREFIX}/nope/", server.addr))
        .await
        .expect("request");
    assert_eq!(resp.status(), 404);

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn existing_folder_missing_file_returns_empty_200() {
    let server = server_with_templates().await;
    make_template_dir(&server, "ubuntu");

    for file in ["kickstart", "user-data", "meta-data"] {
        let resp = reqwest::get(format!(
            "http://{}/{TEST_PREFIX}/ubuntu/{file}",
            server.addr
        ))
        .await
        .expect("request");
        assert_eq!(resp.status(), 200, "{file} should 200");
        assert_eq!(
            resp.text().await.expect("body"),
            "",
            "{file} should be empty"
        );
    }

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn serves_each_file_when_present() {
    let server = server_with_templates().await;
    write_template_file(&server, "ubuntu", "kickstart", "kickstart-body");
    write_template_file(
        &server,
        "ubuntu",
        "user-data",
        "#cloud-config\nautoinstall: {}\n",
    );
    write_template_file(&server, "ubuntu", "meta-data", "instance-id: ubuntu-1\n");

    let cases = [
        ("kickstart", "kickstart-body"),
        ("user-data", "#cloud-config\nautoinstall: {}\n"),
        ("meta-data", "instance-id: ubuntu-1\n"),
    ];
    for (file, expected) in cases {
        let resp = reqwest::get(format!(
            "http://{}/{TEST_PREFIX}/ubuntu/{file}",
            server.addr
        ))
        .await
        .expect("request");
        assert_eq!(resp.status(), 200, "{file} status");
        assert_eq!(
            resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
            "text/plain; charset=utf-8",
            "{file} content-type"
        );
        assert_eq!(resp.text().await.expect("body"), expected, "{file} body");
    }

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn renders_query_vars_in_template_files() {
    let server = server_with_templates().await;
    write_template_file(
        &server,
        "ubuntu",
        "user-data",
        "hostname: {{h}}\ninstance-id: {{id}}\nmissing: {{missing}}\n",
    );

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/user-data?h=node1.example&id=10",
        server.addr
    ))
    .await
    .expect("request");

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.text().await.expect("body"),
        "hostname: node1.example\ninstance-id: 10\nmissing: {{missing}}\n"
    );

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn renders_path_vars_for_autoinstall_file_urls() {
    let server = server_with_templates().await;
    write_template_file(&server, "ubuntu", "user-data", "hostname: {{h}}\n");
    write_template_file(&server, "ubuntu", "meta-data", "instance-id: {{id}}\n");

    let user_data = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/h=node1.example;id=10/user-data",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(user_data.status(), 200);
    assert_eq!(
        user_data.text().await.expect("body"),
        "hostname: node1.example\n"
    );

    let meta_data = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/h=node1.example;id=10/meta-data",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(meta_data.status(), 200);
    assert_eq!(meta_data.text().await.expect("body"), "instance-id: 10\n");

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn rejects_invalid_template_var_names() {
    let server = server_with_templates().await;
    write_template_file(&server, "ubuntu", "user-data", "hostname: {{host_name}}\n");

    let query_resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/user-data?host_name=node1",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(query_resp.status(), 404);

    let path_resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/host_name=node1/user-data",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(path_resp.status(), 404);

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn empty_var_value_renders_empty_string() {
    let server = server_with_templates().await;
    write_template_file(
        &server,
        "ubuntu",
        "user-data",
        "hostname: {{h}}\nid: {{id}}\n",
    );

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/user-data?h=&id=10",
        server.addr
    ))
    .await
    .expect("request");

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.expect("body"), "hostname: \nid: 10\n");

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn rejects_path_vars_without_equals() {
    let server = server_with_templates().await;
    write_template_file(&server, "ubuntu", "user-data", "hostname: {{h}}\n");

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/garbage/user-data",
        server.addr
    ))
    .await
    .expect("request");

    assert_eq!(resp.status(), 404);

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn rejects_newline_var_values() {
    let server = server_with_templates().await;
    write_template_file(&server, "ubuntu", "user-data", "hostname: {{h}}\n");

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/user-data?h=node1%0Aruncmd:%20[echo%20hi]",
        server.addr
    ))
    .await
    .expect("request");

    assert_eq!(resp.status(), 404);

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn query_vars_override_path_vars() {
    let server = server_with_templates().await;
    write_template_file(&server, "ubuntu", "user-data", "hostname: {{h}}\n");

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/h=path/user-data?h=query",
        server.addr
    ))
    .await
    .expect("request");

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.expect("body"), "hostname: query\n");

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn values_containing_placeholders_are_not_rendered_again() {
    let server = server_with_templates().await;
    write_template_file(&server, "ubuntu", "user-data", "{{h}}\n");

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/user-data?h={{{{id}}}}&id=foo",
        server.addr
    ))
    .await
    .expect("request");

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.expect("body"), "{{id}}\n");

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn index_returns_html_with_three_links() {
    let server = server_with_templates().await;
    make_template_dir(&server, "rhel9");

    let resp = reqwest::get(format!("http://{}/{TEST_PREFIX}/rhel9/", server.addr))
        .await
        .expect("request");
    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(ct.starts_with("text/html"), "content-type: {ct}");
    let body = resp.text().await.expect("body");
    assert!(body.contains("rhel9"), "body missing template name: {body}");
    assert!(
        body.contains("href=\"kickstart\""),
        "body missing kickstart link"
    );
    assert!(
        body.contains("href=\"user-data\""),
        "body missing user-data link"
    );
    assert!(
        body.contains("href=\"meta-data\""),
        "body missing meta-data link"
    );

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn rejects_uppercase_template_name() {
    let server = server_with_templates().await;
    make_template_dir(&server, "ubuntu");

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/Ubuntu/kickstart",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(resp.status(), 404);

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn rejects_dot_template_name() {
    let server = server_with_templates().await;

    let resp = reqwest::get(format!("http://{}/{TEST_PREFIX}/../kickstart", server.addr))
        .await
        .expect("request");
    assert!(
        resp.status() == 404 || resp.status() == 401,
        "expected 404 or 401, got {}",
        resp.status()
    );

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn unknown_file_under_valid_template_returns_unauthorized() {
    let server = server_with_templates().await;
    make_template_dir(&server, "ubuntu");

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/ubuntu/something-else",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(resp.status(), 401);

    let _ = server.shutdown.send(());
}

#[tokio::test]
async fn template_path_without_trailing_slash_returns_unauthorized() {
    let server = server_with_templates().await;
    make_template_dir(&server, "ubuntu");

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let resp = client
        .get(format!("http://{}/{TEST_PREFIX}/ubuntu", server.addr))
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status(), 401);

    let _ = server.shutdown.send(());
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_leaf_file_pointing_outside_returns_empty() {
    let outer = tempfile::tempdir().unwrap();
    let templates = outer.path().join("templates");
    std::fs::create_dir(&templates).unwrap();
    let template_dir = templates.join("sneaky");
    std::fs::create_dir(&template_dir).unwrap();
    // Target lives outside templates_dir but inside the outer tempdir.
    let secret = outer.path().join("secret.txt");
    std::fs::write(&secret, "TOP SECRET CONTENTS").unwrap();
    std::os::unix::fs::symlink(&secret, template_dir.join("kickstart")).unwrap();

    let server = spawn(templates, outer).await;

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/sneaky/kickstart",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert_eq!(
        body, "",
        "symlinked-out leaf must return empty body, got: {body:?}"
    );

    let _ = server.shutdown.send(());
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_template_dir_escaping_returns_404() {
    let outer = tempfile::tempdir().unwrap();
    let templates = outer.path().join("templates");
    std::fs::create_dir(&templates).unwrap();
    // Outside templates_dir but inside the outer tempdir.
    let outside_dir = outer.path().join("outside-templates");
    std::fs::create_dir(&outside_dir).unwrap();
    std::fs::write(outside_dir.join("user-data"), "OUTSIDE DATA").unwrap();
    // templates/escaped -> ../outside-templates
    std::os::unix::fs::symlink(&outside_dir, templates.join("escaped")).unwrap();

    let server = spawn(templates, outer).await;

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/escaped/user-data",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(
        resp.status(),
        404,
        "escaping template dir must 404 for file access"
    );

    let resp = reqwest::get(format!("http://{}/{TEST_PREFIX}/escaped/", server.addr))
        .await
        .expect("request");
    assert_eq!(
        resp.status(),
        404,
        "escaping template dir must 404 for index"
    );

    let _ = server.shutdown.send(());
}

#[cfg(unix)]
#[tokio::test]
async fn leaf_path_that_is_a_directory_returns_500() {
    let outer = tempfile::tempdir().unwrap();
    let templates = outer.path().join("templates");
    std::fs::create_dir(&templates).unwrap();
    let tdir = templates.join("weird");
    std::fs::create_dir(&tdir).unwrap();
    // Leaf is a directory, not a regular file. canonicalize succeeds; read
    // fails with EISDIR (non-NotFound) so the 500 path is exercised.
    std::fs::create_dir(tdir.join("kickstart")).unwrap();

    let server = spawn(templates, outer).await;
    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/weird/kickstart",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(resp.status(), 500);

    let _ = server.shutdown.send(());
}

#[cfg(unix)]
#[tokio::test]
async fn self_referencing_leaf_symlink_returns_500() {
    let outer = tempfile::tempdir().unwrap();
    let templates = outer.path().join("templates");
    std::fs::create_dir(&templates).unwrap();
    let tdir = templates.join("loopy");
    std::fs::create_dir(&tdir).unwrap();
    // kickstart -> kickstart (relative). canonicalize returns ELOOP, which is
    // not NotFound, so the canonicalize-error 500 path is exercised.
    std::os::unix::fs::symlink("kickstart", tdir.join("kickstart")).unwrap();

    let server = spawn(templates, outer).await;
    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/loopy/kickstart",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(resp.status(), 500);

    let _ = server.shutdown.send(());
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_template_dir_inside_root_still_works() {
    let outer = tempfile::tempdir().unwrap();
    let templates = outer.path().join("templates");
    std::fs::create_dir(&templates).unwrap();
    std::fs::create_dir(templates.join("ubuntu-24-04")).unwrap();
    std::fs::write(templates.join("ubuntu-24-04/kickstart"), "real kickstart").unwrap();
    // Relative symlink kept inside templates_dir — a legitimate ops pattern
    // (e.g., `latest -> ubuntu-24-04`).
    std::os::unix::fs::symlink("ubuntu-24-04", templates.join("latest")).unwrap();

    let server = spawn(templates, outer).await;

    let resp = reqwest::get(format!(
        "http://{}/{TEST_PREFIX}/latest/kickstart",
        server.addr
    ))
    .await
    .expect("request");
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.expect("body"), "real kickstart");

    let _ = server.shutdown.send(());
}

// The README points users at `examples/templates/example/`. This test asserts the
// claim still holds: pointing `templates_dir` at the bundled examples directory
// (relative to crate root, which is the cargo test CWD) actually serves the files.
#[tokio::test]
async fn bundled_example_template_serves_through_cloudseeder() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let (tx, rx) = oneshot::channel::<()>();

    let templates_dir = PathBuf::from("examples/templates");
    tokio::spawn(async move {
        cloudseeder::serve_with_shutdown(listener, TEST_PREFIX, templates_dir, async {
            let _ = rx.await;
        })
        .await
        .expect("server error");
    });

    let cases = [
        ("kickstart", "Placeholder kickstart"),
        ("user-data", "#cloud-config"),
        ("meta-data", "instance-id"),
    ];
    for (file, needle) in cases {
        let resp = reqwest::get(format!("http://{addr}/{TEST_PREFIX}/example/{file}"))
            .await
            .expect("request");
        assert_eq!(resp.status(), 200, "{file} status");
        let body = resp.text().await.expect("body");
        assert!(
            body.contains(needle),
            "{file} body missing {needle:?}: {body}"
        );
    }

    let resp = reqwest::get(format!("http://{addr}/{TEST_PREFIX}/example/"))
        .await
        .expect("request");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("href=\"kickstart\""));

    let _ = tx.send(());
}
