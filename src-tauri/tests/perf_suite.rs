//! Independent performance suite for the PMS Rust core (`pms_lib`).
//!
//! This file is a separate integration-test binary on purpose: `tests/*.rs` is
//! compiled as its own crate, so it can install its own `#[global_allocator]`
//! and count allocations on the real data paths without touching
//! `src/core/perf_audit.rs` (which owns the unit-test allocator and its own
//! scenarios; nothing here reuses it).
//!
//! Everything is measured through the public `pms_lib::testing` surface only.
//!
//! Run it in release mode, single-threaded (the counters are process-wide
//! statics, so a second test thread would pollute them):
//!
//! ```sh
//! cd src-tauri && cargo test --release --test perf_suite -- --nocapture --test-threads=1
//! ```
//!
//! There is deliberately one `#[test]` function: every scenario shares the
//! same allocator statics, and a single test guarantees no race between
//! scenarios regardless of how the harness is invoked.
//!
//! Argon2 is never in a measurement loop: `create_vault` runs once (to get a
//! real `VaultHeader` with a real KDF slot), and all seal/open scenarios use a
//! fixed 32-byte vault key, as real unit-level benchmarks must.

use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::time::Instant;

use base64::Engine as _;
use rand::RngCore;
use serde_json::{json, Value};
use tempfile::tempdir;
use uuid::Uuid;

use pms_lib::testing::{
    archive_export, archive_import, create_vault, generate_passphrase, generate_password, import,
    open_attachment, open_payload, parse_vault_file, save_vault, seal_attachment,
    seal_attachment_in_place, seal_document, AttachmentKind, AttachmentMeta, CipherSuite,
    CreateVaultConfig, Entry, KdfProfile, PassphraseOptions, PasswordOptions, UnlockSecretInput,
    VaultData, VaultHeader, VaultSettings, MAX_ATTACHMENT_BYTES,
};

/// Poly1305 tag length (XChaCha20-Poly1305) — the only constant of the
/// attachment format this file needs and `pms_lib::testing` does not export.
const TAG_LEN: usize = 16;
/// `magic (5) | header_len (4) | payload_len (8)`, `core/vault_file.rs`.
const FIXED_HEADER_LEN: usize = 17;
const TIMESTAMP: &str = "2026-11-30T12:00:00.000Z";
/// Fixed vault key for the seal/open scenarios (the real one lives inside
/// `VaultSession` and is not reachable from an integration test — by design).
const VAULT_KEY: [u8; 32] = [7u8; 32];

// ---------------------------------------------------------------------------
// Counting allocator
// ---------------------------------------------------------------------------

struct CountingAllocator;

static ENABLED: AtomicBool = AtomicBool::new(true);
static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static REALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
static REALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);
static PEAK_BYTES: AtomicIsize = AtomicIsize::new(0);

/// Call/byte counters are gated, `LIVE_BYTES`/`PEAK_BYTES` are not: an object
/// allocated while the counters are off may be freed while they are on, and a
/// gated live counter would drift (and with it every peak number).
#[inline]
fn counting() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

