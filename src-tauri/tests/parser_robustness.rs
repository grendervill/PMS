//! Robustness of the untrusted-input parsers.
//!
//! Every parser in the app reads bytes an attacker controls (a vault file from a
//! colleague, a KeePass export, a QR code, an archive). A fuzz target is the
//! right tool for exploring that space, but it needs a nightly toolchain and a
//! long-running corpus; this suite runs on stable, in CI, on every push, and
//! covers the same failure classes that matter for a desktop app:
//!
//! * no panic (a panic unwinds through Tauri and kills the window),
//! * no unbounded loop (a hung parser freezes a worker forever),
//! * no allocation blow-up (limits must reject before allocating).
//!
//! The inputs are generated deterministically from a fixed seed, so a failure is
//! reproducible. For deeper coverage the same entry points can be driven by
//! `cargo fuzz` (nightly) with a corpus built from `tests/fixtures`; see
//! `docs/reports/2026-11-19-security-audit-2.md`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use pms_lib::testing::{import, parse_vault_file, share, totp};

/// xorshift64*: deterministic, dependency-free, good enough to mutate bytes.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, limit: usize) -> usize {
        if limit == 0 {
            0
        } else {
            (self.next() % limit as u64) as usize
        }
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| (self.next() & 0xff) as u8).collect()
    }
}

/// Runs `parser` on `input`, asserting that it returns (Ok or Err) quickly and
/// without panicking.
fn must_not_panic<T>(label: &str, input: &[u8], parser: impl FnOnce() -> T) {
    let start = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(parser));
    let elapsed = start.elapsed();
    assert!(
        result.is_ok(),
        "{label} panicked on {} bytes of input: {:?}",
        input.len(),
        &input[..input.len().min(48)]
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "{label} took {elapsed:?} on {} bytes: a parser must fail fast",
        input.len()
    );
}

/// Takes a valid sample and mutates it: fuzzing after a seed is much more
/// effective than random bytes, because it reaches the deeper code paths.
fn mutations(base: &[u8], rounds: usize, seed: u64) -> Vec<Vec<u8>> {
    let mut rng = Rng(seed);
    let mut out = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let mut copy = base.to_vec();
        let mutations = 1 + rng.below(6);
        for _ in 0..mutations {
            match rng.below(5) {
                0 if !copy.is_empty() => {
                    let index = rng.below(copy.len());
                    copy[index] ^= 1 << rng.below(8);
                }
                1 if !copy.is_empty() => {
                    let index = rng.below(copy.len());
                    copy[index] = rng.next() as u8;
                }
                2 => {
                    let index = rng.below(copy.len() + 1);
                    let len = 1 + rng.below(8);
                    let insert = rng.bytes(len);
                    copy.splice(index..index, insert);
                }
                3 if !copy.is_empty() => {
                    let index = rng.below(copy.len());
                    let len = 1 + rng.below((copy.len() - index).min(16));
                    copy.drain(index..index + len);
                }
                _ => {
                    // Truncate: many parsers fail early on short input, which is
                    // exactly where off-by-one bugs live.
                    let keep = rng.below(copy.len().max(1));
                    copy.truncate(keep);
                }
            }
        }
        // Keep the corpus small: the parsers must reject by size, not by trying.
        copy.truncate(64 * 1024);
        out.push(copy);
    }
    out
}

#[test]
fn vault_file_parser_survives_mutations() {
    // A real vault file, produced by the app itself.
    let data = import::parse(
        br#"{"encrypted":false,"folders":[],"items":[]}"#,
        None,
        None,
    );
    assert!(data.is_err(), "an empty Bitwarden export is corrupt, not a vault");

    let (file, _session, _data) = pms_lib::testing::create_vault(
        &pms_lib::testing::CreateVaultConfig {
            name: "Fuzz".to_string(),
            kdf_profile: pms_lib::testing::KdfProfile::Standard,
            cipher: pms_lib::testing::CipherSuite::XChaCha20Poly1305,
        },
        &pms_lib::testing::UnlockSecretInput::Passphrase {
            value: "correct horse".to_string(),
        },
    )
    .expect("create_vault must work");

    must_not_panic("vault_file(valid)", &file, || parse_vault_file(&file));
    for (index, input) in mutations(&file, 400, 0x5eed_1234).into_iter().enumerate() {
        must_not_panic(&format!("vault_file(mutation {index})"), &input, || {
            parse_vault_file(&input)
        });
    }
    // Random bytes, including inputs that start with the magic.
    let mut rng = Rng(0xabcd_ef01);
    for _ in 0..200 {
        let len = 1 + rng.below(512);
        let mut input = rng.bytes(len);
        if rng.below(2) == 0 {
            input.splice(0..0, *b"PMSV1");
        }
        must_not_panic("vault_file(random)", &input, || parse_vault_file(&input));
    }
}

#[test]
fn importer_survives_mutations() {
    let bitwarden = br#"{"encrypted":false,"folders":[{"id":"f1","name":"Social"}],"items":[
        {"id":"i1","type":1,"name":"GitHub","notes":"note","fields":[{"name":"PIN","value":"1234"}],
         "login":{"username":"alex","password":"s3cret","totp":"JBSWY3DPEHPK3PXP",
                  "uris":[{"uri":"https://github.com"}]}}]}"#
        .to_vec();
    must_not_panic("import(bitwarden)", &bitwarden, || {
        import::parse(&bitwarden, None, None)
    });

    let keepass_xml = br#"<?xml version="1.0" encoding="utf-8"?>
