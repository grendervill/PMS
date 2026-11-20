//! Проверка, что сервер не буферизует сейф и вложения целиком.
//!
//! Считаем не число аллокаций (оно шумное), а самый крупный разовый запрос
//! памяти: если кто-то начнёт читать тело в `Vec`, тест упадёт сразу.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use pms_proto::identity::AccountId;
use pms_server::store::vault::ExpectedVersion;
use pms_server::store::Store;
use tokio::io::AsyncRead;

struct Tracking;

static TRACK: AtomicBool = AtomicBool::new(false);
static MAX_ALLOC: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            MAX_ALLOC.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            MAX_ALLOC.fetch_max(new_size, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Tracking = Tracking;

/// Отдаёт `total` байт кусками, не выделяя буфер под весь объём.
struct Generated {
    remaining: usize,
}

impl AsyncRead for Generated {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let want = buf.remaining().min(self.remaining).min(32 * 1024);
        if want == 0 {
            return std::task::Poll::Ready(Ok(()));
        }
        buf.put_slice(&vec![0xA5u8; want]);
        self.remaining -= want;
        std::task::Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn large_uploads_are_streamed_not_buffered() {
    const TOTAL: usize = 16 * 1024 * 1024;

    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), 64 * 1024 * 1024).unwrap();
    let account = AccountId::new([0x51; 16]);
    store
        .create_account(account, [1u8; 32], 64 * 1024 * 1024, "alice")
        .unwrap();

    MAX_ALLOC.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let result = store
        .put_vault(
            account,
            ExpectedVersion::Any,
            Generated { remaining: TOTAL },
            TOTAL as u64,
        )
        .await;
    TRACK.store(false, Ordering::Relaxed);
    result.unwrap();

    let largest = MAX_ALLOC.load(Ordering::Relaxed);
    assert!(
        largest <= 1024 * 1024,
        "загрузка {TOTAL} байт потребовала разовой аллокации {largest} байт — тело буферизуется"
    );
}

#[tokio::test]
async fn registration_body_is_staged_not_buffered() {
    const TOTAL: usize = 16 * 1024 * 1024;

    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), 64 * 1024 * 1024).unwrap();

    MAX_ALLOC.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let staged = store
        .stage_upload(Generated { remaining: TOTAL }, TOTAL as u64)
        .await;
    TRACK.store(false, Ordering::Relaxed);
    let staged = staged.unwrap();

    assert_eq!(staged.len(), TOTAL as u64);
    assert!(!staged.is_empty());
    let largest = MAX_ALLOC.load(Ordering::Relaxed);
    assert!(
        largest <= 1024 * 1024,
        "регистрация {TOTAL} байт потребовала разовой аллокации {largest} байт"
    );
}

#[tokio::test]
async fn blob_uploads_use_a_fixed_buffer() {
    const TOTAL: usize = 16 * 1024 * 1024;
    const BLOB_ID: &str = "0b7c1d2e-3f40-4516-8798-a9bacbdcedfe";

    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), 64 * 1024 * 1024).unwrap();
    let account = AccountId::new([0x52; 16]);
    store
        .create_account(account, [1u8; 32], 64 * 1024 * 1024, "alice")
        .unwrap();

    let mut hasher = blake3::Hasher::new();
    let pattern = vec![0xA5u8; 32 * 1024];
    for _ in 0..(TOTAL / pattern.len()) {
        hasher.update(&pattern);
    }
    let digest = hasher.finalize().to_hex().to_string();

    MAX_ALLOC.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let result = store
        .put_blob(
            account,
            BLOB_ID,
            &digest,
            Generated { remaining: TOTAL },
            TOTAL as u64,
        )
        .await;
    TRACK.store(false, Ordering::Relaxed);
    result.unwrap();

    let largest = MAX_ALLOC.load(Ordering::Relaxed);
    assert!(
        largest <= 1024 * 1024,
        "блоб {TOTAL} байт потребовал разовой аллокации {largest} байт"
    );
}
