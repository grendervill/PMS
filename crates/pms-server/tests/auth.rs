use std::sync::Arc;

use pms_proto::identity::{
    derive_identity, sign_challenge, AccountId, DeviceId, Identity, IdentityKdf,
};
use pms_server::auth::{Auth, AuthError};
use pms_server::config::Auth as AuthConfig;
use pms_server::store::Store;

struct Fixture {
    _dir: tempfile::TempDir,
    store: Arc<Store>,
    auth: Auth,
    identity: Identity,
    account: AccountId,
    device: DeviceId,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path(), 1_000_000).unwrap());
    let identity = derive_identity(b"master-passphrase", "alice", &IdentityKdf::FAST_FOR_TESTS);
    let account = AccountId::from_public_key(&identity.public);
    store
        .create_account(account, identity.public, 1_000_000, "alice")
        .unwrap();

    let auth = Auth::new(
        Arc::clone(&store),
        AuthConfig {
            token_ttl_hours: 12,
            max_sessions_per_account: 5,
            challenge_ttl_seconds: 60,
        },
    );

    Fixture {
        _dir: dir,
        store,
        auth,
        identity,
        account,
        device: DeviceId::new([5u8; 16]),
    }
}

fn sign(fixture: &Fixture, nonce: [u8; 32]) -> [u8; 64] {
    sign_challenge(&fixture.identity, &fixture.account, &fixture.device, &nonce)
}

fn login(fixture: &Fixture) -> pms_server::auth::SessionToken {
    let challenge = fixture
        .auth
        .issue_challenge(fixture.account, fixture.device)
        .unwrap();
    let signature = sign(fixture, challenge.nonce);
    fixture
        .auth
        .verify(
            fixture.account,
            fixture.device,
            &challenge.nonce,
            &signature,
            &fixture.identity.public,
        )
        .unwrap()
}

#[tokio::test]
async fn challenge_then_verify_yields_working_token() {
    let fixture = setup();
    let challenge = fixture
        .auth
        .issue_challenge(fixture.account, fixture.device)
        .unwrap();
    let signature = sign(&fixture, challenge.nonce);

    let token = fixture
        .auth
        .verify(
            fixture.account,
            fixture.device,
            &challenge.nonce,
            &signature,
            &fixture.identity.public,
        )
        .unwrap();

    let authenticated = fixture.auth.authenticate(token.expose()).unwrap();
    assert_eq!(authenticated.account, fixture.account);
    assert_eq!(authenticated.device, fixture.device);
}

#[tokio::test]
async fn nonce_is_single_use() {
    let fixture = setup();
    let challenge = fixture
        .auth
        .issue_challenge(fixture.account, fixture.device)
        .unwrap();
    let signature = sign(&fixture, challenge.nonce);

    fixture
        .auth
        .verify(
            fixture.account,
            fixture.device,
            &challenge.nonce,
            &signature,
            &fixture.identity.public,
        )
        .unwrap();

    let replay = fixture.auth.verify(
        fixture.account,
        fixture.device,
        &challenge.nonce,
        &signature,
        &fixture.identity.public,
    );
    assert!(matches!(replay, Err(AuthError::Unauthorized)), "{replay:?}");
}

#[tokio::test]
async fn zero_nonce_is_a_bad_request_and_zero_signature_is_unauthorized() {
    let fixture = setup();
    let zero_nonce = [0u8; 32];
    let zero_signature = [0u8; 64];

    assert!(matches!(
        fixture.auth.verify(
            fixture.account,
            fixture.device,
            &zero_nonce,
            &zero_signature,
            &fixture.identity.public
        ),
        Err(AuthError::BadNonce)
    ));

    let challenge = fixture
        .auth
        .issue_challenge(fixture.account, fixture.device)
        .unwrap();
    assert!(matches!(
        fixture.auth.verify(
            fixture.account,
            fixture.device,
            &challenge.nonce,
            &zero_signature,
            &fixture.identity.public
        ),
        Err(AuthError::Unauthorized)
    ));
}

#[tokio::test]
async fn expired_nonce_is_rejected() {
    let fixture = setup();
    let expired = [42u8; 32];
    fixture
        .store
        .insert_nonce(fixture.account, &expired, 1)
        .unwrap();

    let signature = sign(&fixture, expired);
    let result = fixture.auth.verify(
        fixture.account,
        fixture.device,
        &expired,
        &signature,
        &fixture.identity.public,
    );
    assert!(matches!(result, Err(AuthError::Unauthorized)), "{result:?}");
}

