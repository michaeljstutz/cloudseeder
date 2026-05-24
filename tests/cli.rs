use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_cloudseeder");

#[test]
fn version_flag_prints_pkg_version() {
    let output = Command::new(BIN).arg("--version").output().expect("spawn");
    assert!(
        output.status.success(),
        "non-zero exit: {:?}",
        output.status
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "version output missing pkg version: {stdout}"
    );
}

#[test]
fn help_flag_lists_config_option() {
    let output = Command::new(BIN).arg("--help").output().expect("spawn");
    assert!(
        output.status.success(),
        "non-zero exit: {:?}",
        output.status
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--config"),
        "help missing --config: {stdout}"
    );
    assert!(
        stdout.contains("CLOUDSEEDER_CONFIG"),
        "help missing CLOUDSEEDER_CONFIG: {stdout}"
    );
    assert!(stdout.contains("render"), "help missing render: {stdout}");
}

#[test]
fn render_command_prints_rendered_template_file() {
    let dir = tempfile::tempdir().unwrap();
    let templates_dir = dir.path().join("templates");
    let template_dir = templates_dir.join("ubuntu");
    std::fs::create_dir_all(&template_dir).unwrap();
    std::fs::write(
        template_dir.join("user-data"),
        "hostname: {{h}}\ninstance-id: {{id}}\nmissing: {{missing}}\n",
    )
    .unwrap();
    let config_path = dir.path().join("cloudseeder.toml");
    std::fs::write(
        &config_path,
        format!(
            "prefix = \"clitest\"\ntemplates_dir = \"{}\"\n",
            templates_dir.display()
        ),
    )
    .unwrap();

    let output = Command::new(BIN)
        .arg("--config")
        .arg(&config_path)
        .args([
            "render",
            "ubuntu",
            "user-data",
            "--var",
            "h=node1.example",
            "--var",
            "id=10",
        ])
        .output()
        .expect("spawn");

    assert!(
        output.status.success(),
        "non-zero exit: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "hostname: node1.example\ninstance-id: 10\nmissing: {{missing}}\n"
    );
}

#[test]
fn render_command_rejects_invalid_var_argument() {
    let output = Command::new(BIN)
        .args(["render", "ubuntu", "user-data", "--var", "not-a-pair"])
        .output()
        .expect("spawn");

    assert!(!output.status.success(), "binary should exit non-zero");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("expected KEY=VALUE"),
        "stderr missing var error: {stderr}"
    );
}

#[test]
fn render_command_rejects_invalid_var_name() {
    let output = Command::new(BIN)
        .args(["render", "ubuntu", "user-data", "--var", "Bad=x"])
        .output()
        .expect("spawn");

    assert!(!output.status.success(), "binary should exit non-zero");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid variable name"),
        "stderr missing var-name error: {stderr}"
    );
}

#[test]
fn invalid_config_exits_with_code_2() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("cloudseeder.toml");
    std::fs::write(&config_path, "addr = \"not-a-real-addr\"\n").unwrap();

    let output = Command::new(BIN)
        .arg("--config")
        .arg(&config_path)
        .env("RUST_LOG", "error")
        .output()
        .expect("spawn");

    assert!(!output.status.success(), "binary should exit non-zero");
    let code = output.status.code().expect("exit code");
    assert_eq!(code, 2, "expected exit code 2 for config error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid addr"),
        "stderr missing addr error: {stderr}"
    );
}

fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

#[cfg(unix)]
#[test]
fn binary_starts_and_shuts_down_on_sigterm() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("cloudseeder.toml");
    let port = pick_free_port();
    std::fs::write(
        &config_path,
        format!("addr = \"127.0.0.1:{port}\"\nprefix = \"clitest\"\n"),
    )
    .unwrap();

    let mut child = Command::new(BIN)
        .arg("--config")
        .arg(&config_path)
        .env("RUST_LOG", "error")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");

    // Poll the port until the server accepts connections.
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("server did not start within 5s");
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // SIGTERM via shell-out to avoid pulling in libc.
    let kill_status = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("spawn kill");
    assert!(kill_status.success(), "kill failed: {kill_status:?}");

    // Poll for child exit. std::process::Child has no built-in timed wait.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                assert!(status.success(), "non-zero exit after SIGTERM: {status:?}");
                return;
            }
            Ok(None) => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    panic!("child did not exit within 5s after SIGTERM");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("wait failed: {e}"),
        }
    }
}
