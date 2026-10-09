//! Durable deduplication committed in the same SQLite transaction as the write.
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub struct ClientOperation {
    key: String,
    fingerprint: String,
}

impl ClientOperation {
    pub fn parse(key: Option<String>, payload: &Value) -> Result<Option<Self>, String> {
        let Some(key) = key else { return Ok(None) };
        if uuid::Uuid::parse_str(&key).is_err() {
            return Err("operation_uid must be a UUID".into());
        }
        let fingerprint = Sha256::digest(payload.to_string().as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Some(Self { key, fingerprint }))
    }
}

pub fn ensure_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS mx_client_operations (
        actor_uid TEXT NOT NULL, scope TEXT NOT NULL, operation_uid TEXT NOT NULL,
        fingerprint TEXT NOT NULL, resource_uid TEXT NOT NULL,
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        PRIMARY KEY(actor_uid, scope, operation_uid)
    );",
    )
}

pub fn existing(
    connection: &Connection,
    actor: &str,
    scope: &str,
    operation: Option<&ClientOperation>,
) -> rusqlite::Result<Option<String>> {
    let Some(operation) = operation else {
        return Ok(None);
    };
    let stored = connection.query_row(
        "SELECT fingerprint, resource_uid FROM mx_client_operations WHERE actor_uid=?1 AND scope=?2 AND operation_uid=?3",
        params![actor, scope, operation.key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    ).optional()?;
    match stored {
        Some((fingerprint, uid)) if fingerprint == operation.fingerprint => Ok(Some(uid)),
        Some(_) => Err(rusqlite::Error::InvalidParameterName("MX_OPERATION:This operation was already submitted with different entries. Reconcile the previous save before starting a new one.".into())),
        None => Ok(None),
    }
}

pub fn complete(
    connection: &Connection,
    actor: &str,
    scope: &str,
    operation: Option<&ClientOperation>,
    uid: &str,
) -> rusqlite::Result<()> {
    if let Some(operation) = operation {
        connection.execute("INSERT INTO mx_client_operations(actor_uid,scope,operation_uid,fingerprint,resource_uid) VALUES(?1,?2,?3,?4,?5)", params![actor,scope,operation.key,operation.fingerprint,uid])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operation_is_payload_bound_account_scoped_and_atomic() {
        let connection = Connection::open_in_memory().unwrap();
        ensure_schema(&connection).unwrap();
        let key = uuid::Uuid::new_v4().to_string();
        let operation =
            ClientOperation::parse(Some(key.clone()), &serde_json::json!({"name":"one"}))
                .unwrap()
                .unwrap();
        {
            let tx = connection.unchecked_transaction().unwrap();
            complete(&tx, "a", "records:x", Some(&operation), "resource").unwrap();
        }
        assert!(
            existing(&connection, "a", "records:x", Some(&operation))
                .unwrap()
                .is_none()
        );
        {
            let tx = connection.unchecked_transaction().unwrap();
            complete(&tx, "a", "records:x", Some(&operation), "resource").unwrap();
            tx.commit().unwrap();
        }
        assert_eq!(
            existing(&connection, "a", "records:x", Some(&operation))
                .unwrap()
                .as_deref(),
            Some("resource")
        );
        assert!(
            existing(&connection, "b", "records:x", Some(&operation))
                .unwrap()
                .is_none()
        );
        assert!(
            existing(&connection, "a", "messages:x", Some(&operation))
                .unwrap()
                .is_none()
        );
        let changed = ClientOperation::parse(Some(key), &serde_json::json!({"name":"two"}))
            .unwrap()
            .unwrap();
        assert!(existing(&connection, "a", "records:x", Some(&changed)).is_err());
        assert!(ClientOperation::parse(Some("bad".into()), &Value::Null).is_err());
    }

    #[test]
    fn concurrent_replays_commit_exactly_one_resource() {
        let path =
            std::env::temp_dir().join(format!("mx-operation-test-{}.db", uuid::Uuid::new_v4()));
        let connection = Connection::open(&path).unwrap();
        ensure_schema(&connection).unwrap();
        connection
            .execute_batch("CREATE TABLE resources(uid TEXT PRIMARY KEY);")
            .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let key = uuid::Uuid::new_v4().to_string();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (path, barrier, key) = (path.clone(), barrier.clone(), key.clone());
                std::thread::spawn(move || {
                    let mut connection = Connection::open(path).unwrap();
                    connection
                        .busy_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    let operation =
                        ClientOperation::parse(Some(key), &serde_json::json!({"value":42}))
                            .unwrap()
                            .unwrap();
                    barrier.wait();
                    let tx = connection
                        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                        .unwrap();
                    if let Some(uid) = existing(&tx, "actor", "create", Some(&operation)).unwrap() {
                        return uid;
                    }
                    let uid = uuid::Uuid::new_v4().to_string();
                    tx.execute("INSERT INTO resources(uid) VALUES(?1)", [&uid])
                        .unwrap();
                    complete(&tx, "actor", "create", Some(&operation), &uid).unwrap();
                    tx.commit().unwrap();
                    uid
                })
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert!(results.iter().all(|uid| uid == &results[0]));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM resources", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(connection);
        std::fs::remove_file(path).unwrap();
    }
}
