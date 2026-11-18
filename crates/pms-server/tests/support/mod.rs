#![allow(dead_code)]

use std::sync::Arc;

use pms_server::api::{router, AppState};
use pms_server::auth::Auth;
use pms_server::config::Config;
use pms_server::store::Store;

/// Поднимает настоящее HTTP-приложение на случайном порту loopback.
pub struct TestServer {
    pub base_url: String,
    pub client: reqwest::Client,
    pub store: Arc<Store>,
    pub config: Arc<Config>,
    _dir: tempfile::TempDir,
    task: tokio::task::JoinHandle<()>,
}

impl TestServer {
    pub async fn start(extra: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let text = format!(
            "[server]\ndata_dir = \"{}\"\nlisten = \"127.0.0.1:0\"\n{extra}",
            dir.path().display()
        );
        let config = Arc::new(Config::parse(&text).unwrap());
        config.validate().unwrap();

        let store = Arc::new(Store::open(dir.path(), config.quota.default_bytes).unwrap());
        let auth = Arc::new(Auth::new(Arc::clone(&store), config.auth.clone()));
        let app = router(AppState::new(Arc::clone(&store), auth, Arc::clone(&config)));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self {
            base_url: format!("http://{address}"),
            client: reqwest::Client::new(),
            store,
            config,
            _dir: dir,
            task,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
