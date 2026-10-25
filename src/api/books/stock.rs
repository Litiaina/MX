use super::*;
fn enabled(s: &Settings) -> rusqlite::Result<()> {
    if !s.inventory {
        Err(fail(
            "INVALID",
            "Inventory is disabled. Existing movements remain available.",
        ))
    } else {
        Ok(())
    }
}
fn event(
    c: &Connection,
    m: &str,
    a: &Claims,
    item: &str,
    movement: &str,
    q: i64,
    on_hand: i64,
    reserved: i64,
    note: &str,
    source: (Option<String>, Option<String>),
    reservation: Option<&str>,
) -> rusqlite::Result<String> {
    let uid = uuid::Uuid::new_v4().to_string();
    c.execute(
        "UPDATE mx_book_items SET on_hand=?1,reserved=?2 WHERE uid=?3 AND module_uid=?4",
        params![on_hand, reserved, item, m],
    )?;
    c.execute("INSERT INTO mx_book_stock_events(uid,module_uid,item_uid,movement,quantity,on_hand,reserved,note,actor_uid,source_uid,snapshot_uid,reservation_uid,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![uid,m,item,movement,q,on_hand,reserved,note.trim(),a.uid,source.0,source.1,reservation,chrono::Utc::now().timestamp_millis()])?;
    Ok(uid)
}
pub(super) fn move_stock(
    tx: &rusqlite::Transaction<'_>,
    m: &str,
    a: &Claims,
    s: &Settings,
    item: &str,
    movement: &str,
    quantity: &str,
    note: &str,
    src: &Option<Source>,
) -> rusqlite::Result<String> {
    enabled(s)?;
    if !multiline(note, 1000) || !matches!(movement, "receive" | "issue" | "reserve") {
        return Err(fail(
            "INVALID",
            "Choose Receive, Issue or Reserve and enter an explanation.",
        ));
    }
    let q = units(quantity, s.stock_decimals)?;
    let (on_hand, reserved) = tx
        .query_row(
            "SELECT on_hand,reserved FROM mx_book_items WHERE uid=?1 AND module_uid=?2",
            params![item, m],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()?
        .ok_or_else(|| fail("INVALID", "Choose an item belonging to this module."))?;
    if movement != "receive" && q > on_hand - reserved {
        return Err(fail(
            "CONFLICT",
            format!(
                "Only {} is available. Reserved stock cannot be issued or reserved twice.",
                decimal(on_hand - reserved, s.stock_decimals)
            ),
        ));
    }
    let source = source(tx, m, a, s, src)?;
    let (next_hand, next_reserved) = match movement {
        "receive" => (add(on_hand, q)?, reserved),
        "issue" => (on_hand - q, reserved),
        _ => (on_hand, add(reserved, q)?),
    };
    let reservation = if movement == "reserve" {
        let uid = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO mx_book_reservations(uid,module_uid,item_uid,quantity,status,note,actor_uid,created_at) VALUES(?1,?2,?3,?4,'held',?5,?6,?7)",params![uid,m,item,q,note.trim(),a.uid,chrono::Utc::now().timestamp_millis()])?;
        Some(uid)
    } else {
        None
    };
    event(
        tx,
        m,
        a,
        item,
        movement,
        q,
        next_hand,
        next_reserved,
        note,
        source,
        reservation.as_deref(),
    )
}
pub(super) fn finish(
    tx: &rusqlite::Transaction<'_>,
    m: &str,
    a: &Claims,
    s: &Settings,
    uid: &str,
    action: &str,
    note: &str,
) -> rusqlite::Result<String> {
    // Releasing a hold remains possible after disabling inventory; consuming does not.
    if action != "release" {
        enabled(s)?
    }
    if !matches!(action, "release" | "consume") || !multiline(note, 1000) {
        return Err(fail(
            "INVALID",
            "Choose Release or Consume and explain the decision.",
        ));
    }
    let (item,q,status)=tx.query_row("SELECT item_uid,quantity,status FROM mx_book_reservations WHERE uid=?1 AND module_uid=?2",params![uid,m],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?))).optional()?.ok_or_else(||fail("INVALID","This reservation does not belong to the module."))?;
    if status != "held" {
        return Err(fail(
            "CONFLICT",
            "This reservation was already released or consumed. It cannot be used twice.",
        ));
    }
    let (hand, reserved) = tx.query_row(
        "SELECT on_hand,reserved FROM mx_book_items WHERE uid=?1",
        [&item],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
    )?;
    if q > reserved || q > hand {
        return Err(fail(
            "CONFLICT",
            "Reservation balances need administrator attention. Nothing was changed.",
        ));
    }
    tx.execute(
        "UPDATE mx_book_reservations SET status=?1 WHERE uid=?2",
        params![
            if action == "release" {
                "released"
            } else {
                "consumed"
            },
            uid
        ],
    )?;
    event(
        tx,
        m,
        a,
        &item,
        action,
        q,
        if action == "consume" { hand - q } else { hand },
        reserved - q,
        note,
        (None, None),
        Some(uid),
    )
}
pub(super) fn list(
    c: &Connection,
    m: &str,
    s: &Settings,
    kind: &str,
    after: i64,
) -> rusqlite::Result<Value> {
    let items=match kind {
        "items"=>c.prepare("SELECT sequence,uid,code,name,unit,on_hand,reserved FROM mx_book_items WHERE module_uid=?1 AND sequence>?2 ORDER BY sequence LIMIT 41")?.query_map(params![m,after],|r|{let hand:i64=r.get(5)?;let reserved:i64=r.get(6)?;Ok(json!({"sequence":r.get::<_,i64>(0)?,"uid":r.get::<_,String>(1)?,"code":r.get::<_,String>(2)?,"name":r.get::<_,String>(3)?,"unit":r.get::<_,String>(4)?,"on_hand":decimal(hand,s.stock_decimals),"reserved":decimal(reserved,s.stock_decimals),"available":decimal(hand-reserved,s.stock_decimals)}))})?.collect::<rusqlite::Result<Vec<_>>>()?,
        "reservations"=>c.prepare("SELECT r.sequence,r.uid,r.item_uid,i.code,i.name,r.quantity,r.status,r.note,r.actor_uid FROM mx_book_reservations r JOIN mx_book_items i ON i.uid=r.item_uid WHERE r.module_uid=?1 AND r.sequence>?2 ORDER BY r.sequence LIMIT 41")?.query_map(params![m,after],|r|Ok(json!({"sequence":r.get::<_,i64>(0)?,"uid":r.get::<_,String>(1)?,"item_uid":r.get::<_,String>(2)?,"code":r.get::<_,String>(3)?,"name":r.get::<_,String>(4)?,"quantity":decimal(r.get(5)?,s.stock_decimals),"status":r.get::<_,String>(6)?,"note":r.get::<_,String>(7)?,"actor_uid":r.get::<_,String>(8)?})))?.collect::<rusqlite::Result<Vec<_>>>()?,
        _=>c.prepare("SELECT e.sequence,e.uid,i.code,i.name,e.movement,e.quantity,e.on_hand,e.reserved,e.note,e.actor_uid,e.created_at FROM mx_book_stock_events e JOIN mx_book_items i ON i.uid=e.item_uid WHERE e.module_uid=?1 AND e.sequence>?2 ORDER BY e.sequence LIMIT 41")?.query_map(params![m,after],|r|Ok(json!({"sequence":r.get::<_,i64>(0)?,"uid":r.get::<_,String>(1)?,"code":r.get::<_,String>(2)?,"name":r.get::<_,String>(3)?,"movement":r.get::<_,String>(4)?,"quantity":decimal(r.get(5)?,s.stock_decimals),"on_hand":decimal(r.get(6)?,s.stock_decimals),"reserved":decimal(r.get(7)?,s.stock_decimals),"note":r.get::<_,String>(8)?,"actor_uid":r.get::<_,String>(9)?,"created_at":r.get::<_,i64>(10)?})))?.collect::<rusqlite::Result<Vec<_>>>()?,
    };
    Ok(journal::page(items))
}
