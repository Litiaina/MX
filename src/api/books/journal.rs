use super::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Line {
    pub account_uid: String,
    pub debit: String,
    pub credit: String,
}
pub(super) fn post(
    tx: &rusqlite::Transaction<'_>,
    m: &str,
    a: &Claims,
    s: &Settings,
    day: &str,
    description: &str,
    src: &Option<Source>,
    lines: &[Line],
    reversal: Option<&str>,
) -> rusqlite::Result<String> {
    if !s.accounting {
        return Err(fail(
            "INVALID",
            "Accounting is disabled. Existing journals remain available.",
        ));
    }
    if !date(day) || (!s.closed_through.is_empty() && day <= s.closed_through.as_str()) {
        return Err(fail(
            "INVALID",
            "Choose a valid posting date after the closed accounting period.",
        ));
    }
    if !multiline(description, 1000) || !(2..=100).contains(&lines.len()) {
        return Err(fail(
            "INVALID",
            "Enter an explanation and 2–100 journal lines.",
        ));
    }
    let mut parsed = Vec::new();
    let mut debit = 0;
    let mut credit = 0;
    let mut accounts = std::collections::BTreeSet::new();
    for line in lines {
        let d = if line.debit.is_empty() {
            0
        } else {
            units(&line.debit, s.money_decimals)?
        };
        let cr = if line.credit.is_empty() {
            0
        } else {
            units(&line.credit, s.money_decimals)?
        };
        if (d == 0) == (cr == 0) {
            return Err(fail(
                "INVALID",
                "Each journal line must have a debit OR a credit, not both. Leave the other amount blank.",
            ));
        }
        let account=tx.query_row("SELECT code,name,debit,credit FROM mx_book_accounts WHERE uid=?1 AND module_uid=?2",params![line.account_uid,m],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).optional()?.ok_or_else(||fail("INVALID","One journal account no longer belongs to this book."))?;
        if !accounts.insert(&line.account_uid) {
            return Err(fail(
                "INVALID",
                "Combine repeated entries for the same account into one journal line.",
            ));
        }
        debit = add(debit, d)?;
        credit = add(credit, cr)?;
        parsed.push((
            line,
            account.0,
            account.1,
            d,
            cr,
            add(account.2, d)?,
            add(account.3, cr)?,
        ));
    }
    if debit != credit {
        return Err(fail(
            "INVALID",
            format!(
                "The journal does not balance: debit {} {}, credit {} {}. Nothing was posted.",
                decimal(debit, s.money_decimals),
                s.currency,
                decimal(credit, s.money_decimals),
                s.currency
            ),
        ));
    }
    let (source_uid, snapshot_uid) = if reversal.is_some() {
        (None, None)
    } else {
        source(tx, m, a, s, src)?
    };
    let uid = uuid::Uuid::new_v4().to_string();
    tx.execute("INSERT INTO mx_book_journals(uid,module_uid,date,description,actor_uid,source_uid,snapshot_uid,reversal_of,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![uid,m,day,description.trim(),a.uid,source_uid,snapshot_uid,reversal,chrono::Utc::now().timestamp_millis()])?;
    for (i, (line, code, name, d, cr, total_d, total_cr)) in parsed.into_iter().enumerate() {
        tx.execute("INSERT INTO mx_book_lines(journal_uid,position,account_uid,account_code,account_name,debit,credit) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![uid,i as i64,line.account_uid,code,name,d,cr])?;
        tx.execute(
            "UPDATE mx_book_accounts SET debit=?1,credit=?2 WHERE uid=?3",
            params![total_d, total_cr, line.account_uid],
        )?;
    }
    Ok(uid)
}
pub(super) fn reverse(
    tx: &rusqlite::Transaction<'_>,
    m: &str,
    a: &Claims,
    s: &Settings,
    uid: &str,
    day: &str,
    reason: &str,
) -> rusqlite::Result<String> {
    if !multiline(reason, 900) {
        return Err(fail("INVALID", "Explain why this journal needs reversing."));
    }
    let original: Option<(String, Option<String>)> = tx
        .query_row(
            "SELECT date,reversal_of FROM mx_book_journals WHERE uid=?1 AND module_uid=?2",
            params![uid, m],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((original_date, reversed)) = original else {
        return Err(fail(
            "INVALID",
            "The original journal was not found in this module.",
        ));
    };
    if reversed.is_some()
        || tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM mx_book_journals WHERE reversal_of=?1)",
            [uid],
            |r| r.get::<_, bool>(0),
        )?
    {
        return Err(fail(
            "CONFLICT",
            "This journal is already a reversal or has already been reversed.",
        ));
    }
    if day < original_date.as_str() {
        return Err(fail(
            "INVALID",
            "A reversal cannot predate its original journal.",
        ));
    }
    let lines=tx.prepare("SELECT account_uid,credit,debit FROM mx_book_lines WHERE journal_uid=?1 ORDER BY position")?.query_map([uid],|r|{let d:i64=r.get(1)?;let c:i64=r.get(2)?;Ok(Line{account_uid:r.get(0)?,debit:if d==0{String::new()}else{decimal(d,s.money_decimals)},credit:if c==0{String::new()}else{decimal(c,s.money_decimals)}})})?.collect::<rusqlite::Result<Vec<_>>>()?;
    post(
        tx,
        m,
        a,
        s,
        day,
        &format!("Reversal: {reason}"),
        &None,
        &lines,
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
    let items = if kind == "accounts" {
        c.prepare("SELECT sequence,uid,code,name,category,debit,credit FROM mx_book_accounts WHERE module_uid=?1 AND sequence>?2 ORDER BY sequence LIMIT 41")?.query_map(params![m,after],|r|{let d:i64=r.get(5)?;let cr:i64=r.get(6)?;Ok(json!({"sequence":r.get::<_,i64>(0)?,"uid":r.get::<_,String>(1)?,"code":r.get::<_,String>(2)?,"name":r.get::<_,String>(3)?,"category":r.get::<_,String>(4)?,"debit":decimal(d,s.money_decimals),"credit":decimal(cr,s.money_decimals),"balance":decimal(d-cr,s.money_decimals)}))})?.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        c.prepare("SELECT j.sequence,j.uid,j.date,j.description,j.reversal_of,(SELECT uid FROM mx_book_journals WHERE reversal_of=j.uid) FROM mx_book_journals j WHERE j.module_uid=?1 AND j.sequence>?2 ORDER BY j.sequence LIMIT 41")?.query_map(params![m,after],|r|Ok(json!({"sequence":r.get::<_,i64>(0)?,"uid":r.get::<_,String>(1)?,"date":r.get::<_,String>(2)?,"description":r.get::<_,String>(3)?,"reversal_of":r.get::<_,Option<String>>(4)?,"reversed_by":r.get::<_,Option<String>>(5)?})))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(page(items))
}
pub(super) fn page(mut items: Vec<Value>) -> Value {
    let more = items.len() > 40;
    items.truncate(40);
    let next = if more {
        items
            .last()
            .and_then(|v| v.get("sequence"))
            .cloned()
            .unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    json!({"items":items,"has_next":more,"next_cursor":next})
}
pub(super) fn detail(
    c: &Connection,
    m: &str,
    uid: &str,
    s: &Settings,
    a: &Claims,
) -> rusqlite::Result<Value> {
    let mut header=c.query_row("SELECT date,description,actor_uid,source_uid,snapshot_uid,reversal_of,created_at FROM mx_book_journals WHERE uid=?1 AND module_uid=?2",params![uid,m],|r|Ok(json!({"uid":uid,"date":r.get::<_,String>(0)?,"description":r.get::<_,String>(1)?,"actor_uid":r.get::<_,String>(2)?,"source_uid":r.get::<_,Option<String>>(3)?,"snapshot_uid":r.get::<_,Option<String>>(4)?,"reversal_of":r.get::<_,Option<String>>(5)?,"created_at":r.get::<_,i64>(6)?}))).optional()?.ok_or_else(||fail("INVALID","This journal was not found in this module."))?;
    let actor_name: Option<String> = c
        .query_row(
            "SELECT name FROM users WHERE uid=?1",
            [header["actor_uid"].as_str().unwrap_or("")],
            |r| r.get(0),
        )
        .optional()?;
    header["actor_name"] = json!(actor_name);
    if let Some(source) = header["source_uid"].as_str() {
        if !privacy::record_allowed(c, source, &a.uid, a.access_level)? {
            header["source_uid"] = Value::Null;
            header["snapshot_uid"] = Value::Null;
        }
    }
    header["lines"]=json!(c.prepare("SELECT account_uid,account_code,account_name,debit,credit FROM mx_book_lines WHERE journal_uid=?1 ORDER BY position")?.query_map([uid],|r|Ok(json!({"account_uid":r.get::<_,String>(0)?,"code":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?,"debit":decimal(r.get(3)?,s.money_decimals),"credit":decimal(r.get(4)?,s.money_decimals)})))?.collect::<rusqlite::Result<Vec<_>>>()?);
    Ok(header)
}
