use super::{sharing, store};
use rusqlite::{Connection, params};

fn database() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE users(uid TEXT PRIMARY KEY,name TEXT,email TEXT);INSERT INTO users VALUES('a','Alice','a@test'),('b','Bob','b@test'),('c','Carol','c@test'),('d','Dan','d@test');").unwrap();
    store::ensure(&c).unwrap();
    crate::api::collaboration::ensure_collaboration_schema(&c).unwrap();
    c
}
fn count(c: &Connection, table: &str) -> i64 {
    c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn personal_shares_are_atomic_durable_and_idempotent_and_reuse_the_direct_chat() {
    let c = database();
    let file = store::folder(
        &c,
        "a",
        "00000000-0000-4000-8000-000000000001",
        "Reference",
        None,
    )
    .unwrap();
    let first = store::grant(&c, "a", &file.uid, "b", Some("viewer")).unwrap();
    assert!(first.changed);
    let channel = first.channel_uid.unwrap();
    assert_eq!(count(&c, "mx_channels"), 1);
    assert_eq!(count(&c, "mx_messages"), 1);
    assert_eq!(count(&c, "mx_notifications"), 1);
    let card = sharing::message_links(&c, first.message_uid.as_ref().unwrap(), "b").unwrap();
    assert_eq!(card[0]["item"]["name"], "Reference");
    assert_eq!(card[0]["item"]["permission"], "viewer");
    assert!(card[0]["item"].get("object_key").is_none());
    assert_eq!(
        sharing::message_links(&c, first.message_uid.as_ref().unwrap(), "c").unwrap()[0]["unavailable"],
        true
    );
    assert!(
        !store::grant(&c, "a", &file.uid, "b", Some("viewer"))
            .unwrap()
            .changed
    );
    assert_eq!(count(&c, "mx_messages"), 1);
    assert_eq!(count(&c, "mx_notifications"), 1);
    assert_eq!(
        store::grant(&c, "a", &file.uid, "b", Some("editor"))
            .unwrap()
            .channel_uid
            .as_deref(),
        Some(channel.as_str())
    );
    assert_eq!(count(&c, "mx_channels"), 1);
    assert_eq!(count(&c, "mx_messages"), 2);
    store::grant(&c, "a", &file.uid, "b", None).unwrap();
    assert_eq!(
        sharing::message_links(&c, first.message_uid.as_ref().unwrap(), "b").unwrap()[0]["unavailable"],
        true
    );
    assert_eq!(count(&c, "mx_messages"), 2);
}

#[test]
fn notification_failure_rolls_back_access_message_and_activity() {
    let c = database();
    let file = store::folder(
        &c,
        "a",
        "00000000-0000-4000-8000-000000000001",
        "Reference",
        None,
    )
    .unwrap();
    crate::api::notifications::ensure_notification_schema(&c).unwrap();
    c.execute_batch("CREATE TRIGGER fail_notification BEFORE INSERT ON mx_notifications BEGIN SELECT RAISE(ABORT,'test notification failure');END;").unwrap();
    let activity = count(&c, "mx_drive_activity");
    assert!(store::grant(&c, "a", &file.uid, "b", Some("viewer")).is_err());
    assert_eq!(count(&c, "mx_drive_grants"), 0);
    assert_eq!(count(&c, "mx_messages"), 0);
    assert_eq!(count(&c, "mx_channels"), 0);
    assert_eq!(count(&c, "mx_drive_activity"), activity);
    assert!(store::accessible(&c, &file.uid, "b", false).is_err());
}

#[test]
fn group_shares_follow_notification_settings_and_current_membership() {
    let c = database();
    let file = store::folder(
        &c,
        "a",
        "00000000-0000-4000-8000-000000000001",
        "Reference",
        None,
    )
    .unwrap();
    c.execute("INSERT INTO mx_channels(uid,kind,name,created_by,created_at) VALUES('group','group','Team','a',1)",[]).unwrap();
    for (user, level) in [
        ("a", "all"),
        ("b", "all"),
        ("c", "muted"),
        ("d", "mentions"),
    ] {
        c.execute("INSERT INTO mx_channel_members(channel_uid,user_uid,notification_level,joined_at) VALUES('group',?1,?2,1)",params![user,level]).unwrap();
    }
    let first = sharing::grant(&c, "a", &file.uid, "group", Some("viewer")).unwrap();
    assert_eq!(first.channel_uid.as_deref(), Some("group"));
    assert_eq!(count(&c, "mx_notifications"), 1);
    assert_eq!(
        c.query_row("SELECT recipient_uid FROM mx_notifications", [], |r| r
            .get::<_, String>(
            0
        ))
        .unwrap(),
        "b"
    );
    assert!(
        !sharing::grant(&c, "a", &file.uid, "group", Some("viewer"))
            .unwrap()
            .changed
    );
    assert_eq!(count(&c, "mx_messages"), 1);
    c.execute(
        "DELETE FROM mx_channel_members WHERE channel_uid='group' AND user_uid='b'",
        [],
    )
    .unwrap();
    assert_eq!(
        sharing::message_links(&c, first.message_uid.as_ref().unwrap(), "b").unwrap()[0]["unavailable"],
        true
    );
    assert_eq!(
        sharing::message_links(&c, first.message_uid.as_ref().unwrap(), "c").unwrap()[0]["item"]["name"],
        "Reference"
    );
    sharing::grant(&c, "a", &file.uid, "group", None).unwrap();
    assert_eq!(
        sharing::message_links(&c, first.message_uid.as_ref().unwrap(), "c").unwrap()[0]["unavailable"],
        true
    );
}
