use std::net::TcpListener;

use serde::{Deserialize, Serialize};

use crate::accounts::{self, AccountAuth};
use crate::db::Database;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgePorts {
    pub imap: u16,
    pub smtp: u16,
}

pub(crate) fn select(db: &Database) -> AppResult<BridgePorts> {
    if let Some(persisted) = persisted(db)? {
        if available(persisted) {
            return Ok(persisted);
        }
    }
    free_pair()
}

fn persisted(db: &Database) -> AppResult<Option<BridgePorts>> {
    Ok(accounts::list(db)?
        .into_iter()
        .filter(|account| account.auth_type == AccountAuth::Proton)
        .find_map(|account| {
            (account.imap_port != 0
                && account.smtp_port != 0
                && account.imap_port != account.smtp_port)
                .then_some(BridgePorts {
                    imap: account.imap_port,
                    smtp: account.smtp_port,
                })
        }))
}

fn available(ports: BridgePorts) -> bool {
    let imap = TcpListener::bind(("127.0.0.1", ports.imap));
    let smtp = TcpListener::bind(("127.0.0.1", ports.smtp));
    imap.is_ok() && smtp.is_ok()
}

pub(crate) fn free_pair() -> AppResult<BridgePorts> {
    let imap = TcpListener::bind(("127.0.0.1", 0))?;
    let smtp = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(BridgePorts {
        imap: imap.local_addr()?.port(),
        smtp: smtp.local_addr()?.port(),
    })
}

pub(crate) fn update_account_rows(db: &Database, ports: BridgePorts) -> AppResult<()> {
    let mut conn = db
        .conn
        .lock()
        .map_err(|_| AppError::Message("database unavailable".into()))?;
    let tx = conn.transaction()?;
    tx.execute(
        "update accounts set imap_port = ?1, smtp_port = ?2 where auth_type = 'proton'",
        rusqlite::params![ports.imap, ports.smtp],
    )?;
    tx.commit()?;
    Ok(())
}
