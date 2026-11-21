//! Приём тела регистрации на диск, чтобы не держать сейф в памяти.
//!
//! `POST /v1/accounts` может нести целый существующий сейф. Если читать его в
//! `Bytes`, одна открытая регистрация с гигабайтным телом убивает сервер на
//! 2 ГиБ RAM. Поэтому тело пишется во временный файл, хеш считается по дороге,
//! и только потом проверяется подпись, которая этот хеш покрывает.

use std::path::Path;

use tokio::io::AsyncRead;

use super::temp::TempFile;
use super::{create_private_dir, io_error, Store, StoreError};

pub struct Staged {
    temp: TempFile,
    len: u64,
    digest: String,
    /// Держим резерв, пока тело лежит в `staging/`: освободится при удалении.
    _reservation: super::Reservation,
}

impl Staged {
    pub fn path(&self) -> &Path {
        self.temp.path()
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

impl Store {
    pub async fn stage_upload<R: AsyncRead + Unpin>(
        &self,
        mut body: R,
        len: u64,
    ) -> Result<Staged, StoreError> {
        let directory = self.root().join("staging");
        create_private_dir(&directory)?;

        let mut name = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut name);
        let reservation = self.reserve(len)?;
        let temp = TempFile::at(directory.join(hex::encode(name)));

        let policy = self.fsync_policy();
        let digest = super::hash_stream_to_file(temp.path(), &mut body, len, policy).await?;
        let written = tokio::fs::metadata(temp.path())
            .await
            .map_err(io_error)?
            .len();
        if written != len {
            return Err(StoreError::Truncated {
                expected: len,
                got: written,
            });
        }

        Ok(Staged {
            temp,
            len,
            digest,
            _reservation: reservation,
        })
    }
}
