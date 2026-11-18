#![forbid(unsafe_code)]
//! Сервер синхронизации PMS: хранит запечатанный сейф и запечатанные вложения,
//! не разбирая их содержимое.
//!
//! Слои: `config` (TOML и валидация), `tls` (режимы сертификатов), `store`
//! (файлы и SQLite-индекс), `auth` (nonce и сессии), `api` (HTTP).

pub mod api;
pub mod auth;
pub mod config;
pub mod server;
pub mod store;
pub mod tls;

pub use server::{run, ServerError, ServerHandle};