#[tokio::test]
async fn attacker_key_cannot_authenticate_account() {
    let fixture = setup();
    let attacker = derive_identity(b"other-passphrase", "alice", &IdentityKdf::FAST_FOR_TESTS);

    let challenge = fixture
        .auth
        .issue_challenge(fixture.account, fixture.device)
        .unwrap();
    let signature = sign_challenge(
        &attacker,
        &fixture.account,
        &fixture.device,
        &challenge.nonce,
    );

    let result = fixture.auth.verify(
        fixture.account,
        fixture.device,
        &challenge.nonce,
        &signature,
        &fixture.identity.public,
    );
    assert!(matches!(result, Err(AuthError::Unauthorized)));
    assert!(fixture
        .store
        .list_blobs(fixture.account)
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn token_lookup_is_exact_and_revocable() {
    let fixture = setup();
    let token = login(&fixture);

    assert!(fixture.auth.authenticate(token.expose()).is_ok());
    assert!(fixture
        .auth
        .authenticate(&format!("{}ff", token.expose()))
        .is_err());

    fixture.auth.revoke_all(fixture.account).unwrap();
    assert!(matches!(
        fixture.auth.authenticate(token.expose()),
        Err(AuthError::Unauthorized)
    ));
}

#[tokio::test]
async fn rate_limiter_backs_off_after_five_failures() {
    let fixture = setup();
    let limiter = fixture.auth.rate_limiter();
    let key = fixture.account.to_string();

    for _ in 0..5 {
        assert!(limiter.check(&key).is_ok());
        limiter.record_failure(&key);
    }

    assert!(matches!(
        limiter.check(&key),
        Err(AuthError::RateLimited { .. })
    ));
    assert!(limiter.check("другой-аккаунт").is_ok());
}

#[tokio::test]
async fn expired_nonces_do_not_count_as_failed_attempts() {
    let fixture = setup();

    // Шесть просроченных nonce — это не подбор, а медленный пользователь.
    for index in 0..6u8 {
        let mut nonce = [0x80u8; 32];
        nonce[1] = index;
        fixture
            .store
            .insert_nonce(fixture.account, &nonce, 1)
            .unwrap();
        let signature = sign(&fixture, nonce);
        let _ = fixture.auth.verify(
            fixture.account,
            fixture.device,
            &nonce,
            &signature,
            &fixture.identity.public,
        );
    }

    let token = login(&fixture);
    assert!(fixture.auth.authenticate(token.expose()).is_ok());
}

#[tokio::test]
async fn revoking_a_token_invalidates_only_that_session() {
    let fixture = setup();
    let first = login(&fixture);
    let second = login(&fixture);

    assert!(fixture.auth.revoke(first.expose()).unwrap());
    assert!(matches!(
        fixture.auth.authenticate(first.expose()),
        Err(AuthError::Unauthorized)
    ));
    assert!(fixture.auth.authenticate(second.expose()).is_ok());
    assert!(fixture.auth.authenticate("00").is_err());
}

#[tokio::test]
async fn one_nonce_per_account_and_eviction_follows_insert_order() {
    let fixture = setup();

    // Десять challenge подряд: активным остаётся только последний nonce.
    let mut nonces = Vec::new();
    for _ in 0..10 {
        let challenge = fixture
            .auth
            .issue_challenge(fixture.account, fixture.device)
            .unwrap();
        nonces.push(challenge.nonce);
    }

    for nonce in &nonces[..9] {
        let signature = sign(&fixture, *nonce);
        let result = fixture.auth.verify(
            fixture.account,
            fixture.device,
            nonce,
            &signature,
            &fixture.identity.public,
        );
        assert!(
            matches!(result, Err(AuthError::Unauthorized)),
            "старый nonce принят"
        );
    }

    let last = *nonces.last().unwrap();
    let signature = sign(&fixture, last);
    assert!(fixture
        .auth
        .verify(
            fixture.account,
            fixture.device,
            &last,
            &signature,
            &fixture.identity.public
        )
        .is_ok());
}