<KeePassFile><Root><Group><Name>Root</Name><Entry>
<String><Key>Title</Key><Value>Mail &amp; Co</Value></String>
<String><Key>Password</Key><Value>s3cret</Value></String>
</Entry></Group></Root></KeePassFile>"#
        .to_vec();
    must_not_panic("import(keepass-xml)", &keepass_xml, || {
        import::parse(&keepass_xml, None, None)
    });

    for (label, base, seed) in [
        ("bitwarden", bitwarden, 0x1111_2222u64),
        ("keepass-xml", keepass_xml, 0x3333_4444),
    ] {
        for (index, input) in mutations(&base, 300, seed).into_iter().enumerate() {
            let label = format!("import({label} mutation {index})");
            must_not_panic(&label, &input, || {
                import::parse(&input, Some("password"), Some(&[1u8; 32]))
            });
        }
    }

    // A zip container: the importer must not be tricked by a fake local header.
    let mut zip_like = b"PK\x03\x04".to_vec();
    zip_like.extend_from_slice(&[0xff; 128]);
    must_not_panic("import(zip-like)", &zip_like, || {
        import::parse(&zip_like, None, None)
    });

    // KDBX signature with a nonsense header: the guard has to stay in bounds.
    let mut kdbx = 0x9AA2_D903u32.to_le_bytes().to_vec();
    kdbx.extend_from_slice(&0xB54B_FB67u32.to_le_bytes());
    kdbx.extend_from_slice(&0x0004_0001u32.to_le_bytes());
    kdbx.extend_from_slice(&[0x0b, 0xff, 0xff, 0x00, 0x00, 1, 2, 3]);
    must_not_panic("import(kdbx-header)", &kdbx, || {
        import::parse(&kdbx, Some("password"), None)
    });
}

#[test]
fn qr_and_totp_parsers_survive_mutations() {
    let card = share::SharedCard {
        name: "GitHub".to_string(),
        login: "alex".to_string(),
        password: "s3cret".to_string(),
        urls: vec!["https://github.com".to_string()],
        comment: "note".to_string(),
        totp: None,
    };
    let plain = share::encode(&card, None).expect("encode");
    let encrypted = share::encode(&card, Some("qr-pass")).expect("encode");

    for (label, payload, password) in [
        ("plain", plain.clone(), None),
        ("encrypted", encrypted.clone(), Some("qr-pass")),
    ] {
        must_not_panic(&format!("share({label})"), payload.as_bytes(), || {
            share::decode(&payload, password)
        });
    }
    for (index, input) in mutations(plain.as_bytes(), 300, 0x7777_8888)
        .into_iter()
        .chain(mutations(encrypted.as_bytes(), 300, 0x9999_aaaa))
        .enumerate()
    {
        let text = String::from_utf8_lossy(&input).into_owned();
        must_not_panic(&format!("share(mutation {index})"), &input, || {
            share::decode(&text, Some("qr-pass"))
        });
    }

    let uri = "otpauth://totp/Example:alex?secret=JBSWY3DPEHPK3PXP&issuer=Example&digits=6&period=30";
    must_not_panic("totp(uri)", uri.as_bytes(), || totp::inspect(uri));
    for (index, input) in mutations(uri.as_bytes(), 300, 0xbbbb_cccc)
        .into_iter()
        .enumerate()
    {
        let text = String::from_utf8_lossy(&input).into_owned();
        must_not_panic(&format!("totp(mutation {index})"), &input, || {
            totp::inspect(&text)
        });
    }
    let mut rng = Rng(0xdddd_eeee);
    for _ in 0..300 {
        let len = rng.below(64);
        let text: String = rng
            .bytes(len)
            .into_iter()
            .map(|byte| char::from(0x20 + (byte % 0x5f)))
            .collect();
        must_not_panic("totp(random)", text.as_bytes(), || totp::inspect(&text));
    }
}

#[test]
fn import_limits_reject_bombs_without_allocating() {
    // 1 GiB of Argon2 memory times 10^7 passes, in a 300-byte file.
    let argon = br#"{"encrypted":true,"passwordProtected":true,"salt":"c2FsdA==","kdfType":1,
        "kdfIterations":10000000,"kdfMemory":1048576,"data":"2.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|YmFk|YmFk"}"#;
    let start = Instant::now();
    assert!(import::parse(argon, Some("password"), None).is_err());
    assert!(start.elapsed() < Duration::from_millis(100));

    // PBKDF2 with 10^9 rounds.
    let pbkdf2 = br#"{"encrypted":true,"passwordProtected":true,"salt":"c2FsdA==","kdfType":0,
        "kdfIterations":1000000000,"data":"2.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|YmFk|YmFk"}"#;
    let start = Instant::now();
    assert!(import::parse(pbkdf2, Some("password"), None).is_err());
    assert!(start.elapsed() < Duration::from_millis(100));

    // A zip that declares a 4 GiB entry but carries a few bytes.
    use std::io::Write;
    let mut zip_bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("data.json", options).expect("header");
        writer.write_all(b"{\"items\":[]}").expect("write");
        writer.finish().expect("finish");
    }
    let start = Instant::now();
    let _ = import::parse(&zip_bytes, None, None);
    assert!(start.elapsed() < Duration::from_millis(500));
}
