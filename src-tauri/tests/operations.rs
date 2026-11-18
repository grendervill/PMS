//! End-to-end operation timings: what a user actually waits for.
//!
//! The allocation suite (`tests/perf_suite.rs`) answers "how many copies does a
//! save make". This one answers "how long does unlocking take", "how long does a
//! save with fsync take", "how much memory does exporting a 200 MiB vault need".
//! It deliberately runs the *real* key derivation (Argon2id) and the real file
//! system, so the numbers are what the app shows.
//!
//! ```sh
//! cargo test --release --test operations -- --nocapture --test-threads=1
//! ```
//!
//! Release mode is required: debug Argon2 is 30-50x slower and the io numbers are
//! meaningless.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use pms_lib::testing::{
    CreateVaultConfig, CipherSuite, KdfProfile, PasswordOptions, UnlockSecretInput, VaultData,
    archive_export, archive_import, create_vault, generate_password, generate_passphrase,
    import, save_vault, unlock_vault,
};
use pms_lib::testing::{PassphraseOptions, VaultSession};
use tempfile::tempdir;

const PASSPHRASE: &str = "correct horse battery staple";

fn passphrase() -> UnlockSecretInput {
    UnlockSecretInput::Passphrase {
        value: PASSPHRASE.to_string(),
    }
}

fn profile(name: &str) -> KdfProfile {
    match name {
        "standard" => KdfProfile::Standard,
        "strong" => KdfProfile::Strong,
        _ => KdfProfile::Paranoid,
    }
}

fn config(name: &str) -> CreateVaultConfig {
    CreateVaultConfig {
        name: name.to_string(),
        kdf_profile: KdfProfile::Standard,
        cipher: CipherSuite::XChaCha20Poly1305,
    }
}

/// Current RSS of this process in MiB.
fn rss_mib() -> f64 {
    let pid = std::process::id().to_string();
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .expect("ps must be available");
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse::<f64>()
        .map(|kib| kib / 1024.0)
        .unwrap_or(0.0)
}

fn best_of(runs: usize, mut f: impl FnMut()) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..runs {
        let start = Instant::now();
        f();
        best = best.min(start.elapsed().as_secs_f64() * 1000.0);
    }
    best
}

fn row(cells: &[String]) -> String {
    let mut line = String::from("|");
    for cell in cells {
        line.push(' ');
        line.push_str(cell);
        line.push_str(" |");
    }
    line
}

fn header(cells: &[&str]) -> String {
    let mut out = row(&cells.iter().map(|cell| cell.to_string()).collect::<Vec<_>>());
    out.push('\n');
    out.push('|');
    for _ in cells {
        out.push_str("---|");
    }
    out
}

fn filler(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.extend_from_slice(&state.to_le_bytes());
    }
    out.truncate(len);
    out
}

/// A realistic vault document: entries with links, history and a TOTP secret.
fn document(vault_id: uuid::Uuid, entries: usize) -> VaultData {
    let created = "2026-01-02T03:04:05.678Z".to_string();
    let mut data = VaultData {
        schema_version: 1,
        id: vault_id,
        name: "Замеры".to_string(),
        created_at: created.clone(),
        updated_at: created.clone(),
        settings: Default::default(),
        entries: Vec::new(),
        ssh_keys: Vec::new(),
        bank_cards: Vec::new(),
        documents: Vec::new(),
        attachments: Vec::new(),
        drafts: Vec::new(),
        favorites: Vec::new(),
    };
    for index in 0..entries {
        let entry: pms_lib::testing::Entry = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(),
            "name": format!("Сервис номер {index}"),
            "login": format!("alex{index}@example.com"),
            "password": format!("s3cret-{index}-password-with-some-length"),
            "url": format!("https://example{index}.com"),
            "urls": [format!("https://example{index}.com"), format!("https://login.example{index}.com")],
            "comment": format!("Заметка к записи {index}: адрес, договор, прочие детали средней длины."),
            "totp": {
                "secret": "JBSWY3DPEHPK3PXP",
                "algorithm": "SHA1",
                "digits": 6,
                "period": 30,
                "issuer": "Example",
                "account": format!("alex{index}")
            },
            "passwordHistory": [
                { "value": format!("old-{index}-a"), "changedAt": created },
                { "value": format!("old-{index}-b"), "changedAt": created }
            ],
            "createdAt": created,
            "updatedAt": created
        }))
        .expect("the entry fixture must deserialize");
        data.entries.push(entry);
    }
    data
}

