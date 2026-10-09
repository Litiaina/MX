//! Bounded metadata pages and key-seek preview neighbors. Never fetch file bytes.
use super::store;
use rusqlite::{Connection, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::{Value, json};

fn mine() -> String {
    "mine".into()
}
fn limit() -> usize {
    50
}
fn sort() -> String {
    "modified_desc".into()
}
#[derive(Deserialize)]
pub(super) struct Listing {
    #[serde(default = "mine")]
    pub view: String,
    pub parent_uid: Option<String>,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "limit")]
    pub limit: usize,
    #[serde(default = "sort")]
    pub sort: String,
    #[serde(default)]
    pub kind: String,
}
impl Default for Listing {
    fn default() -> Self {
        Self {
            view: mine(),
            parent_uid: None,
            q: String::new(),
            offset: 0,
            limit: limit(),
            sort: sort(),
            kind: String::new(),
        }
    }
}
// Hidden-tree traversal is restricted to owners relevant to this actor. Mine /
// trash never enumerate grants, let alone unrelated users' private namespaces.
const CTE: &str = r#"WITH RECURSIVE roots(uid) AS (
 SELECT item_uid FROM mx_drive_grants WHERE user_uid=?1 AND ?4 NOT IN ('mine','trash')
 UNION SELECT g.item_uid FROM mx_drive_space_grants g JOIN mx_channel_members m ON m.channel_uid=g.space_uid
 JOIN mx_channels s ON s.uid=g.space_uid WHERE m.user_uid=?1 AND s.kind<>'direct' AND s.archived_at IS NULL AND ?4 NOT IN ('mine','trash')
), shared(uid) AS (SELECT uid FROM roots UNION SELECT i.uid FROM mx_drive_items i JOIN shared s ON i.parent_uid=s.uid),
 relevant(owner_uid) AS (SELECT ?1 UNION SELECT i.owner_uid FROM mx_drive_items i JOIN roots r ON r.uid=i.uid),
 hidden(uid) AS (SELECT uid FROM mx_drive_items WHERE owner_uid IN relevant AND trashed_at IS NOT NULL
 UNION SELECT i.uid FROM mx_drive_items i JOIN hidden h ON i.parent_uid=h.uid) "#;
const FILTER: &str = r#"(i.owner_uid=?1 OR i.uid IN shared)
 AND (?2 IS NULL OR i.parent_uid=?2) AND (?3='' OR INSTR(LOWER(i.name),LOWER(?3))>0)
 AND (?5='' OR i.kind=?5)
 AND (CASE ?4 WHEN 'trash' THEN i.owner_uid=?1 AND i.trashed_at IS NOT NULL ELSE i.uid NOT IN hidden END)
 AND (CASE ?4 WHEN 'mine' THEN i.owner_uid=?1 AND (?2 IS NOT NULL OR ?3<>'' OR i.parent_uid IS NULL)
 WHEN 'shared' THEN i.owner_uid<>?1 AND (?2 IS NOT NULL OR ?3<>'' OR i.uid IN roots)
 WHEN 'starred' THEN EXISTS(SELECT 1 FROM mx_drive_stars s WHERE s.item_uid=i.uid AND s.user_uid=?1)
 WHEN 'recent' THEN i.kind='file' ELSE 1 END)"#;
