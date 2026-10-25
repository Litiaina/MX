use super::*;
fn actor(uid: &str, access_level: i64) -> Claims {
    Claims {
        uid: uid.into(),
        email: format!("{uid}@example.invalid"),
        access_level,
        auth_version: 0,
        token_kind: "access".into(),
        exp: usize::MAX,
    }
}
fn setup() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE users(uid TEXT PRIMARY KEY,name TEXT NOT NULL,email TEXT NOT NULL,access_level INTEGER NOT NULL);INSERT INTO users VALUES('admin','Admin','a@x',0),('manager','Manager','m@x',1),('editor','Editor','e@x',2),('viewer','Viewer','v@x',3);").unwrap();
    crate::db::schema::initialize(&c).unwrap();
    c.execute_batch("INSERT INTO mx_modules(uid,slug,name,singular_name,config_json,created_at,updated_at) VALUES('book','book','Book','Entry','{}',0,0),('other','other','Other','Entry','{}',0,0);
      INSERT INTO mx_module_permissions(module_uid,access_level,can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments) VALUES('book',1,1,1,1,0,0,1,1),('book',2,1,1,1,0,0,0,1),('book',3,1,0,0,0,0,0,0);
      INSERT INTO mx_module_account_permissions(module_uid,user_uid,can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments,updated_at) VALUES('book','manager',1,1,1,0,0,1,1,0),('book','editor',1,1,1,0,0,0,1,0),('book','viewer',1,0,0,0,0,0,0,0);").unwrap();
    c
}
fn enabled(c: &Connection) -> Configuration {
    let cfg = Configuration {
        revision: 0,
        settings: Settings {
            accounting: true,
            inventory: true,
            currency: "PHP".into(),
            ..Settings::default()
        },
    };
    save(c, "book", &actor("admin", 0), cfg).unwrap();
    load(c, "book").unwrap()
}
fn command(c: &Connection, action: Action) -> Command {
    Command {
        operation_uid: uuid::Uuid::new_v4().to_string(),
        configuration_revision: load(c, "book").unwrap().revision,
        action,
    }
}
fn apply(c: &Connection, action: Action) -> String {
    execute(c, "book", &actor("admin", 0), &command(c, action)).unwrap()["uid"]
        .as_str()
        .unwrap()
        .to_string()
}
fn account(c: &Connection, code: &str) -> String {
    apply(
        c,
        Action::Account {
            code: code.into(),
            name: code.into(),
            category: "asset".into(),
        },
    )
}
fn item(c: &Connection) -> String {
    apply(
        c,
        Action::Item {
            code: uuid::Uuid::new_v4().to_string(),
            name: "Paper".into(),
            unit: "ream".into(),
        },
    )
}
fn journal(a: &str, b: &str, amount: &str) -> Action {
    Action::Journal {
        date: "2026-10-25".into(),
        description: "Opening balance".into(),
        source: None,
        lines: vec![
            journal::Line {
                account_uid: a.into(),
                debit: amount.into(),
                credit: "".into(),
            },
            journal::Line {
                account_uid: b.into(),
                debit: "".into(),
                credit: amount.into(),
            },
        ],
    }
}
fn movement(item: &str, kind: &str, q: &str) -> Action {
    Action::Stock {
        item_uid: item.into(),
        movement: kind.into(),
        quantity: q.into(),
        note: "Count verified".into(),
        source: None,
    }
}
#[test]
fn exact_decimals_never_round_or_accept_float_syntax() {
    assert!(multiline("Invoice checked\nDelivery checked\tDone", 1000));
    assert!(!text("Multiline\nAccount", 120));
    assert!(!multiline("Unsafe\0Note", 1000));
    assert_eq!(units("0.10", 2).unwrap(), 10);
    assert_eq!(units("90071992547409.93", 2).unwrap(), 9007199254740993);
    assert_eq!(decimal(i64::MIN, 6), "-9223372036854.775808");
    for bad in [
        "",
        "0",
        "0.000",
        "-1",
        "+1",
        "1e3",
        "1,000",
        " 1",
        "1.001",
        "1.",
        ".1",
        "NaN",
        "9223372036854775808",
        "92233720368547758.08",
    ] {
        assert!(units(bad, 2).is_err(), "{bad}");
    }
    assert_eq!(units("92233720368547758.07", 2).unwrap(), i64::MAX);
    assert!(add(i64::MAX, 1).is_err());
}
#[test]
fn startup_is_opt_in_and_read_paths_do_not_materialize_books() {
    let c = setup();
    assert_eq!(load(&c, "book").unwrap().revision, 0);
    c.execute_batch("PRAGMA query_only=ON").unwrap();
    let v = info(&c, "book", &actor("admin", 0)).unwrap();
    assert_eq!(v["settings"]["accounting"], false);
    assert!(
        journal::list(&c, "book", &Settings::default(), "journals", 0).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(prevent_delete(&c, "book").is_ok());
}
#[test]
fn journals_balance_replay_once_and_reverse_with_preserved_evidence() {
    let c = setup();
    let cfg = enabled(&c);
    let a = account(&c, "1000");
    let b = account(&c, "3000");
    let req = command(&c, journal(&a, &b, "90071992547409.93"));
    let posted = execute(&c, "book", &actor("admin", 0), &req).unwrap();
    assert_eq!(
        execute(&c, "book", &actor("admin", 0), &req).unwrap()["uid"],
        posted["uid"]
    );
    let mut changed = req.clone();
    changed.action = journal(&a, &b, "1.00");
    assert!(execute(&c, "book", &actor("admin", 0), &changed).is_err());
    let uid = posted["uid"].as_str().unwrap();
    let evidence = journal::detail(&c, "book", uid, &cfg.settings, &actor("admin", 0)).unwrap();
    assert_eq!(evidence["lines"][0]["debit"], "90071992547409.93");
    let r = Action::Reverse {
        journal_uid: uid.into(),
        date: "2026-10-26".into(),
        reason: "Correct opening balances".into(),
    };
    apply(&c, r.clone());
    assert!(execute(&c, "book", &actor("admin", 0), &command(&c, r)).is_err());
    assert_eq!(
        journal::detail(&c, "book", uid, &cfg.settings, &actor("admin", 0)).unwrap(),
        evidence
    );
    let accounts = journal::list(&c, "book", &cfg.settings, "accounts", 0).unwrap();
    assert!(
        accounts["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["balance"] == "0.00")
    );
    assert!(prevent_delete(&c, "book").is_err());
}
#[test]
fn invalid_journals_leave_no_partial_balances_or_retry_receipts() {
    let c = setup();
    enabled(&c);
    let a = account(&c, "cash");
    let b = account(&c, "equity");
    let before = c.total_changes();
    let mut bad = journal(&a, &b, "10.00");
    if let Action::Journal { lines, .. } = &mut bad {
        lines[1].credit = "9.99".into();
    }
    assert!(
        execute(&c, "book", &actor("admin", 0), &command(&c, bad))
            .unwrap_err()
            .to_string()
            .contains("does not balance")
    );
    assert_eq!(c.total_changes(), before);
    assert_eq!(
        c.query_row("SELECT count(*) FROM mx_book_journals", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        c.query_row("SELECT SUM(debit+credit) FROM mx_book_accounts", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
    let foreign = apply(
        &c,
        Action::Account {
            code: "third".into(),
            name: "Third".into(),
            category: "asset".into(),
        },
    );
    c.execute(
        "UPDATE mx_book_accounts SET module_uid='other' WHERE uid=?1",
        [&foreign],
    )
    .unwrap();
    assert!(
        execute(
            &c,
            "book",
            &actor("admin", 0),
            &command(&c, journal(&a, &foreign, "1"))
        )
        .is_err()
    );
}
#[test]
fn overflowing_balances_rollback_the_entire_posting() {
    let c = setup();
    enabled(&c);
    let a = account(&c, "a");
    let b = account(&c, "b");
    apply(&c, journal(&a, &b, "92233720368547758.07"));
    assert!(
        execute(
            &c,
            "book",
            &actor("admin", 0),
            &command(&c, journal(&a, &b, "0.01"))
        )
        .is_err()
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM mx_book_journals", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn stock_holds_release_and_consumption_cannot_oversell_or_double_consume() {
    let c = setup();
    let cfg = enabled(&c);
    let i = item(&c);
    apply(&c, movement(&i, "receive", "10.5"));
    let req = command(&c, movement(&i, "reserve", "8"));
    execute(&c, "book", &actor("admin", 0), &req).unwrap();
    execute(&c, "book", &actor("admin", 0), &req).unwrap();
    assert!(
        execute(
            &c,
            "book",
            &actor("admin", 0),
            &command(&c, movement(&i, "issue", "3"))
        )
        .is_err()
    );
    let holds = stock::list(&c, "book", &cfg.settings, "reservations", 0).unwrap();
    assert_eq!(holds["items"].as_array().unwrap().len(), 1);
    let uid = holds["items"][0]["uid"].as_str().unwrap();
    let consume = command(
        &c,
        Action::Reservation {
            reservation_uid: uid.into(),
            action: "consume".into(),
            note: "Delivered".into(),
        },
    );
    execute(&c, "book", &actor("admin", 0), &consume).unwrap();
    execute(&c, "book", &actor("admin", 0), &consume).unwrap();
    assert!(
        execute(
            &c,
            "book",
            &actor("admin", 0),
            &command(
                &c,
                Action::Reservation {
                    reservation_uid: uid.into(),
                    action: "release".into(),
                    note: "Late duplicate".into()
                }
            )
        )
        .is_err()
    );
    let balance = stock::list(&c, "book", &cfg.settings, "items", 0).unwrap();
    assert_eq!(balance["items"][0]["on_hand"], "2.500");
    assert_eq!(balance["items"][0]["reserved"], "0.000");
}
#[test]
fn settings_freeze_units_require_revision_and_periods_only_move_forward() {
    let c = setup();
    let mut cfg = enabled(&c);
    let a = account(&c, "a");
    let b = account(&c, "b");
    apply(&c, journal(&a, &b, "1"));
    cfg.settings.currency = "USD".into();
    assert!(save(&c, "book", &actor("admin", 0), cfg.clone()).is_err());
    cfg.settings.currency = "PHP".into();
    cfg.settings.closed_through = "2026-10-25".into();
    save(&c, "book", &actor("admin", 0), cfg.clone()).unwrap();
    assert!(save(&c, "book", &actor("admin", 0), cfg).is_err());
    assert!(
        execute(
            &c,
            "book",
            &actor("admin", 0),
            &command(&c, journal(&a, &b, "1"))
        )
        .is_err()
    );
    let mut cfg = load(&c, "book").unwrap();
    cfg.settings.closed_through = "".into();
    assert!(save(&c, "book", &actor("admin", 0), cfg).is_err());
    let _ = item(&c);
    let mut cfg = load(&c, "book").unwrap();
    cfg.settings.stock_decimals = 0;
    assert!(save(&c, "book", &actor("admin", 0), cfg).is_err());
}
#[test]
fn book_access_is_separate_from_row_visibility_and_requires_explicit_grants() {
    let c = setup();
    let mut cfg = enabled(&c);
    for (uid, level) in [("manager", 1), ("editor", 2), ("viewer", 3)] {
        assert!(operator(&c, "book", &actor(uid, level), &cfg.settings).is_err());
    }
    cfg.settings.operator_levels = vec![0, 1, 2];
    save(&c, "book", &actor("admin", 0), cfg).unwrap();
    let cfg = load(&c, "book").unwrap();
    assert!(operator(&c, "book", &actor("manager", 1), &cfg.settings).is_ok());
    assert!(operator(&c, "book", &actor("editor", 2), &cfg.settings).is_err());
    assert!(authorize(&c, "book", &actor("manager", 1), true).is_err());
    c.execute(
        "UPDATE mx_module_account_permissions SET can_read=0 WHERE user_uid='manager'",
        [],
    )
    .unwrap();
    assert!(operator(&c, "book", &actor("manager", 1), &cfg.settings).is_err());
}
#[test]
fn catalogs_and_histories_use_bounded_keyset_pages() {
    let c = setup();
    let cfg = enabled(&c);
    for n in 0..83 {
        account(&c, &format!("A{n}"));
    }
    let mut after = 0;
    let mut count = 0;
    loop {
        let page = journal::list(&c, "book", &cfg.settings, "accounts", after).unwrap();
        let rows = page["items"].as_array().unwrap();
        assert!(rows.len() <= 40);
        count += rows.len();
        if !page["has_next"].as_bool().unwrap() {
            break;
        }
        after = page["next_cursor"].as_i64().unwrap();
    }
    assert_eq!(count, 83);
}
#[test]
fn command_wire_format_rejects_unknown_and_float_amounts() {
    let valid = json!({"operation_uid":uuid::Uuid::new_v4().to_string(),"configuration_revision":1,"kind":"stock","item_uid":"i","movement":"receive","quantity":"1.00","note":"Counted","source":null});
    assert!(serde_json::from_value::<Command>(valid.clone()).is_ok());
    let mut bad = valid.clone();
    bad["quantity"] = json!(1.2);
    assert!(serde_json::from_value::<Command>(bad).is_err());
    let mut bad = valid;
    bad["bypass"] = json!(true);
    assert!(serde_json::from_value::<Command>(bad).is_err());
}
#[test]
fn evidence_cannot_be_edited_or_deleted_even_by_accidental_raw_sql() {
    let c = setup();
    enabled(&c);
    let a = account(&c, "a");
    let b = account(&c, "b");
    apply(&c, journal(&a, &b, "1"));
    let i = item(&c);
    apply(&c, movement(&i, "receive", "1"));
    for table in [
        "mx_book_journals",
        "mx_book_lines",
        "mx_book_stock_events",
        "mx_book_audit",
    ] {
        assert!(c.execute(&format!("DELETE FROM {table}"), []).is_err());
        let column = if table == "mx_book_lines" {
            "position"
        } else if table == "mx_book_audit" {
            "sequence"
        } else {
            "uid"
        };
        assert!(
            c.execute(&format!("UPDATE {table} SET {column}={column}"), [])
                .is_err()
        );
    }
}
#[test]
fn concurrent_reservations_and_restarts_preserve_invariants() {
    let path = std::env::temp_dir().join(format!("mx-books-{}.db", uuid::Uuid::new_v4()));
    let seed = setup();
    enabled(&seed);
    let i = item(&seed);
    apply(&seed, movement(&i, "receive", "5"));
    // VACUUM INTO copies this disposable fixture, never a deployment database.
    seed.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    let c = Connection::open(&path).unwrap();
    c.execute_batch("PRAGMA journal_mode=WAL;PRAGMA foreign_keys=ON")
        .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            let item = i.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let c = Connection::open(path).unwrap();
                c.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
                c.execute_batch("PRAGMA foreign_keys=ON").unwrap();
                let req = command(&c, movement(&item, "reserve", "1"));
                barrier.wait();
                execute(&c, "book", &actor("admin", 0), &req).is_ok()
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|ok| *ok)
            .count(),
        5
    );
    let (hand, reserved): (i64, i64) = c
        .query_row(
            "SELECT on_hand,reserved FROM mx_book_items WHERE uid=?1",
            [&i],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((hand, reserved), (5000, 5000));
    c.execute_batch("BEGIN IMMEDIATE").unwrap();
    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("PRAGMA query_only=ON").unwrap();
    assert_eq!(
        stock::list(
            &reader,
            "book",
            &load(&reader, "book").unwrap().settings,
            "reservations",
            0
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    c.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    drop(c);
    let reopened = Connection::open(&path).unwrap();
    assert_eq!(
        reopened
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        reopened
            .query_row("SELECT count(*) FROM mx_book_stock_events", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        6
    );
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn receipt_or_evidence_failure_rolls_back_posting_and_original_request_can_retry() {
    let c = setup();
    enabled(&c);
    let a = account(&c, "a");
    let b = account(&c, "b");
    let request = command(&c, journal(&a, &b, "12.34"));
    let before: i64 = c
        .query_row("SELECT count(*) FROM mx_client_operations", [], |r| {
            r.get(0)
        })
        .unwrap();
    c.execute_batch("CREATE TRIGGER books_fault BEFORE INSERT ON mx_client_operations BEGIN SELECT RAISE(ABORT,'synthetic receipt failure'); END;").unwrap();
    assert!(execute(&c, "book", &actor("admin", 0), &request).is_err());
    assert_eq!(
        c.query_row("SELECT count(*) FROM mx_book_journals", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM mx_book_lines", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        c.query_row("SELECT SUM(debit+credit) FROM mx_book_accounts", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM mx_client_operations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        before
    );
    c.execute_batch("DROP TRIGGER books_fault;CREATE TRIGGER books_fault BEFORE INSERT ON mx_book_audit WHEN NEW.event='command' BEGIN SELECT RAISE(ABORT,'synthetic evidence failure'); END;").unwrap();
    assert!(execute(&c, "book", &actor("admin", 0), &request).is_err());
    assert_eq!(
        c.query_row("SELECT count(*) FROM mx_book_journals", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    c.execute_batch("DROP TRIGGER books_fault").unwrap();
    assert_eq!(
        execute(&c, "book", &actor("admin", 0), &request).unwrap()["replayed"],
        false
    );
}

#[test]
fn disabling_inventory_keeps_history_and_allows_only_releasing_existing_holds() {
    let c = setup();
    let cfg = enabled(&c);
    let item = item(&c);
    apply(&c, movement(&item, "receive", "4"));
    apply(&c, movement(&item, "reserve", "3"));
    let uid = stock::list(&c, "book", &cfg.settings, "reservations", 0).unwrap()["items"][0]["uid"]
        .as_str()
        .unwrap()
        .to_string();
    let mut cfg = load(&c, "book").unwrap();
    cfg.settings.inventory = false;
    save(&c, "book", &actor("admin", 0), cfg).unwrap();
    assert!(
        execute(
            &c,
            "book",
            &actor("admin", 0),
            &command(
                &c,
                Action::Reservation {
                    reservation_uid: uid.clone(),
                    action: "consume".into(),
                    note: "Not allowed".into()
                }
            )
        )
        .is_err()
    );
    apply(
        &c,
        Action::Reservation {
            reservation_uid: uid,
            action: "release".into(),
            note: "Return held stock".into(),
        },
    );
    assert_eq!(
        stock::list(&c, "book", &load(&c, "book").unwrap().settings, "items", 0).unwrap()["items"]
            [0]["available"],
        "4.000"
    );
    assert_eq!(
        stock::list(
            &c,
            "book",
            &load(&c, "book").unwrap().settings,
            "stock-events",
            0
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn receipt_reconciliation_after_access_change_cannot_post_new_entries_or_leak_another_actors_receipt()
 {
    let c = setup();
    let mut cfg = enabled(&c);
    let item = item(&c);
    cfg.settings.operator_levels = vec![0, 1];
    save(&c, "book", &actor("admin", 0), cfg).unwrap();
    let manager = actor("manager", 1);
    let request = command(&c, movement(&item, "receive", "2"));
    let original = execute(&c, "book", &manager, &request).unwrap();
    c.execute("UPDATE mx_module_account_permissions SET can_read=0,can_update=0,can_report=0 WHERE user_uid='manager'",[]).unwrap();
    assert_eq!(
        execute(&c, "book", &manager, &request).unwrap()["uid"],
        original["uid"]
    );
    let mut fresh = request.clone();
    fresh.operation_uid = uuid::Uuid::new_v4().to_string();
    assert!(execute(&c, "book", &manager, &fresh).is_err());
    assert!(execute(&c, "book", &actor("viewer", 3), &request).is_err());
    assert!(operator(&c, "book", &manager, &load(&c, "book").unwrap().settings).is_err());
    assert_eq!(
        c.query_row("SELECT count(*) FROM mx_book_stock_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