fn bitwarden_json(items: usize) -> Vec<u8> {
    let mut out = String::with_capacity(items * 900 + 128);
    out.push_str(r#"{"encrypted":false,"folders":[{"id":"f1","name":"Social"}],"items":["#);
    for index in 0..items {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            r#"{{"id":"i{index}","type":1,"name":"Сервис {index}","folderId":"f1","notes":"Заметка к записи номер {index} средней длины для реалистичности.","fields":[{{"name":"PIN","value":"12{index}"}}],"login":{{"username":"alex{index}@example.com","password":"s3cret-{index}","totp":"JBSWY3DPEHPK3PXP","uris":[{{"uri":"https://example{index}.com"}}]}}}}"#
        ));
    }
    out.push_str("]}");
    out.into_bytes()
}

fn write_file(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).expect("write must succeed");
}

#[test]
fn operations() {
    println!("\n# Сквозные операции PMS (release, один поток)");
    println!(
        "\narch: {} | threads: {} | rss на старте: {:.1} МиБ",
        std::env::consts::ARCH,
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1),
        rss_mib()
    );

    // ---------------------------------------------------------------- unlock
    println!("\n## 1. Создание и разблокировка (настоящий Argon2id)");
    println!("{}", header(&["профиль", "создание, мс", "разблокировка, мс", "всего, мс"]));

    let mut standard_unlock = 0.0;
    for name in ["standard", "strong", "paranoid"] {
        let cfg = CreateVaultConfig {
            name: "Замеры".to_string(),
            kdf_profile: profile(name),
            cipher: CipherSuite::XChaCha20Poly1305,
        };
        let start = Instant::now();
        let (file, _session, data) = create_vault(&cfg, &passphrase()).expect("create must work");
        let create_ms = start.elapsed().as_secs_f64() * 1000.0;
        let unlock_ms = best_of(2, || {
            unlock_vault(&file, &passphrase()).expect("unlock must work with the right secret");
        });
        if name == "standard" {
            standard_unlock = unlock_ms;
        }
        println!(
            "{}",
            row(&[
                name.to_string(),
                format!("{create_ms:.0}"),
                format!("{unlock_ms:.0}"),
                format!("{:.0}", create_ms + unlock_ms),
            ])
        );
        assert_eq!(data.entries.len(), 0);
    }
    println!(
        "\nЗамечание: `standard` — профиль по умолчанию (65 536 КиБ, t=3), `paranoid` — 524 288 КиБ, t=6.\n\
         Разблокировка целиком уходит в `spawn_blocking` (commands/vault.rs), поэтому интерфейс всё это время жив."
    );
    // Loose on purpose: this is a regression guard, not a benchmark of the
    // machine (a loaded CI runner can be an order of magnitude slower).
    assert!(
        standard_unlock < 15_000.0,
        "unlocking with the default profile must stay under 15 s (was {standard_unlock:.0} ms)"
    );

    // ------------------------------------------------------------- save + fsync
    println!("\n## 2. Сохранение хранилища: сериализация + AEAD + запись на диск с fsync");
    println!("{}", header(&["записей", "файл, КиБ", "seal, мс", "seal+запись, мс", "файл открывается"]));

    let (_, mut session, _) = create_vault(&config("Замеры"), &passphrase()).expect("create");
    for entries in [50usize, 500, 5000] {
        let data = document(session.header.vault_id, entries);
        let file = save_vault(&mut session, &data).expect("save must work");
        let sealed_only = best_of(3, || {
            let _ = save_vault(&mut session, &data).expect("save must work");
        });
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("pms.vault");
        let with_io = best_of(3, || {
            let bytes = save_vault(&mut session, &data).expect("save must work");
            let mut handle = fs::File::create(&path).expect("create");
            std::io::Write::write_all(&mut handle, &bytes).expect("write");
            handle.sync_all().expect("fsync");
        });
        let reopened = pms_lib::testing::parse_vault_file(&file).is_ok();
        println!(
            "{}",
            row(&[
                entries.to_string(),
                format!("{:.1}", file.len() as f64 / 1024.0),
                format!("{sealed_only:.2}"),
                format!("{with_io:.2}"),
                if reopened { "да" } else { "НЕТ" }.to_string(),
            ])
        );
        assert!(reopened, "a saved file must parse again");
        assert!(
            with_io < 1_000.0,
            "saving {entries} entries with fsync must stay under 1 s (was {with_io:.1} ms)"
        );
    }

    // ------------------------------------------------------------- attachments
    println!("\n## 3. Вложение 16 МиБ: путь загрузки (base64 → decode → AEAD → файл)");
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("attachments")).expect("mkdir");
    let raw = filler(16 * 1024 * 1024, 0x9e37_79b9);
    let encoded = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(&raw)
    };

    let (_, session, _) = create_vault(&config("Вложения"), &passphrase()).expect("create");
    let attachment_id = uuid::Uuid::new_v4();
    let baseline = rss_mib();
    let seal_ms = best_of(2, || {
        let mut blob = vec![0u8; pms_lib::testing::ATTACHMENT_HEADER_LEN + raw.len()];
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode_slice(encoded.as_bytes(), &mut blob[pms_lib::testing::ATTACHMENT_HEADER_LEN..])
            .expect("decode");
        let mut blob = blob;
        blob.truncate(pms_lib::testing::ATTACHMENT_HEADER_LEN + raw.len());
        pms_lib::testing::seal_attachment_in_place(
            session.vault_key_for_tests(),
            session.header.vault_id,
            attachment_id,
            &mut blob,
        )
        .expect("seal");
    });
    let peak = rss_mib();
    println!(
        "{}",
        row(&[
            "16 МиБ".to_string(),
            format!("{:.1}", encoded.len() as f64 / 1024.0 / 1024.0),
            format!("{seal_ms:.1}"),
            format!("{:.0}", 16.0 / (seal_ms / 1000.0)),
            format!("{:+.1}", peak - baseline),
        ])
    );
    println!("\nСтолбцы: размер | base64 на входе, МиБ | seal, мс | МиБ/с | прирост RSS, МиБ");
    assert!(
        peak - baseline < 160.0,
        "sealing a 16 MiB attachment must not add more than ~160 MiB RSS (was {:.1})",
        peak - baseline
    );

    // ------------------------------------------------------------------ import
    println!("\n## 4. Импорт");
    println!("{}", header(&["источник", "записей", "вход, МиБ", "мс", "записей/с"]));
    for items in [200usize, 2_000, 20_000] {
        let json = bitwarden_json(items);
        let ms = best_of(2, || {
            let bundle = import::parse(&json, None, None).expect("import must parse");
            assert_eq!(bundle.items.len(), items);
        });
        println!(
            "{}",
            row(&[
                "Bitwarden JSON".to_string(),
                items.to_string(),
                format!("{:.1}", json.len() as f64 / 1024.0 / 1024.0),
                format!("{ms:.1}"),
                format!("{:.0}", items as f64 / (ms / 1000.0)),
            ])
        );
    }
    let kdbx = fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/keepass_argon2id.kdbx"))
        .expect("fixture");
    let kdbx_ms = best_of(2, || {
        let bundle = import::parse(&kdbx, Some("demopass"), None).expect("kdbx must parse");
        assert_eq!(bundle.items.len(), 2);
    });
    println!(
        "{}",
        row(&[
            "KeePass KDBX (Argon2id)".to_string(),
            "2".to_string(),
            format!("{:.3}", kdbx.len() as f64 / 1024.0 / 1024.0),
            format!("{kdbx_ms:.0}"),
            "—".to_string(),
        ])
    );

    // --------------------------------------------------------------- archive
    println!("\n## 5. Архив: хранилище ~63 МиБ + 16 вложений по 8 МиБ (~191 МиБ)");
    let source = tempdir().expect("tempdir");
    let attachments = source.path().join("attachments");
    fs::create_dir_all(&attachments).expect("mkdir");
    let fat = document(uuid::Uuid::new_v4(), 500);
    let fat_file = {
        let cfg = config("Архив");
        let (_file, session, _) = create_vault(&cfg, &passphrase()).expect("create");
        let mut session: VaultSession = session;
        let mut data = fat.clone();
        data.id = session.header.vault_id;
        // Pad the vault to ~64 MiB with a draft, so the archive is realistic.
        data.drafts.push(pms_lib::testing::Draft {
            id: "padding".to_string(),
            kind: pms_lib::testing::DraftKind::Entry,
            payload: serde_json::json!({ "padding": "A".repeat(63 * 1024 * 1024) }),
            updated_at: "2026-01-02T03:04:05.678Z".to_string(),
        });
        save_vault(&mut session, &data).expect("save")
    };
    write_file(&source.path().join("pms.vault"), &fat_file);
    for index in 0..16 {
        write_file(
            &attachments.join(format!("{}.pmsa", uuid::Uuid::from_u128(index as u128 + 1))),
            &filler(8 * 1024 * 1024, 0x1234_5678 + index as u64),
        );
    }
    let total_mib = 63.0 + 16.0 * 8.0;

    let archive = tempdir().expect("tempdir");
    let archive_path = archive.path().join("backup.pmsvault");
    let baseline = rss_mib();
    let start = Instant::now();
    archive_export(source.path(), &archive_path).expect("export must work");
    let export_ms = start.elapsed().as_secs_f64() * 1000.0;
    let export_peak = rss_mib();

    let restore = tempdir().expect("tempdir");
    let restore_path = restore.path().join("restored");
    let baseline_import = rss_mib();
    let start = Instant::now();
    archive_import(&archive_path, &restore_path).expect("import must work");
    let import_ms = start.elapsed().as_secs_f64() * 1000.0;
    let import_peak = rss_mib();

    println!(
        "{}",
        header(&["операция", "объём, МиБ", "время, мс", "МиБ/с", "прирост RSS, МиБ", "peak/объём"])
    );
    for (label, ms, peak_delta) in [
        ("export", export_ms, export_peak - baseline),
        ("import", import_ms, import_peak - baseline_import),
    ] {
        println!(
            "{}",
            row(&[
                label.to_string(),
                format!("{total_mib:.0}"),
                format!("{ms:.0}"),
                format!("{:.0}", total_mib / (ms / 1000.0)),
                format!("{peak_delta:+.1}"),
                format!("{:.2}", peak_delta / total_mib),
            ])
        );
    }
    // Everything must survive the round trip byte for byte.
    let restored_vault = fs::read(restore_path.join("pms.vault")).expect("restored vault");
    assert_eq!(restored_vault, fat_file, "the vault must round trip");
    let mut restored = 0;
    for entry in fs::read_dir(restore_path.join("attachments")).expect("ls") {
        let entry = entry.expect("entry");
        let name = entry.file_name();
        let original = attachments.join(&name);
        assert_eq!(
            fs::read(entry.path()).expect("read"),
            fs::read(&original).expect("read"),
            "attachment {name:?} must round trip"
        );
        restored += 1;
    }
    assert_eq!(restored, 16);
    println!(
        "\nСтриминговый экспорт/импорт: прирост RSS {:.0}/{:.0} МиБ на {:.0} МиБ данных — файлы не читаются в память целиком.",
        export_peak - baseline,
        import_peak - baseline_import,
        total_mib
    );
    assert!(
        export_peak - baseline < total_mib / 2.0,
        "export must stream: RSS grew {:.1} MiB for {total_mib:.0} MiB of data",
        export_peak - baseline
    );

    // Very generous guards against catastrophic regressions only.
    assert!(export_ms < 180_000.0, "export took {export_ms:.0} ms");
    assert!(import_ms < 180_000.0, "import took {import_ms:.0} ms");

    // ------------------------------------------------------------- generators
    println!("\n## 6. Генераторы (1000 вызовов)");
    let passwords = best_of(3, || {
        for _ in 0..1000 {
            let _ = generate_password(&PasswordOptions::default());
        }
    });
    let phrases = best_of(3, || {
        for _ in 0..1000 {
            let _ = generate_passphrase(&PassphraseOptions::default());
        }
    });
    let sample = generate_passphrase(&PassphraseOptions::default()).expect("passphrase");
    println!(
        "{}",
        header(&["генератор", "всего, мс", "мкс/вызов", "пример"])
    );
    println!(
        "{}",
        row(&[
            "пароль 16, все группы".to_string(),
            format!("{passwords:.1}"),
            format!("{:.1}", passwords * 1000.0 / 1000.0 / 1000.0 * 1000.0),
            "…".to_string(),
        ])
    );
    println!(
        "{}",
        row(&[
            "парольная фраза, 5 слов".to_string(),
            format!("{phrases:.1}"),
            format!("{:.1}", phrases),
            sample,
        ])
    );
    assert!(passwords < 2_000.0, "1000 passwords took {passwords:.0} ms");
    assert!(phrases < 2_000.0, "1000 passphrases took {phrases:.0} ms");

    // ------------------------------------------------------------- guard rails
    println!("\n## 7. Защитные лимиты (должны срабатывать мгновенно)");
    println!("{}", header(&["сценарий", "мс", "результат"]));
    let bombs = [
        (
            "Bitwarden: Argon2 память 1 ГиБ × 10^7 проходов",
            br#"{"encrypted":true,"passwordProtected":true,"salt":"c2FsdA==","kdfType":1,"kdfIterations":10000000,"kdfMemory":1048576,"data":"2.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|YmFk|YmFk"}"#.to_vec(),
        ),
        (
            "Bitwarden: PBKDF2 10^9 итераций",
            br#"{"encrypted":true,"passwordProtected":true,"salt":"c2FsdA==","kdfType":0,"kdfIterations":1000000000,"data":"2.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|YmFk|YmFk"}"#.to_vec(),
        ),
    ];
    for (label, payload) in bombs {
        let start = Instant::now();
        let result = import::parse(&payload, Some("password"), None);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{}",
            row(&[
                label.to_string(),
                format!("{ms:.2}"),
                if result.is_err() { "отклонено" } else { "ПРОПУЩЕНО" }.to_string(),
            ])
        );
        assert!(result.is_err(), "{label} must be rejected");
        assert!(ms < 100.0, "{label} took {ms:.0} ms to reject");
    }

    // Release the fixtures so the final RSS figure reflects the code, not the
    // 63 MiB document this test built.
    drop(restored_vault);
    drop(fat_file);
    drop(fat);
    println!(
        "\nRSS процесса: {:.0} МиБ — это high-water mark всех сценариев (в нём 512 МиБ профиля paranoid и 63 МиБ фикстуры),\n         а не потребление приложения на одном хранилище; прирост на самих операциях указан в таблицах выше.",
        rss_mib()
    );
}

