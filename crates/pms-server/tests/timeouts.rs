//! Дедлайны: недописанные заголовки и бесконечное тело не должны держать сервер.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use pms_server::config::Config;

async fn start(extra: &str) -> pms_server::ServerHandle {
    let dir = tempfile::tempdir().expect("каталог");
    let text = format!(
        "[server]\ndata_dir = \"{}\"\nlisten = \"127.0.0.1:0\"\n[tls]\nrequired = false\nmode = \"off\"\n{extra}",
        dir.path().display()
    );
    let config = Config::parse(&text).expect("конфиг");
    let handle = pms_server::run(config).await.expect("сервер");
    // Каталог должен жить, пока сервер работает: держим его до конца теста.
    std::mem::forget(dir);
    handle
}

fn connect(handle: &pms_server::ServerHandle) -> TcpStream {
    let stream = TcpStream::connect(handle.addr()).expect("соединение");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("таймаут чтения");
    stream
}

/// Заголовки не закончены: сервер обязан закрыть соединение, а не ждать вечно.
#[tokio::test(flavor = "multi_thread")]
async fn unfinished_headers_are_cut_by_the_header_timeout() {
    let server = start("[limits]\nheader_read_timeout_secs = 1\n").await;
    let mut stream = connect(&server);
    stream
        .write_all(b"GET /v1/healthz HTTP/1.1\r\nHost: pms\r\n")
        .expect("частичные заголовки");

    let started = Instant::now();
    let mut buffer = [0u8; 256];
    let read = stream.read(&mut buffer);
    let elapsed = started.elapsed();

    assert!(read.is_ok(), "соединение должно закрыться, а не зависнуть");
    assert!(
        elapsed < Duration::from_secs(6),
        "закрытие заняло {elapsed:?}: дедлайн заголовков не работает"
    );
    server.shutdown();
}

/// Тело обещано, но не приходит: запрос обрывается по бюджету, а не висит.
#[tokio::test(flavor = "multi_thread")]
async fn stalled_body_is_cut_by_the_body_budget() {
    let server = start("[limits]\nbody_timeout_secs = 1\nheader_read_timeout_secs = 2\n").await;
    let mut stream = connect(&server);
    stream
        .write_all(
            b"PUT /v1/vault HTTP/1.1\r\nHost: pms\r\nx-pms-protocol: 1\r\nif-match: *\r\n\
              content-length: 4096\r\n\r\n",
        )
        .expect("заголовки");

    let started = Instant::now();
    let mut buffer = [0u8; 512];
    let read = stream.read(&mut buffer);
    let elapsed = started.elapsed();

    assert!(read.is_ok());
    assert!(
        elapsed < Duration::from_secs(8),
        "тело держало запрос {elapsed:?}: общего бюджета нет"
    );
    server.shutdown();
}