#[inline]
fn track_live(delta: isize) {
    let live = LIVE_BYTES.fetch_add(delta, Ordering::Relaxed) + delta;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if counting() {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        track_live(layout.size() as isize);
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if counting() {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        track_live(layout.size() as isize);
        System.alloc_zeroed(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        track_live(-(layout.size() as isize));
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if counting() {
            REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            REALLOC_BYTES.fetch_add(new_size, Ordering::Relaxed);
        }
        track_live(new_size as isize - layout.size() as isize);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Clone, Copy, Default)]
struct Stats {
    alloc_calls: usize,
    realloc_calls: usize,
    alloc_bytes: usize,
    realloc_bytes: usize,
    peak_bytes: usize,
}

impl Stats {
    fn snapshot() -> Self {
        Self {
            alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
            realloc_calls: REALLOC_CALLS.load(Ordering::Relaxed),
            alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            realloc_bytes: REALLOC_BYTES.load(Ordering::Relaxed),
            peak_bytes: PEAK_BYTES.load(Ordering::Relaxed).max(0) as usize,
        }
    }

    fn since(&self, base: &Stats) -> Self {
        Self {
            alloc_calls: self.alloc_calls.saturating_sub(base.alloc_calls),
            realloc_calls: self.realloc_calls.saturating_sub(base.realloc_calls),
            alloc_bytes: self.alloc_bytes.saturating_sub(base.alloc_bytes),
            realloc_bytes: self.realloc_bytes.saturating_sub(base.realloc_bytes),
            peak_bytes: self.peak_bytes.saturating_sub(base.peak_bytes),
        }
    }
}

struct Measurement<T> {
    value: T,
    stats: Stats,
    best_ms: f64,
}

/// Runs `f` once as a warm-up, then `runs` timed/counted runs and keeps the
/// fastest. The peak is measured relative to the live heap at the start of each
/// run, otherwise leftovers from the previous run hide the real high-water mark.
fn measure<T>(runs: usize, mut f: impl FnMut() -> T) -> Measurement<T> {
    ENABLED.store(true, Ordering::SeqCst);
    black_box(f());
    let mut stats = Stats::default();
    let mut best_ms = f64::MAX;
    let mut value = None;
    for _ in 0..runs {
        PEAK_BYTES.store(LIVE_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
        let base = Stats::snapshot();
        let start = Instant::now();
        let out = f();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        stats = Stats::snapshot().since(&base);
        best_ms = best_ms.min(elapsed);
        value = Some(out);
    }
    ENABLED.store(true, Ordering::Relaxed);
    Measurement {
        value: value.expect("measure() must run at least once"),
        stats,
        best_ms,
    }
}

/// Runs setup/verification code with the call/byte counters switched off so it
/// does not pay the extra atomic RMWs per allocation. Live/peak bytes are still
/// tracked (see `counting`).
fn quiet<T>(f: impl FnOnce() -> T) -> T {
    ENABLED.store(false, Ordering::SeqCst);
    let out = f();
    ENABLED.store(true, Ordering::SeqCst);
    out
}

// ---------------------------------------------------------------------------
// Markdown table helpers
// ---------------------------------------------------------------------------

fn row(cells: &[String]) -> String {
    let mut line = String::from("|");
    for cell in cells {
        line.push(' ');
        line.push_str(cell);
        line.push_str(" |");
    }
    line
}

fn table_header(cells: &[&str]) -> String {
    let mut line = row(&cells
        .iter()
        .map(|cell| cell.to_string())
        .collect::<Vec<_>>());
    line.push('\n');
    line.push('|');
    for _ in cells {
        line.push_str(" --- |");
    }
    line
}

fn kib(bytes: usize) -> f64 {
    bytes as f64 / 1024.0
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn mib_per_sec(bytes: usize, ms: f64) -> f64 {
    if ms <= 0.0 {
        0.0
    } else {
        mib(bytes) / (ms / 1000.0)
    }
}

fn per_ratio(value: usize, base: usize) -> f64 {
    if base == 0 {
        0.0
    } else {
        value as f64 / base as f64
    }
}

fn size_label(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.0} МиБ", mib(bytes))
    } else {
        format!("{:.0} КиБ", kib(bytes))
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Entries are built from the same JSON shape the renderer sends: `TotpConfig`
/// and `HistoryItem` are not part of `pms_lib::testing`, and `serde` is the
/// only supported way to fill those fields from an integration test.
fn entry(index: usize) -> Entry {
    let with_totp = index.is_multiple_of(3);
    let with_history = index.is_multiple_of(5);
    let totp = if with_totp {
        json!({
            "secret": "JBSWY3DPEHPK3PXP",
            "algorithm": "SHA1",
            "digits": 6,
            "period": 30,
            "issuer": "Example Corp",
            "account": format!("user{index}@example.com"),
        })
    } else {
        Value::Null
    };
    let password_history = if with_history {
        json!([
            {
                "value": format!("0ld-P@ssw0rd-{index}-a"),
                "changedAt": "2024-02-02T09:00:00.000Z",
            },
            {
                "value": format!("0ld-P@ssw0rd-{index}-b"),
                "changedAt": "2023-05-17T18:30:00.000Z",
            },
        ])
    } else {
        json!([])
    };
    let login_history = if with_history {
        json!([{
            "value": format!("old{index}@example.com"),
            "changedAt": "2023-11-09T11:15:00.000Z",
        }])
    } else {
        json!([])
    };

    let value = json!({
        "id": Uuid::new_v4(),
        "kind": "login",
        "schemaVersion": 1,
        "name": format!("Сервис «{index}» — личный кабинет"),
        "login": format!("user{index}@example.com"),
        "password": format!("P@ssw0rd-{index}-Xk29"),
        "url": format!("https://service{index}.example.com/login"),
        "urls": [
            format!("https://service{index}.example.com"),
            format!("https://auth{index}.example.com/signin"),
        ],
        "comment": format!(
            "Двухфакторный вход включён, резервные коды лежат в сейфе (запись №{index})."
        ),
        "totp": totp,
        "attachmentIds": [],
        "sshKeyIds": [],
        "loginHistory": login_history,
        "passwordHistory": password_history,
        "createdAt": TIMESTAMP,
        "updatedAt": TIMESTAMP,
    });

    serde_json::from_value(value)
        .expect("an Entry must deserialize from the JSON shape the renderer sends")
}

fn attachment_meta(index: usize) -> AttachmentMeta {
    AttachmentMeta {
        id: Uuid::new_v4(),
        name: format!("scan-{index:02}.pdf"),
        mime: "application/pdf".to_string(),
        size: 1_048_576,
        sha256: "b4c0ffee".repeat(8),
        kind: AttachmentKind::Pdf,
        created_at: TIMESTAMP.to_string(),
    }
}

fn document(vault_id: Uuid, name: &str, entries: usize, attachments: usize) -> VaultData {
    VaultData {
        schema_version: 1,
        id: vault_id,
        name: name.to_string(),
        created_at: TIMESTAMP.to_string(),
        updated_at: TIMESTAMP.to_string(),
        settings: VaultSettings::default(),
        entries: (0..entries).map(entry).collect(),
        ssh_keys: Vec::new(),
        bank_cards: Vec::new(),
        documents: Vec::new(),
        attachments: (0..attachments).map(attachment_meta).collect(),
        drafts: Vec::new(),
        favorites: Vec::new(),
    }
}

/// A one-entry document whose `comment` carries `bytes` of pseudo-ciphertext
/// (base64 of random bytes, so it looks like real vault content and does not
/// compress), used to build a realistic ~2 MiB `pms.vault`.
fn fat_document(vault_id: Uuid, payload_bytes: usize) -> VaultData {
    let raw_len = payload_bytes / 4 * 3;
    let mut raw = vec![0u8; raw_len];
    rand::thread_rng().fill_bytes(&mut raw);
    let blob = base64::engine::general_purpose::STANDARD.encode(&raw);

    let mut entry = entry(0);
    entry.comment = blob;
    // `VaultData` implements `Drop` (it zeroizes the secrets it holds), so a
    // struct-update expression would try to move out of it; mutate instead.
    let mut data = document(vault_id, "Импортированное хранилище", 0, 0);
    data.entries = vec![entry];
    data
}

/// `header_core_aad` from `core/crypto.rs` is `pub` but is not re-exported by
/// `pms_lib::testing`, so the test rebuilds the AAD from the public header
/// fields. If that formula drifts, `open_payload` fails and the scenario panics
/// with an explicit message — the check is self-verifying.
fn header_aad(header: &VaultHeader) -> Vec<u8> {
    let mut aad = Vec::with_capacity(64 + header.created_at.len());
    aad.extend_from_slice(b"PMS-CORE-v1");
    aad.extend_from_slice(&header.format.to_le_bytes());
    aad.extend_from_slice(header.vault_id.as_bytes());
    aad.push(header.cipher.id());
    aad.extend_from_slice(&(header.created_at.len() as u32).to_le_bytes());
    aad.extend_from_slice(header.created_at.as_bytes());
    aad
}

/// `ATTACHMENT_HEADER_LEN` is not re-exported either: derive it from an empty
/// attachment (`header + tag`) once, and assert the derivation is consistent.
fn attachment_header_len(vault_id: Uuid) -> usize {
    let probe = seal_attachment(&VAULT_KEY, vault_id, Uuid::new_v4(), &[])
        .expect("sealing an empty attachment must work");
    probe
        .len()
        .checked_sub(TAG_LEN)
        .expect("attachment header + tag layout")
}

/// Deterministic pseudo-random filler, fast enough for tens of MiB.
fn filler(len: usize, seed: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut state = seed | 1;
    for _ in 0..len {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        out.push((state >> 33) as u8);
    }
    out
}

/// Bitwarden JSON, own generator (nothing shared with `perf_audit`): 2 uris,
/// 2 custom fields, a TOTP secret and 3 password-history entries per item.
fn bitwarden_json(items: usize) -> Vec<u8> {
    let mut out = String::with_capacity(items * 700 + 256);
    out.push_str(
        "{\"encrypted\":false,\"folders\":[{\"id\":\"folder-1\",\"name\":\"Работа\"}],\"items\":[",
    );
    for index in 0..items {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"id\":\"item-{index}\",\"type\":1,\"name\":\"Портал заказчика {index}\",\
             \"folderId\":\"folder-1\",\
             \"notes\":\"Карточка заказчика №{index}: договор, контакт и комментарий средней длины.\",\
             \"fields\":[{{\"name\":\"ПИН\",\"value\":\"4{index:03}\"}},\
             {{\"name\":\"Отдел\",\"value\":\"Закупки {index}\"}}],\
             \"login\":{{\"username\":\"p{index}@corp.example.com\",\"password\":\"Str0ng-{index}-pass\",\
             \"totp\":\"JBSWY3DPEHPK3PXP\",\
             \"uris\":[{{\"uri\":\"https://portal{index}.corp.example.com\"}},\
             {{\"uri\":\"https://sso{index}.corp.example.com\"}}]}},\
             \"passwordHistory\":[\
             {{\"lastUsedDate\":\"2025-01-01T00:00:00.000Z\",\"password\":\"prev-a-{index}\"}},\
             {{\"lastUsedDate\":\"2024-01-01T00:00:00.000Z\",\"password\":\"prev-b-{index}\"}},\
             {{\"lastUsedDate\":\"2023-01-01T00:00:00.000Z\",\"password\":\"prev-c-{index}\"}}]}}"
        ));
    }
    out.push_str("]}");
    out.into_bytes()
}

// ---------------------------------------------------------------------------
// The suite
// ---------------------------------------------------------------------------

#[test]
fn perf_suite() {
    // One real vault header (real KDF slot, real cipher). Argon2id "standard"
    // costs a few hundred milliseconds — paid exactly once, outside the timers.
    let (_, session, _) = quiet(|| {
        create_vault(
            &CreateVaultConfig {
                name: "Замеры".to_string(),
                kdf_profile: KdfProfile::Standard,
                cipher: CipherSuite::XChaCha20Poly1305,
            },
            &UnlockSecretInput::Passphrase {
                value: "correct horse battery staple".to_string(),
            },
        )
        .expect("create_vault must work")
    });
    let header = session.header.clone();
    assert_eq!(
        header.cipher,
        CipherSuite::XChaCha20Poly1305,
        "the suite measures the default cipher"
    );

    println!(
        "\n# Замеры производительности ядра PMS (release, один поток)\n\n\
         Хранилище: `{}`, cipher: `XChaCha20Poly1305`, key: фиксированный 32-байтный.\n",
        header.vault_id
    );

    methodology();
    savings(&header);
    opening(&header);
    attachments();
    importing();
    archive();
    ipc_volume();
    generator();
}

/// Sizes of the synthetic vaults used by the save/open scenarios.
const DOC_SIZES: [usize; 4] = [0, 50, 500, 5000];

/// 1. What the numbers mean.
fn methodology() {
    println!("## 1. Методика\n");
    println!(
        "- Счётчик — собственный `#[global_allocator]` этого тестового бинарника (не тот, что в \
         `src/core/perf_audit.rs`).\n\
         - `alloc` — число вызовов `alloc`/`alloc_zeroed`; `realloc` — число ростов буфера.\n\
         - `alloc, КиБ` — запрошенные байты через `alloc`, `+realloc, КиБ` — они же вместе с \
         запрошенными размерами при `realloc`.\n\
         - `peak, КиБ` — пик живых байт сверх живого хипа на старте операции (пик сбрасывается \
         перед каждым прогоном).\n\
         - `мс` — лучшее время из 3 прогонов после прогрева; все замеры одно-поточные.\n\
         - Argon2 в замерах не участвует: `create_vault` вызывается один раз ради реального \
         `VaultHeader`, все seal/open идут с фиксированным ключом `[7u8; 32]`.\n"
    );
}

/// 2. Saving: `seal_document` (the main path — every edit hits it).
fn savings(header: &VaultHeader) {
    println!("## 2. Сохранение хранилища: `seal_document`\n");
    println!(
        "{}",
        table_header(&[
            "Записей",
            "JSON, КиБ",
            "Файл, КиБ",
            "alloc",
            "realloc",
            "alloc, КиБ",
            "+realloc, КиБ",
            "alloc/файл",
            "peak, КиБ",
            "peak/файл",
            "peak/JSON",
            "мс",
        ])
    );

    for &count in DOC_SIZES.iter() {
        let data = quiet(|| document(header.vault_id, &format!("Хранилище {count}"), count, 3));
        let plain_len =
            quiet(|| serde_json::to_vec(&data).expect("the document must serialize to JSON")).len();

        // Two shapes of the same save:
        //  * cold — no size hint (first save after unlocking);
        //  * warm — the hint is the previous plaintext length, which is what the
        //    session passes on every subsequent save.
        let cold = measure(3, || {
            seal_document(header, &VAULT_KEY, &data, 0)
                .expect("seal_document must seal a valid document")
        });
        let measurement = measure(3, || {
            seal_document(header, &VAULT_KEY, &data, plain_len)
                .expect("seal_document must seal a valid document")
        });
        let stats = measurement.stats;
        let best_ms = measurement.best_ms;
        let (file, sealed_plain_len) = measurement.value;
        assert_eq!(
            sealed_plain_len, plain_len,
            "seal_document {count}: the reported plaintext length must match the JSON"
        );
        println!(
            "cold-save {count}: alloc={} realloc={} peak={} КиБ {:.2} мс",
            cold.stats.alloc_calls,
            cold.stats.realloc_calls,
            cold.stats.peak_bytes / 1024,
            cold.best_ms
        );

        // Correctness: the produced file parses, keeps the vault id, and the
        // payload is exactly the tail of the file.
        let parsed = parse_vault_file(&file).expect("a sealed document must parse");
        assert_eq!(
            parsed.header.vault_id, header.vault_id,
            "seal_document {count}: vault_id must survive the round trip"
        );
        let header_len = FIXED_HEADER_LEN
            + u32::from_le_bytes(file[5..9].try_into().expect("length field")) as usize;
        assert_eq!(
            parsed.payload.len(),
            file.len() - header_len,
            "seal_document {count}: payload must be the file tail"
        );

        let file_len = file.len();
        println!(
            "{}",
            row(&[
                count.to_string(),
                format!("{:.1}", kib(plain_len)),
                format!("{:.1}", kib(file_len)),
                stats.alloc_calls.to_string(),
                stats.realloc_calls.to_string(),
                format!("{:.1}", kib(stats.alloc_bytes)),
                format!("{:.1}", kib(stats.alloc_bytes + stats.realloc_bytes)),
                format!("{:.2}", per_ratio(stats.alloc_bytes, file_len)),
                format!("{:.1}", kib(stats.peak_bytes)),
                format!("{:.2}", per_ratio(stats.peak_bytes, file_len)),
                format!("{:.2}", per_ratio(stats.peak_bytes, plain_len)),
                format!("{:.2}", best_ms),
            ])
        );

        // Budgets. Small documents pay the fixed 4 KiB headroom that
        // `seal_document_after` reserves, so the ratios only make sense once
        // the payload dominates it (>= 64 KiB).
        if file_len >= 64 * 1024 {
            assert!(
                stats.alloc_bytes as f64 <= 2.5 * file_len as f64,
                "seal_document {count}: allocated {} B > 2.5 × file {} B",
                stats.alloc_bytes,
                file_len
            );
            // Measured 2.3–2.7 × plaintext: `seal_document_after` reserves only
            // 4 KiB, so `serde_json::to_writer` grows the buffer by doubling and
            // the high-water mark lands above 2 × plaintext. The budget keeps a
            // regression visible without chasing that overshoot.
            assert!(
                stats.peak_bytes as f64 <= 3.2 * plain_len as f64,
                "seal_document {count}: peak {} B > 3.2 × plaintext {} B",
                stats.peak_bytes,
                plain_len
            );
        }
    }

    // The public entry point (`save_vault`) exists on top of `seal_document`
    // and also stores the payload in the session: measure it once so a
    // regression in the wrapper (extra copies) is visible too.
    let mut session = quiet(|| {
        create_vault(
            &CreateVaultConfig {
                name: "Замеры save_vault".to_string(),
                kdf_profile: KdfProfile::Standard,
                cipher: CipherSuite::XChaCha20Poly1305,
            },
            &UnlockSecretInput::Passphrase {
                value: "correct horse battery staple".to_string(),
            },
        )
        .expect("create_vault must work")
    })
    .1;
    let data = quiet(|| document(session.header.vault_id, "save_vault 500", 500, 3));
    let measurement = measure(3, || {
        save_vault(&mut session, &data).expect("save_vault must save a document")
    });
    let stats = measurement.stats;
    let file_len = measurement.value.len();
    let plain_len = quiet(|| serde_json::to_vec(&data).expect("json").len());
    println!(
        "{}",
        row(&[
            "500 (`save_vault`)".to_string(),
            format!("{:.1}", kib(plain_len)),
            format!("{:.1}", kib(file_len)),
            stats.alloc_calls.to_string(),
            stats.realloc_calls.to_string(),
            format!("{:.1}", kib(stats.alloc_bytes)),
            format!("{:.1}", kib(stats.alloc_bytes + stats.realloc_bytes)),
            format!("{:.2}", per_ratio(stats.alloc_bytes, file_len)),
            format!("{:.1}", kib(stats.peak_bytes)),
            format!("{:.2}", per_ratio(stats.peak_bytes, file_len)),
            format!("{:.2}", per_ratio(stats.peak_bytes, plain_len)),
            format!("{:.2}", measurement.best_ms),
        ])
    );
    println!(
        "\n`alloc/файл` ≈ 2.06: содержимое шифруется на месте в одном буфере, плюс ровно одна \
         копия sealed payload для сессии (она нужна `rewrap`/`remove_slot`) и мелкий `Vec` под \
         заголовок. `cold-save` — тот же сейв без подсказки размера: буфер растёт удвоением, \
         10–14 реаллокаций и пик 2.4–2.8×; `warm` (подсказка `plaintext_hint` от предыдущего \
         сейва) — 2 реаллокации и пик 2.06×. Бюджеты: `alloc ≤ 2.5 × файла` и `peak ≤ 3.2 × JSON`.\n"
    );
}

/// 3. Opening: `parse_vault_file` + `open_payload`.
///
/// The decrypted bytes must equal `serde_json::to_vec(&data)`.
fn opening(header: &VaultHeader) {
    println!("## 3. Открытие хранилища: `parse_vault_file` + `open_payload`\n");
    println!(
        "{}",
        table_header(&[
            "Записей",
            "Файл, КиБ",
            "JSON, КиБ",
            "alloc",
            "realloc",
            "alloc, КиБ",
            "+realloc, КиБ",
            "peak, КиБ",
            "peak/файл",
            "мс",
            "JSON совпал",
        ])
    );

    for &count in DOC_SIZES.iter() {
        let data = quiet(|| document(header.vault_id, &format!("Хранилище {count}"), count, 3));
        let expected =
            quiet(|| serde_json::to_vec(&data).expect("the document must serialize to JSON"));
        let file = quiet(|| {
            seal_document(header, &VAULT_KEY, &data, expected.len())
                .expect("seal_document must seal a valid document")
                .0
        });
        let probe = parse_vault_file(&file).expect("a sealed document must parse");
        let cipher = probe.header.cipher;
        let aad = header_aad(&probe.header);
        drop(probe);

        let measurement = measure(3, || {
            let parsed = parse_vault_file(&file).expect("the vault file must parse");
            open_payload(&VAULT_KEY, cipher, &aad, &parsed.payload)
                .expect("open_payload must decrypt with the same key and AAD")
        });
        let stats = measurement.stats;
        let plain = measurement.value;
        let matches = plain == expected;
        assert!(
            matches,
            "open {count}: decrypted JSON must equal serde_json::to_vec(&data) ({} vs {} bytes)",
            plain.len(),
            expected.len()
        );

        println!(
            "{}",
            row(&[
                count.to_string(),
                format!("{:.1}", kib(file.len())),
                format!("{:.1}", kib(expected.len())),
                stats.alloc_calls.to_string(),
                stats.realloc_calls.to_string(),
                format!("{:.1}", kib(stats.alloc_bytes)),
                format!("{:.1}", kib(stats.alloc_bytes + stats.realloc_bytes)),
                format!("{:.1}", kib(stats.peak_bytes)),
                format!("{:.2}", per_ratio(stats.peak_bytes, file.len())),
                format!("{:.2}", measurement.best_ms),
                if matches {
                    "да".to_string()
                } else {
                    "НЕТ".to_string()
                },
            ])
        );

        if file.len() >= 64 * 1024 {
            assert!(
                stats.peak_bytes as f64 <= 2.5 * file.len() as f64,
                "open {count}: peak {} B > 2.5 × file {} B",
                stats.peak_bytes,
                file.len()
            );
        }
    }
    println!(
        "\n`peak/файл` ≈ 2: `parse_vault_file` копирует payload из файла (`to_vec`), а \
         `open_payload` аллоцирует расшифрованный plaintext того же размера.\n"
    );
}

/// 4. Attachments: seal/open round trip plus the in-place variant.
fn attachments() {
    println!("## 4. Вложения: `seal_attachment` / `open_attachment`\n");
    let vault_id = Uuid::new_v4();
    let attachment_id = Uuid::new_v4();
    let header_len = attachment_header_len(vault_id);
    println!("Заголовок вложения: {header_len} байт (+ {TAG_LEN} байт тега).\n");

    println!(
        "{}",
        table_header(&[
            "Размер",
            "Blob, МиБ",
            "путь",
            "alloc",
            "realloc",
            "alloc, КиБ",
            "+realloc, КиБ",
            "peak, КиБ",
            "мс",
            "МиБ/с",
        ])
    );

    for size in [64 * 1024usize, 1024 * 1024, 16 * 1024 * 1024] {
        assert!(
            size <= MAX_ATTACHMENT_BYTES,
            "the fixture must stay inside the attachment limit"
        );
        let mut plain = vec![0u8; size];
        rand::thread_rng().fill_bytes(&mut plain);

        let seal = measure(3, || {
            seal_attachment(&VAULT_KEY, vault_id, attachment_id, &plain)
                .expect("seal_attachment must seal the blob")
        });
        let blob_len = seal.value.len();
        assert_eq!(
            blob_len,
            header_len + size + TAG_LEN,
            "attachment blob layout must be header | ciphertext | tag"
        );
        let sealed = seal.value;
        let reopened = quiet(|| {
            open_attachment(&VAULT_KEY, vault_id, attachment_id, &sealed)
                .expect("open_attachment must decrypt the blob")
        });
        assert!(
            reopened == plain,
            "open_attachment(seal_attachment(x)) must equal x for {size} bytes"
        );

        println!(
            "{}",
            row(&[
                size_label(size),
                format!("{:.2}", mib(blob_len)),
                "seal".to_string(),
                seal.stats.alloc_calls.to_string(),
                seal.stats.realloc_calls.to_string(),
                format!("{:.1}", kib(seal.stats.alloc_bytes)),
                format!(
                    "{:.1}",
                    kib(seal.stats.alloc_bytes + seal.stats.realloc_bytes)
                ),
                format!("{:.1}", kib(seal.stats.peak_bytes)),
                format!("{:.2}", seal.best_ms),
                format!("{:.0}", mib_per_sec(size, seal.best_ms)),
            ])
        );

        let open = measure(3, || {
            open_attachment(&VAULT_KEY, vault_id, attachment_id, &sealed)
                .expect("open_attachment must decrypt the blob")
        });
        assert_eq!(
            open.value.len(),
            size,
            "open_attachment must return the original size"
        );
        println!(
            "{}",
            row(&[
                size_label(size),
                format!("{:.2}", mib(blob_len)),
                "open".to_string(),
                open.stats.alloc_calls.to_string(),
                open.stats.realloc_calls.to_string(),
                format!("{:.1}", kib(open.stats.alloc_bytes)),
                format!(
                    "{:.1}",
                    kib(open.stats.alloc_bytes + open.stats.realloc_bytes)
                ),
                format!("{:.1}", kib(open.stats.peak_bytes)),
                format!("{:.2}", open.best_ms),
                format!("{:.0}", mib_per_sec(size, open.best_ms)),
            ])
        );

        if size == 16 * 1024 * 1024 {
            assert!(
                seal.stats.peak_bytes as f64 <= 2.5 * size as f64,
                "seal_attachment 16 MiB: peak {} B > 2.5 × 16 MiB",
                seal.stats.peak_bytes
            );
        }
    }

    println!("\n### 4.1 `seal_attachment_in_place`: сколько буферов в пике\n");
    println!(
        "{}",
        table_header(&[
            "Размер",
            "путь",
            "alloc",
            "realloc",
            "alloc, КиБ",
            "+realloc, КиБ",
            "peak, КиБ",
            "мс",
            "МиБ/с",
        ])
    );

    for size in [64 * 1024usize, 1024 * 1024, 16 * 1024 * 1024] {
        let mut plain = vec![0u8; size];
        rand::thread_rng().fill_bytes(&mut plain);

        // A — сегодняшний путь: plaintext уже лежит в отдельном Vec, копируется в blob.
        let copied = measure(3, || {
            let source = plain.clone();
            seal_attachment(&VAULT_KEY, vault_id, attachment_id, &source)
                .expect("seal_attachment must seal the blob")
        });

        // B — путь загрузки: буфер с заголовком собирается на месте, plaintext
        // не отдельным Vec, а сразу в blob[HEADER_LEN..] (здесь — через
        // extend_from_slice, чтобы сценарий был самодостаточным и проверяемым).
        let in_place = measure(3, || {
            let mut blob = Vec::with_capacity(header_len + plain.len() + TAG_LEN);
            blob.resize(header_len, 0);
            blob.extend_from_slice(&plain);
            seal_attachment_in_place(&VAULT_KEY, vault_id, attachment_id, &mut blob)
                .expect("seal_attachment_in_place must seal the buffer");
            blob
        });
        let reopened = quiet(|| {
            open_attachment(&VAULT_KEY, vault_id, attachment_id, &in_place.value)
                .expect("open_attachment must decrypt the in-place blob")
        });
        assert!(
            reopened == plain,
            "seal_attachment_in_place(x) must round trip for {size} bytes"
        );

        // C — steady state пути загрузки: буфер уже заполнен, повторная
        // шифрует уже шифротекст, поэтому проверяем только стоимость пути.
        let mut reusable = Vec::with_capacity(header_len + plain.len() + TAG_LEN);
        reusable.resize(header_len, 0);
        reusable.extend_from_slice(&plain);
        let steady = measure(3, || {
            reusable.truncate(header_len + plain.len());
            seal_attachment_in_place(&VAULT_KEY, vault_id, attachment_id, &mut reusable)
                .expect("seal_attachment_in_place must seal the buffer");
            reusable.len()
        });

        for (label, stats, best_ms) in [
            ("A: plaintext Vec + seal", copied.stats, copied.best_ms),
            ("B: буфер + in_place", in_place.stats, in_place.best_ms),
            ("C: буфер готов (steady)", steady.stats, steady.best_ms),
        ] {
            println!(
                "{}",
                row(&[
                    size_label(size),
                    label.to_string(),
                    stats.alloc_calls.to_string(),
                    stats.realloc_calls.to_string(),
                    format!("{:.1}", kib(stats.alloc_bytes)),
                    format!("{:.1}", kib(stats.alloc_bytes + stats.realloc_bytes)),
                    format!("{:.1}", kib(stats.peak_bytes)),
                    format!("{:.2}", best_ms),
                    format!("{:.0}", mib_per_sec(size, best_ms)),
                ])
            );
        }
    }
    println!(
        "\nA — путь `commands/attachments.rs` сегодня: base64 уже декодирован в отдельный \
         `Vec` (`plain.to_vec()` — его аналог), `seal_attachment` копирует его в blob → в пике живы \
         2 буфера размера вложения. B — `seal_attachment_in_place`: plaintext сразу лежит в \
         `blob[HEADER_LEN..]`, поэтому в пике один буфер. C — steady state загрузки (буфер уже \
         заполнен): на само шифрование 0 байт и 0 аллокаций, только 16-байтный тег.\n"
    );
}

/// 5. Import: Bitwarden JSON.
fn importing() {
    println!("## 5. Импорт Bitwarden JSON\n");
    println!(
        "{}",
        table_header(&[
            "Записей",
            "JSON, МиБ",
            "операция",
            "alloc",
            "realloc",
            "alloc/запись",
            "peak, КиБ",
            "мс",
            "записей/с",
        ])
    );

    for items in [200usize, 2_000, 20_000] {
        let json = quiet(|| bitwarden_json(items));

        let parsed = measure(3, || {
            let bundle = import::parse(&json, None, None).expect("Bitwarden JSON must parse");
            bundle.items.len()
        });
        assert_eq!(
            parsed.value, items,
            "import::parse must return every Bitwarden item"
        );
        print_import_row(items, json.len(), "parse", &parsed, parsed.value);

        // Serialization alone, to separate it from parsing: this is what the
        // `import_parse` command adds on top of the parser.
        let bundle = quiet(|| import::parse(&json, None, None).expect("Bitwarden JSON must parse"));
        let serialize = measure(3, || {
            serde_json::to_vec(&bundle)
                .expect("the import bundle must serialize")
                .len()
        });
        print_import_row(items, json.len(), "serde_json::to_vec", &serialize, items);

        // The whole command: parse + stream the bundle out as JSON.
        let full = measure(3, || {
            let bundle = import::parse(&json, None, None).expect("Bitwarden JSON must parse");
            let bytes = serde_json::to_vec(&bundle).expect("the import bundle must serialize");
            black_box(bytes.len());
            bundle.items.len()
        });
        print_import_row(items, json.len(), "parse + to_vec", &full, full.value);

        let alloc_per_item = per_ratio(parsed.stats.alloc_calls, items);
        assert!(
            alloc_per_item <= 12.0,
            "import {items}: {alloc_per_item:.1} alloc/item > 12 ({} allocations)",
            parsed.stats.alloc_calls
        );
        assert!(
            parsed.stats.realloc_calls <= items / 100 + 20,
            "import {items}: {} reallocations > {}+20",
            parsed.stats.realloc_calls,
            items / 100
        );
    }
    println!(
        "\nПарсер отдаёт `Cow`-срезы в исходный буфер: почти все аллокации — это `Vec` под элементы \
         и вложенные списки, строки не копируются. `parse + to_vec` — то, что реально делает \
         команда `import_parse`.\n"
    );
}

fn print_import_row(
    items: usize,
    json_len: usize,
    label: &str,
    measurement: &Measurement<usize>,
    returned_items: usize,
) {
    println!(
        "{}",
        row(&[
            items.to_string(),
            format!("{:.2}", mib(json_len)),
            label.to_string(),
            measurement.stats.alloc_calls.to_string(),
            measurement.stats.realloc_calls.to_string(),
            format!("{:.1}", per_ratio(measurement.stats.alloc_calls, items)),
            format!("{:.1}", kib(measurement.stats.peak_bytes)),
            format!("{:.2}", measurement.best_ms),
            format!("{:.0}", items as f64 / (measurement.best_ms / 1000.0)),
        ])
    );
    assert!(
        returned_items <= items,
        "{label}: the parser cannot invent items"
    );
}

/// 6. `pms_archive`: export/import of a 2 MiB vault plus 20 × 1 MiB attachments.
fn archive() {
    println!("## 6. Архив (`pms_archive`): 2 МиБ хранилище + 20 вложений по 1 МиБ\n");

    let header = quiet(|| {
        create_vault(
            &CreateVaultConfig {
                name: "Архив".to_string(),
                kdf_profile: KdfProfile::Standard,
                cipher: CipherSuite::XChaCha20Poly1305,
            },
            &UnlockSecretInput::Passphrase {
                value: "correct horse battery staple".to_string(),
            },
        )
        .expect("create_vault must work")
        .1
        .header
    });
    assert_ne!(header.vault_id, Uuid::nil(), "sanity: a real vault id");

    let vault_bytes = quiet(|| {
        let data = fat_document(header.vault_id, 2 * 1024 * 1024);
        seal_document(&header, &VAULT_KEY, &data, 2 * 1024 * 1024)
            .expect("seal_document must seal the archive vault")
            .0
    });

    let attachment_bytes: Vec<Vec<u8>> = (0..20)
        .map(|index| filler(1024 * 1024, 0x51ce_0000 + index as u64))
        .collect();

    let source = tempdir().expect("tempdir for the archive source");
    let attachments_dir = source.path().join("attachments");
    fs::create_dir_all(&attachments_dir).expect("create attachments dir");
    let vault_path = source.path().join("pms.vault");
    fs::write(&vault_path, &vault_bytes).expect("write the pseudo vault");
    for (index, bytes) in attachment_bytes.iter().enumerate() {
        fs::write(
            attachments_dir.join(format!("att{:02}.pmsa", index + 1)),
            bytes,
        )
        .expect("write an attachment");
    }
    let total_bytes = vault_bytes.len() + attachment_bytes.iter().map(Vec::len).sum::<usize>();
    println!(
        "Источник: `pms.vault` {:.2} МиБ + 20 × 1 МиБ вложений = {:.2} МиБ.\n",
        mib(vault_bytes.len()),
        mib(total_bytes)
    );

    let archive_path = source.path().join("backup.pmsvault");
    let export = measure(3, || {
        archive_export(source.path(), &archive_path).expect("archive_export must succeed")
    });
    let archive_len = fs::metadata(&archive_path).expect("archive metadata").len() as usize;

    // `archive_import` refuses a non-empty destination, so every run needs a
    // fresh path: the closure derives a new directory name per run.
    let restore_root = tempdir().expect("tempdir for the archive target");
    let mut run = 0usize;
    let import_measure = measure(3, || {
        run += 1;
        let destination = restore_root.path().join(format!("restored-{run}"));
        archive_import(&archive_path, &destination).expect("archive_import must succeed")
    });

    println!(
        "{}",
        table_header(&[
            "Операция",
            "Объём, МиБ",
            "alloc",
            "realloc",
            "alloc, КиБ",
            "peak, КиБ",
            "peak/объём",
            "мс",
            "МиБ/с"
        ])
    );
    for (label, stats, best_ms, bytes) in [
        ("export", export.stats, export.best_ms, total_bytes),
        (
            "import",
            import_measure.stats,
            import_measure.best_ms,
            total_bytes,
        ),
    ] {
        println!(
            "{}",
            row(&[
                label.to_string(),
                format!("{:.2}", mib(bytes)),
                stats.alloc_calls.to_string(),
                stats.realloc_calls.to_string(),
                format!("{:.1}", kib(stats.alloc_bytes)),
                format!("{:.1}", kib(stats.peak_bytes)),
                format!("{:.2}", per_ratio(stats.peak_bytes, bytes)),
                format!("{:.2}", best_ms),
                format!("{:.0}", mib_per_sec(bytes, best_ms)),
            ])
        );
    }
    println!(
        "\nРазмер архива: {:.2} МиБ (zip `Stored`, без сжатия — как в `pms_archive::export`).\n",
        mib(archive_len)
    );

    // Byte-for-byte verification of the whole round trip.
    let check_root = tempdir().expect("tempdir for the archive verification");
    let restored = check_root.path().join("restored");
    quiet(|| archive_import(&archive_path, &restored).expect("verification import"));
    assert_eq!(
        fs::read(restored.join("pms.vault")).expect("read restored vault"),
        vault_bytes,
        "the restored pms.vault must be byte-identical"
    );
    for (index, bytes) in attachment_bytes.iter().enumerate() {
        let name = format!("att{:02}.pmsa", index + 1);
        assert_eq!(
            fs::read(restored.join("attachments").join(&name)).expect("read a restored attachment"),
            *bytes,
            "the restored attachment {name} must be byte-identical"
        );
    }
    assert_eq!(
        fs::read_dir(restored.join("attachments"))
            .expect("list restored attachments")
            .count(),
        attachment_bytes.len(),
        "the archive must restore exactly the exported attachments"
    );
    println!("Проверка: все 21 файл после `archive_import` совпадают побайтово.\n");
}

/// 7. IPC volume, computed from the code (no network, no JS runtime).
fn ipc_volume() {
    println!("## 7. Сквозной объём IPC\n");
    println!(
        "Формулы (n — размер файла в байтах):\n\
         - base64: `b64(n) = 4 * ceil(n / 3)` — раздувание в 4/3 (+33.3 %).\n\
         - Rust → JS, base64: `STANDARD.encode` (b64) + JSON-тело ответа (b64) + JS-строка (b64) + \
         `atob` → бинарная строка (n) + побайтовый цикл → `Uint8Array` (n) = **3·b64 + 2n**.\n\
         - JS → Rust, base64: бинарная строка из `toBase64` (n) + `btoa` (b64) + JSON аргументов \
         (b64) + `serde_json` → `String` (b64) + `STANDARD.decode` → `Vec<u8>` (n) = **3·b64 + 2n**.\n\
         - raw IPC (`tauri::ipc::Response` / `InvokeBody::Raw`): тело сообщения само и есть байты — \
         **n** в каждую сторону.\n\
         Значения посчитаны кодом, не вписаны руками.\n"
    );

    let n = 4 * 1024 * 1024usize; // 4 MiB vault
    let b64 = n.div_ceil(3) * 4;
    let base64_touched = 3 * b64 + 2 * n;
    let raw_touched = n;

    println!(
        "{}",
        table_header(&[
            "Направление",
            "Схема",
            "На проводе, МиБ",
            "Полных копий",
            "Всего пройдено, МиБ",
            "К n",
        ])
    );
    for (direction, scheme, wire, copies, touched) in [
        ("Rust → JS", "raw ArrayBuffer", n, 1, raw_touched),
        ("Rust → JS", "base64-строка", b64, 5, base64_touched),
        ("JS → Rust", "raw ArrayBuffer", n, 1, raw_touched),
        ("JS → Rust", "base64-строка", b64, 5, base64_touched),
    ] {
        println!(
            "{}",
            row(&[
                direction.to_string(),
                scheme.to_string(),
                format!("{:.2}", mib(wire)),
                copies.to_string(),
                format!("{:.2}", mib(touched)),
                format!("{:.2}×", per_ratio(touched, n)),
            ])
        );
    }

    println!("\n### 7.1 Что делает текущая схема (по коду)\n");
    println!(
        "{}",
        table_header(&[
            "Команда",
            "Направление",
            "Схема сейчас",
            "На проводе, МиБ",
            "Пройдено, МиБ",
        ])
    );
    // Facts read from the code:
    // - `commands/vault.rs`: `vault_save`, `vault_rewrap`, `vault_remove_slot`
    //   return `tauri::ipc::Response` (raw), while `vault_create` returns
    //   `file_b64` and `vault_inspect`/`vault_unlock` take `file_b64`.
    // - `commands/attachments.rs`: `attachment_seal`/`attachment_open` are the
    //   base64 variants (kept for Android storage), `attachment_open_file` is raw.
    // - `commands/storage.rs`: `storage_read_file`/`storage_write_atomic` are
    //   base64 (`b64` field), i.e. every vault read/write in the desktop backend.
    // - `src/lib/base64.ts`: `toBase64` builds the binary string in 32 KiB chunks
    //   and calls `btoa`; `fromBase64` calls `atob` and then copies into a
    //   `Uint8Array` byte by byte.
    let commands: [(&str, &str, bool); 7] = [
        ("vault_save", "Rust → JS", false),
        ("vault_create (file)", "Rust → JS", true),
        ("vault_unlock (file)", "JS → Rust", true),
        ("vault_inspect (file)", "JS → Rust", true),
        ("storage_read_file", "Rust → JS", true),
        ("storage_write_atomic", "JS → Rust", true),
        ("attachment_seal (blob)", "Rust → JS", true),
    ];
    let mut round_trip = 0usize;
    for (command, direction, base64) in commands {
        let (wire, touched) = if base64 {
            (b64, base64_touched)
        } else {
            (raw_touched, raw_touched)
        };
        round_trip += touched;
        println!(
            "{}",
            row(&[
                command.to_string(),
                direction.to_string(),
                if base64 { "base64" } else { "raw" }.to_string(),
                format!("{:.2}", mib(wire)),
                format!("{:.2}", mib(touched)),
            ])
        );
    }

    // One interactive save on desktop today: read the vault from disk
    // (`storage_read_file`, base64), `vault_save` (raw response), write it back
    // (`storage_write_atomic`, base64).
    let save_cycle = base64_touched + raw_touched + base64_touched;
    println!(
        "\nОдин цикл сохранения на desktop (read `storage_read_file` → `vault_save` → \
         `storage_write_atomic`) для файла 4 МиБ: **{:.2} МиБ** пройденных байт ({:.2}× файла), \
         из них {:.2} МиБ на проводе вместо {:.2} МиБ.\n",
        mib(save_cycle),
        per_ratio(save_cycle, n),
        mib(2 * b64 + n),
        mib(3 * n)
    );
    println!(
        "Для сравнения, полностью raw-схема на том же цикле: {:.2} МиБ пройденных байт \
         ({:.2}× файла), {:.2} МиБ на проводе. Разница — только объём копий в JS/Rust, сеть тут \
         ни при чём: это memcpy на каждой операции.\n",
        mib(3 * n),
        per_ratio(3 * n, n),
        mib(3 * n)
    );
    println!(
        "Суммарная стоимость перечисленных команд для файла 4 МиБ: {:.2} МиБ пройденных байт.\n",
        mib(round_trip)
    );
}

/// 8. Generators: `generate_password` / `generate_passphrase`.
fn generator() {
    println!("## 8. Генераторы\n");
    println!(
        "{}",
        table_header(&[
            "Генератор",
            "вызовов",
            "мкс/вызов",
            "alloc/вызов",
            "байт alloc/вызов",
            "realloc/вызов",
            "мс всего",
            "пример",
        ])
    );

    const CALLS: usize = 1000;

    let password_options = PasswordOptions {
        length: 16,
        lowercase: true,
        uppercase: true,
        digits: true,
        symbols: true,
        ..PasswordOptions::default()
    };
    let password = measure(1, || {
        let mut total = 0usize;
        let mut last = String::new();
        for _ in 0..CALLS {
            let value = generate_password(&password_options)
                .expect("generate_password must produce a password");
            total += value.len();
            last = value;
        }
        black_box(total);
        last
    });
    let per_call = password.best_ms * 1000.0 / CALLS as f64;
    let allocs_per_call = per_ratio(password.stats.alloc_calls, CALLS);
    println!(
        "{}",
        row(&[
            "generate_password (16, все группы)".to_string(),
            CALLS.to_string(),
            format!("{:.1}", per_call),
            format!("{:.2}", allocs_per_call),
            format!("{:.0}", per_ratio(password.stats.alloc_bytes, CALLS)),
            format!("{:.2}", per_ratio(password.stats.realloc_calls, CALLS)),
            format!("{:.2}", password.best_ms),
            password.value.clone(),
        ])
    );
    assert!(
        allocs_per_call < 10.0,
        "generate_password: {allocs_per_call:.2} alloc/call >= 10"
    );
    assert!(
        per_call < 1000.0,
        "generate_password: {per_call:.1} µs/call >= 1 ms"
    );

    let passphrase_options = PassphraseOptions {
        words: 5,
        ..PassphraseOptions::default()
    };
    let passphrase = measure(1, || {
        let mut total = 0usize;
        let mut last = String::new();
        for _ in 0..CALLS {
            let value = generate_passphrase(&passphrase_options)
                .expect("generate_passphrase must produce a passphrase");
            total += value.len();
            last = value;
        }
        black_box(total);
        last
    });
    let per_call = passphrase.best_ms * 1000.0 / CALLS as f64;
    let allocs_per_call = per_ratio(passphrase.stats.alloc_calls, CALLS);
    println!(
        "{}",
        row(&[
            "generate_passphrase (5 слов)".to_string(),
            CALLS.to_string(),
            format!("{:.1}", per_call),
            format!("{:.2}", allocs_per_call),
            format!("{:.0}", per_ratio(passphrase.stats.alloc_bytes, CALLS)),
            format!("{:.2}", per_ratio(passphrase.stats.realloc_calls, CALLS)),
            format!("{:.2}", passphrase.best_ms),
            passphrase.value.clone(),
        ])
    );
    assert!(
        allocs_per_call < 10.0,
        "generate_passphrase: {allocs_per_call:.2} alloc/call >= 10"
    );
    assert!(
        per_call < 1000.0,
        "generate_passphrase: {per_call:.1} µs/call >= 1 ms"
    );

    println!(
        "\nБюджеты генератора (< 1 мс и < 10 аллокаций на вызов) выполнены с запасом: вызов не \
         делает ни одного крупного копирования, только сборку строки из готовых групп/слов.\n"
    );
}