fn validate(c: &Connection, actor: &str, q: &Listing) -> rusqlite::Result<(&'static str, bool)> {
    if !["mine", "shared", "recent", "starred", "trash"].contains(&q.view.as_str())
        || !["", "file", "folder"].contains(&q.kind.as_str())
        || !(1..=100).contains(&q.limit)
        || q.q.len() > 240
    {
        return Err(store::problem(
            400,
            "Choose a valid view, search and page size (1–100).",
        ));
    }
    if let Some(uid) = &q.parent_uid {
        if store::accessible(c, uid, actor, false)?.kind != "folder" {
            return Err(store::problem(400, "Not a folder."));
        }
    }
    match q.sort.as_str() {
        "name_asc" => Ok(("i.name COLLATE NOCASE", false)),
        "name_desc" => Ok(("i.name COLLATE NOCASE", true)),
        "modified_asc" => Ok(("i.updated_at", false)),
        "modified_desc" => Ok(("i.updated_at", true)),
        _ => Err(store::problem(400, "Choose name or modified-date sorting.")),
    }
}
pub(super) fn list(c: &Connection, actor: &str, q: &Listing) -> rusqlite::Result<Value> {
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Deferred)?;
    let (column, descending) = validate(&tx, actor, q)?;
    let total: usize = tx.query_row(
        &format!("{CTE} SELECT COUNT(*) FROM mx_drive_items i WHERE {FILTER}"),
        params![actor, q.parent_uid, q.q, q.view, q.kind],
        |r| r.get::<_, i64>(0),
    )? as usize;
    let offset = q.offset.min(total.saturating_sub(1) / q.limit * q.limit);
    // Project stars with the page, rather than one extra query per owned row.
    let item_sql=store::ITEM_SQL.replacen(" FROM mx_drive_items i",",EXISTS(SELECT 1 FROM mx_drive_stars s WHERE s.item_uid=i.uid AND s.user_uid=?1) FROM mx_drive_items i",1);
    let sql = format!(
        "{CTE} {} WHERE {FILTER} ORDER BY i.kind DESC,{column} {},i.uid ASC LIMIT ?6 OFFSET ?7",
        item_sql,
        if descending { "DESC" } else { "ASC" }
    );
    let mut s = tx.prepare(&sql)?;
    let rows = s
        .query_map(
            params![
                actor,
                q.parent_uid,
                q.q,
                q.view,
                q.kind,
                q.limit as i64,
                offset as i64
            ],
            |row| {
                let mut item = store::row_item(row)?;
                item.starred = row.get(13)?;
                Ok(item)
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut items = Vec::with_capacity(rows.len());
    for mut item in rows {
        if item.owner_uid == actor {
            item.permission = "owner".into();
        } else {
            item = store::accessible(&tx, &item.uid, actor, false)?;
        }
        items.push(item);
    }
    let mut breadcrumbs = Vec::new();
    if let Some(uid) = &q.parent_uid {
        for item in store::ancestors(&tx, uid)?.into_iter().rev() {
            if let Ok(item) = store::accessible(&tx, &item.uid, actor, false) {
                breadcrumbs.push(json!({"uid":item.uid,"name":item.name}));
            }
        }
    }
    drop(s);
    tx.commit()?;
    Ok(
        json!({"items":items,"total":total,"offset":offset,"limit":q.limit,"page":offset/q.limit+1,"total_pages":total.div_ceil(q.limit).max(1),"has_more":offset+items.len()<total,"breadcrumbs":breadcrumbs}),
    )
}
pub(super) fn neighbors(
    c: &Connection,
    actor: &str,
    uid: &str,
    q: &Listing,
) -> rusqlite::Result<Value> {
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Deferred)?;
    let (column, descending) = validate(&tx, actor, q)?;
    let item = store::accessible(&tx, uid, actor, false)?;
    if item.kind != "file" || q.view == "trash" {
        return Err(store::problem(400, "Select an active file."));
    }
    let matches:bool=tx.query_row(&format!("{CTE} SELECT EXISTS(SELECT 1 FROM mx_drive_items i WHERE {FILTER} AND i.kind='file' AND i.uid=?6)"),params![actor,q.parent_uid,q.q,q.view,q.kind,uid],|r|r.get(0))?;
    if !matches {
        return Err(store::problem(
            409,
            "This file is no longer in this listing. Refresh Drive.",
        ));
    }
    let scalar = if q.sort.starts_with("name") {
        rusqlite::types::Value::Text(item.name.clone())
    } else {
        rusqlite::types::Value::Integer(item.updated_at)
    };
    let before = if descending { " > " } else { " < " };
    let after = if descending { " < " } else { " > " };
    let previous_pred = format!("({column}{before}?6 OR ({column}=?6 AND i.uid<?7))");
    let next_pred = format!("({column}{after}?6 OR ({column}=?6 AND i.uid>?7))");
    let seek = |predicate: &str, reverse: bool| -> rusqlite::Result<Option<store::Item>> {
        let direction = if descending ^ reverse { "DESC" } else { "ASC" };
        let tie = if reverse { "DESC" } else { "ASC" };
        let mut s=tx.prepare(&format!("{CTE} SELECT i.uid FROM mx_drive_items i WHERE {FILTER} AND i.kind='file' AND {predicate} ORDER BY {column} {direction},i.uid {tie} LIMIT 1"))?;
        let ids = s
            .query_map(
                params![actor, q.parent_uid, q.q, q.view, q.kind, scalar, uid],
                |r| r.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.first()
            .map(|uid| store::accessible(&tx, uid, actor, false))
            .transpose()
    };
    let previous = seek(&previous_pred, true)?;
    let next = seek(&next_pred, false)?;
    let total: i64 = tx.query_row(
        &format!("{CTE} SELECT COUNT(*) FROM mx_drive_items i WHERE {FILTER} AND i.kind='file'"),
        params![actor, q.parent_uid, q.q, q.view, q.kind],
        |r| r.get(0),
    )?;
    let before_count:i64=tx.query_row(&format!("{CTE} SELECT COUNT(*) FROM mx_drive_items i WHERE {FILTER} AND i.kind='file' AND {previous_pred}"),params![actor,q.parent_uid,q.q,q.view,q.kind,scalar,uid],|r|r.get(0))?;
    tx.commit()?;
    Ok(json!({"previous":previous,"next":next,"position":before_count+1,"total":total}))
}

#[cfg(test)]
mod tests {
    use super::super::sharing;
    use super::*;
    pub(super) fn fixture(count: usize) -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE users(uid TEXT PRIMARY KEY,name TEXT,email TEXT);INSERT INTO users VALUES('a','Alice','a@test'),('b','Bob','b@test'),('c','Carol','c@test');").unwrap();
        store::ensure(&c).unwrap();
        let tx = c.transaction().unwrap();
        for owner in ["a", "b"] {
            for index in 0..count {
                tx.execute("INSERT INTO mx_drive_items(uid,owner_uid,kind,name,created_at,updated_at,revision) VALUES(?1,?2,'file',?3,1,?4,1)",params![format!("{owner}-{index:05}"),owner,format!("File {index:05}.txt"),(index/3) as i64]).unwrap();
            }
        }
        tx.commit().unwrap();
        c
    }
    #[test]
    fn large_namespace_is_bounded_paginated_sorted_and_tenant_scoped() {
        let c = fixture(10_123);
        let mut q = Listing {
            sort: "name_asc".into(),
            ..Default::default()
        };
        let first = list(&c, "a", &q).unwrap();
        assert_eq!(first["total"], 10_123);
        assert_eq!(first["items"].as_array().unwrap().len(), 50);
        assert_eq!(first["items"][0]["uid"], "a-00000");
        q.offset = 10_100;
        let last = list(&c, "a", &q).unwrap();
        assert_eq!(last["items"].as_array().unwrap().len(), 23);
        assert_eq!(last["has_more"], false);
        q.offset = usize::MAX;
        assert_eq!(list(&c, "a", &q).unwrap()["offset"], 10_100);
        q.q = "File 000".into();
        q.offset = 0;
        q.limit = 25;
        let search = list(&c, "a", &q).unwrap();
        assert_eq!(search["total"], 100);
        assert_eq!(search["items"].as_array().unwrap().len(), 25);
        q.kind = "folder".into();
        assert_eq!(list(&c, "a", &q).unwrap()["total"], 0);
        q.kind = "".into();
        q.q = "".into();
        q.view = "shared".into();
        assert_eq!(list(&c, "a", &q).unwrap()["total"], 0);
    }
    #[test]
    fn page_sizes_sort_and_folder_access_are_validated() {
        let c = fixture(1);
        let mut q = Listing::default();
        for limit in [0, 101, usize::MAX] {
            q.limit = limit;
            assert!(list(&c, "a", &q).is_err());
        }
        q.limit = 50;
        q.sort = "name; DROP TABLE users".into();
        assert!(list(&c, "a", &q).is_err());
        q.sort = "name_asc".into();
        q.parent_uid = Some("b-00000".into());
        assert!(list(&c, "a", &q).is_err());
    }
    #[test]
    fn seek_preview_crosses_pages_and_handles_ties_in_every_sort() {
        let c = fixture(125);
        for sort in ["name_asc", "name_desc", "modified_asc", "modified_desc"] {
            let q = Listing {
                sort: sort.into(),
                limit: 100,
                ..Default::default()
            };
            let mut all = list(&c, "a", &q).unwrap()["items"]
                .as_array()
                .unwrap()
                .clone();
            let next = Listing { offset: 100, ..q };
            all.extend(
                list(&c, "a", &next).unwrap()["items"]
                    .as_array()
                    .unwrap()
                    .clone(),
            );
            for index in [0, 49, 50, 99, 100, 124] {
                let n = neighbors(&c, "a", all[index]["uid"].as_str().unwrap(), &next).unwrap();
                assert_eq!(n["position"], index + 1);
                assert_eq!(n["total"], 125);
                assert_eq!(
                    n["previous"]["uid"],
                    if index > 0 {
                        all[index - 1]["uid"].clone()
                    } else {
                        Value::Null
                    }
                );
                assert_eq!(
                    n["next"]["uid"],
                    all.get(index + 1)
                        .map(|i| i["uid"].clone())
                        .unwrap_or(Value::Null)
                );
            }
        }
    }
    #[test]
    fn deletions_never_resurrect_and_empty_last_pages_clamp() {
        let c = fixture(51);
        let q = Listing {
            offset: 50,
            sort: "name_asc".into(),
            ..Default::default()
        };
        store::mutate(&c, "a", "a-00050", 1, "trash", None, None).unwrap();
        let p = list(&c, "a", &q).unwrap();
        assert_eq!(p["offset"], 0);
        assert_eq!(p["total"], 50);
        assert!(
            !p["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["uid"] == "a-00050")
        );
        let n = neighbors(&c, "a", "a-00049", &q).unwrap();
        assert!(n["next"].is_null());
        assert!(neighbors(&c, "a", "a-00050", &q).is_err());
    }
    fn space(c: &Connection, kind: &str) {
        c.execute("INSERT INTO mx_channels(uid,kind,name,created_by,created_at) VALUES('space',?1,'Operations','a',1)",[kind]).unwrap();
        for actor in ["a", "b"] {
            c.execute("INSERT INTO mx_channel_members(channel_uid,user_uid,role,joined_at) VALUES('space',?1,'member',1)",[actor]).unwrap();
        }
    }
    #[test]
    fn space_grants_inherit_follow_membership_and_never_leak_private_siblings() {
        let c = fixture(2);
        space(&c, "group");
        let folder = store::folder(
            &c,
            "a",
            &uuid::Uuid::new_v4().to_string(),
            "Private parent",
            None,
        )
        .unwrap();
        let shared = store::folder(
            &c,
            "a",
            &uuid::Uuid::new_v4().to_string(),
            "Shared folder",
            Some(&folder.uid),
        )
        .unwrap();
        c.execute(
            "UPDATE mx_drive_items SET parent_uid=?1 WHERE uid='a-00000'",
            [&shared.uid],
        )
        .unwrap();
        sharing::grant(&c, "a", &shared.uid, "space", Some("viewer")).unwrap();
        assert_eq!(
            store::accessible(&c, "a-00000", "b", false)
                .unwrap()
                .permission,
            "viewer"
        );
        assert!(store::accessible(&c, "a-00001", "b", false).is_err());
        assert!(store::accessible(&c, &folder.uid, "b", false).is_err());
        let q = Listing {
            view: "shared".into(),
            parent_uid: Some(shared.uid.clone()),
            ..Default::default()
        };
        let p = list(&c, "b", &q).unwrap();
        assert_eq!(p["total"], 1);
        assert_eq!(p["breadcrumbs"].as_array().unwrap().len(), 1);
        assert!(store::mutate(&c, "b", "a-00000", 1, "rename", Some("Blocked"), None).is_err());
        sharing::grant(&c, "a", &shared.uid, "space", Some("editor")).unwrap();
        store::grant(&c, "a", "a-00000", "b", Some("viewer")).unwrap();
        assert_eq!(
            store::accessible(&c, "a-00000", "b", false)
                .unwrap()
                .permission,
            "editor"
        );
        c.execute(
            "DELETE FROM mx_channel_members WHERE channel_uid='space' AND user_uid='b'",
            [],
        )
        .unwrap();
        assert_eq!(
            store::accessible(&c, "a-00000", "b", false)
                .unwrap()
                .permission,
            "viewer"
        );
        store::grant(&c, "a", "a-00000", "b", None).unwrap();
        assert!(store::accessible(&c, "a-00000", "b", false).is_err());
        assert_eq!(
            list(
                &c,
                "b",
                &Listing {
                    view: "shared".into(),
                    ..Default::default()
                }
            )
            .unwrap()["total"],
            0
        );
        c.execute("INSERT INTO mx_channel_members(channel_uid,user_uid,role,joined_at) VALUES('space','c','member',1)",[]).unwrap();
        assert_eq!(
            store::accessible(&c, "a-00000", "c", false)
                .unwrap()
                .permission,
            "editor"
        );
        sharing::grant(&c, "a", &shared.uid, "space", None).unwrap();
        assert!(store::accessible(&c, "a-00000", "c", false).is_err());
        assert_eq!(c.query_row("SELECT COUNT(*) FROM mx_drive_activity WHERE action='sharing.changed' AND json_extract(data,'$.space_uid')='space'",[],|r|r.get::<_,i64>(0)).unwrap(),3);
    }
    #[test]
    fn spaces_are_owner_managed_current_members_only_and_archival_revokes_access() {
        let c = fixture(1);
        space(&c, "channel");
        assert!(sharing::grant(&c, "b", "a-00000", "space", Some("viewer")).is_err());
        assert!(sharing::grant(&c, "a", "a-00000", "space", Some("owner")).is_err());
        sharing::grant(&c, "a", "a-00000", "space", Some("viewer")).unwrap();
        c.execute("UPDATE mx_channels SET archived_at=2 WHERE uid='space'", [])
            .unwrap();
        assert!(store::accessible(&c, "a-00000", "b", false).is_err());
        assert!(sharing::grant(&c, "a", "a-00000", "space", Some("editor")).is_err());
        sharing::grant(&c, "a", "a-00000", "space", None).unwrap();
        c.execute(
            "UPDATE mx_channels SET archived_at=NULL,kind='direct' WHERE uid='space'",
            [],
        )
        .unwrap();
        assert!(sharing::grant(&c, "a", "a-00000", "space", Some("viewer")).is_err());
        c.execute("UPDATE mx_channels SET kind='group' WHERE uid='space'", [])
            .unwrap();
        c.execute("DELETE FROM mx_channel_members WHERE user_uid='a'", [])
            .unwrap();
        assert!(sharing::grant(&c, "a", "a-00000", "space", Some("viewer")).is_err());
    }
}
