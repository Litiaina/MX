use super::*;

fn actor(uid: &str, level: i64) -> Claims {
    Claims {
        uid: uid.into(),
        email: format!("{uid}@example.invalid"),
        access_level: level,
        auth_version: 0,
        token_kind: "access".into(),
        exp: usize::MAX,
    }
}
fn setup(c: &Connection) {
    c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE users(uid TEXT PRIMARY KEY,name TEXT NOT NULL,email TEXT NOT NULL,access_level INTEGER NOT NULL);
      INSERT INTO users VALUES('admin','Admin','a@x',0),('author','Author','b@x',2),('reviewer','Reviewer','c@x',1),('viewer','Viewer','d@x',3);").unwrap();
    crate::db::schema::initialize(c).unwrap();
    c.execute_batch("INSERT INTO mx_modules(uid,slug,name,singular_name,config_json,created_at,updated_at) VALUES('process','process','Process','Item','{}',0,0),('untouched','untouched','Untouched','Item','{}',0,0);
      INSERT INTO mx_module_permissions(module_uid,access_level,can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments) VALUES('process',0,1,1,1,1,1,1,1),('process',1,1,1,1,1,0,1,1),('process',2,1,1,1,0,0,0,1),('process',3,1,0,0,0,0,0,0);
      INSERT INTO mx_module_account_permissions(module_uid,user_uid,can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments,updated_at) VALUES('process','author',1,1,1,0,0,0,1,0),('process','reviewer',1,1,1,1,0,1,1,0),('process','viewer',1,0,0,0,0,0,0,0);
      INSERT INTO mx_records(uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks,module_uid) VALUES('record',0,1,'','','','','','','process');
      INSERT INTO mx_record_owners VALUES('record','author','creation');").unwrap();
}
fn definition(kind: &str) -> Definition {
    Definition {
        enabled: true,
        name: format!("{kind} process"),
        kind: kind.into(),
        initial_state: "draft".into(),
        title_field: "".into(),
        restart_on_edit: kind != "crm",
        states: vec![
            State {
                key: "draft".into(),
                name: "Draft".into(),
            },
            State {
                key: "review".into(),
                name: "In review".into(),
            },
            State {
                key: "approved".into(),
                name: if kind == "crm" { "Won" } else { "Approved" }.into(),
            },
        ],
        actions: vec![
            Action {
                key: "submit".into(),
                name: "Submit".into(),
                from: "draft".into(),
                to: "review".into(),
                access_levels: vec![0, 1, 2],
                creator_policy: "any".into(),
                required_fields: vec![],
                note_required: false,
            },
            Action {
                key: "approve".into(),
                name: "Approve".into(),
                from: "review".into(),
                to: "approved".into(),
                access_levels: vec![0, 1],
                creator_policy: "not_creator".into(),
                required_fields: vec![],
                note_required: true,
            },
        ],
    }
}
fn enable(c: &Connection, kind: &str) {
    save(
        c,
        "process",
        &actor("admin", 0),
        SaveConfiguration {
            revision: 0,
            definition: definition(kind),
            confirm_existing_records: true,
        },
    )
    .unwrap();
}
fn request(c: &Connection, who: &Claims, action: &str) -> Execute {
    let status = status(c, "process", "record", who).unwrap();
    Execute {
        operation_uid: uuid::Uuid::new_v4().to_string(),
        action: action.into(),
        configuration_revision: status["configuration_revision"].as_i64().unwrap(),
        workflow_revision: status["workflow_revision"].as_i64().unwrap(),
        record_revision: status["record_revision"].as_i64().unwrap(),
        note: "Reviewed supporting evidence".into(),
    }
}
#[test]
fn all_three_processes_have_real_actions_and_durable_history_not_just_a_type_label() {
    for kind in ["crm", "erp", "accounting"] {
        let c = Connection::open_in_memory().unwrap();
        setup(&c);
        enable(&c, kind);
        let author = actor("author", 2);
        let reviewer = actor("reviewer", 1);
        let first = execute(
            &c,
            "process",
            "record",
            &author,
            &request(&c, &author, "submit"),
        )
        .unwrap();
        assert_eq!(first["status"]["state"], "review");
        let result = execute(
            &c,
            "process",
            "record",
            &reviewer,
            &request(&c, &reviewer, "approve"),
        )
        .unwrap();
        assert_eq!(result["status"]["state"], "approved");
        let events = history(&c, "process", "record", &author, i64::MAX).unwrap();
        assert_eq!(events["items"].as_array().unwrap().len(), 2);
        assert_eq!(events["items"][0]["actor_uid"], "reviewer");
        assert_eq!(events["items"][0]["record_revision"], 1);
        assert!(
            !events["items"][0]["snapshot_uid"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        assert_eq!(load(&c, "untouched").unwrap().revision, 0);
    }
}
#[test]
fn stale_actions_and_changed_replays_cannot_apply_twice() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    enable(&c, "erp");
    let who = actor("author", 2);
    let req = request(&c, &who, "submit");
    execute(&c, "process", "record", &who, &req).unwrap();
    assert_eq!(
        execute(&c, "process", "record", &who, &req).unwrap()["replayed"],
        true
    );
    let mut changed = req.clone();
    changed.note = "Changed payload".into();
    assert!(
        execute(&c, "process", "record", &who, &changed)
            .unwrap_err()
            .to_string()
            .contains("MX_OPERATION")
    );
    changed.operation_uid = uuid::Uuid::new_v4().to_string();
    assert!(
        execute(&c, "process", "record", &who, &changed)
            .unwrap_err()
            .to_string()
            .contains("CONFLICT")
    );
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM mx_workflow_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn edits_invalidate_approval_without_writeful_reads_and_crm_can_opt_out() {
    for kind in ["erp", "accounting", "crm"] {
        let c = Connection::open_in_memory().unwrap();
        setup(&c);
        enable(&c, kind);
        let who = actor("author", 2);
        execute(&c, "process", "record", &who, &request(&c, &who, "submit")).unwrap();
        c.execute(
            "UPDATE mx_records SET revision=revision+1 WHERE uid='record'",
            [],
        )
        .unwrap();
        c.execute_batch("PRAGMA query_only=ON;").unwrap();
        let s = status(&c, "process", "record", &who).unwrap();
        assert_eq!(s["state"], if kind == "crm" { "review" } else { "draft" });
        assert_eq!(s["changed_since_action"], true);
        assert_eq!(
            queue(
                &c,
                "process",
                &who,
                QueueQuery {
                    state: Some(if kind == "crm" { "review" } else { "draft" }.into()),
                    ..Default::default()
                }
            )
            .unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            history(&c, "process", "record", &who, i64::MAX).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
#[test]
fn module_grants_creator_isolation_and_separation_of_duties_are_server_enforced() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    enable(&c, "erp");
    let who = actor("author", 2);
    execute(&c, "process", "record", &who, &request(&c, &who, "submit")).unwrap();
    let mut approval = request(&c, &actor("reviewer", 1), "approve");
    c.execute(
        "UPDATE mx_record_owners SET creator_uid='reviewer' WHERE record_uid='record'",
        [],
    )
    .unwrap();
    assert!(
        execute(&c, "process", "record", &actor("reviewer", 1), &approval)
            .unwrap_err()
            .to_string()
            .contains("FORBIDDEN")
    );
    c.execute(
        "UPDATE mx_record_owners SET creator_uid='author' WHERE record_uid='record'",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE mx_module_account_permissions SET can_update=0 WHERE user_uid='reviewer'",
        [],
    )
    .unwrap();
    assert!(
        execute(&c, "process", "record", &actor("reviewer", 1), &approval)
            .unwrap_err()
            .to_string()
            .contains("FORBIDDEN")
    );
    c.execute("UPDATE mx_modules SET config_json='{\"record_visibility\":\"creator\"}' WHERE uid='process'",[]).unwrap();
    assert!(status(&c, "process", "record", &actor("reviewer", 1)).is_err());
    assert!(history(&c, "process", "record", &actor("viewer", 3), i64::MAX).is_err());
    assert_eq!(
        queue(&c, "process", &actor("reviewer", 1), QueueQuery::default()).unwrap()["items"],
        json!([])
    );
    approval.configuration_revision = 99;
    assert!(execute(&c, "process", "record", &actor("reviewer", 1), &approval).is_err());
}
#[test]
fn configuration_is_opt_in_revision_checked_and_preserves_stage_identity() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    let who = actor("admin", 0);
    assert_eq!(
        status(&c, "process", "record", &who).unwrap()["enabled"],
        false
    );
    assert!(
        save(
            &c,
            "process",
            &who,
            SaveConfiguration {
                revision: 0,
                definition: definition("erp"),
                confirm_existing_records: false
            }
        )
        .unwrap_err()
        .to_string()
        .contains("Confirm")
    );
    enable(&c, "erp");
    assert!(
        save(
            &c,
            "process",
            &who,
            SaveConfiguration {
                revision: 0,
                definition: definition("erp"),
                confirm_existing_records: true
            }
        )
        .is_err()
    );
    let mut d = definition("erp");
    d.initial_state = "approved".into();
    assert!(
        save(
            &c,
            "process",
            &who,
            SaveConfiguration {
                revision: 1,
                definition: d,
                confirm_existing_records: true
            }
        )
        .is_err()
    );
    let stale = request(&c, &actor("author", 2), "submit");
    let mut d = definition("erp");
    d.name = "Renamed safely".into();
    d.states[1].name = "Manager review".into();
    save(
        &c,
        "process",
        &who,
        SaveConfiguration {
            revision: 1,
            definition: d,
            confirm_existing_records: true,
        },
    )
    .unwrap();
    assert!(
        execute(&c, "process", "record", &actor("author", 2), &stale)
            .unwrap_err()
            .to_string()
            .contains("CONFLICT")
    );
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM mx_workflow_config_history", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn action_validation_is_bounded_and_unknown_or_restricted_requirements_rejected() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    let mut d = definition("erp");
    d.actions[0].required_fields = vec!["missing".into()];
    assert!(validate(&c, "process", &d).is_err());
    let mut d = definition("erp");
    d.actions[0].access_levels = vec![8];
    assert!(validate(&c, "process", &d).is_err());
    let mut d = definition("erp");
    d.actions[0].creator_policy = "made_up".into();
    assert!(validate(&c, "process", &d).is_err());
    let mut d = definition("erp");
    d.title_field = "missing".into();
    assert!(validate(&c, "process", &d).is_err());
    enable(&c, "erp");
    let who = actor("author", 2);
    let mut r = request(&c, &who, "submit");
    r.operation_uid = "not-a-uuid".into();
    assert!(execute(&c, "process", "record", &who, &r).is_err());
}
#[test]
fn queue_has_stable_bounded_pagination_and_filters_before_limit() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    enable(&c, "erp");
    for i in 0..105 {
        let uid = format!("r-{i:04}");
        c.execute("INSERT INTO mx_records(uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks,module_uid) VALUES(?1,0,?2,'','','','','','','process')",params![uid,i+2]).unwrap();
    }
    let who = actor("reviewer", 1);
    let mut after = None;
    let mut found = BTreeSet::new();
    loop {
        let page = queue(
            &c,
            "process",
            &who,
            QueueQuery {
                after,
                state: Some("draft".into()),
                actionable: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let items = page["items"].as_array().unwrap();
        assert!(items.len() <= 40);
        for item in items {
            assert!(found.insert(item["uid"].as_str().unwrap().to_owned()));
        }
        if page["has_next"] == false {
            break;
        }
        after = Some(page["next_cursor"].as_str().unwrap().into());
    }
    assert_eq!(found.len(), 106);
    assert_eq!(
        queue(
            &c,
            "process",
            &actor("viewer", 3),
            QueueQuery {
                actionable: Some(true),
                ..Default::default()
            }
        )
        .unwrap()["items"],
        json!([])
    );
}
#[test]
fn rollback_leaves_no_action_snapshot_state_or_dedup_entry() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    enable(&c, "erp");
    let who = actor("author", 2);
    let r = request(&c, &who, "submit");
    c.execute_batch("CREATE TRIGGER reject_workflow_event BEFORE INSERT ON mx_workflow_events BEGIN SELECT RAISE(ABORT,'simulated disk/write failure'); END;").unwrap();
    assert!(execute(&c, "process", "record", &who, &r).is_err());
    for table in [
        "mx_workflow_events",
        "mx_workflow_instances",
        "mx_client_operations",
        "mx_record_versions",
    ] {
        assert_eq!(
            c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    c.execute_batch("DROP TRIGGER reject_workflow_event;")
        .unwrap();
    assert!(execute(&c, "process", "record", &who, &r).is_ok());
}

#[test]
fn notifications_are_targeted_atomic_preference_aware_and_not_duplicated_by_replay() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    enable(&c, "erp");
    let who = actor("author", 2);
    let r = request(&c, &who, "submit");
    c.execute_batch("CREATE TRIGGER reject_notification BEFORE INSERT ON mx_notification_outbox BEGIN SELECT RAISE(ABORT,'simulated notification persistence failure'); END;").unwrap();
    assert!(execute(&c, "process", "record", &who, &r).is_err());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM mx_workflow_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    c.execute_batch("DROP TRIGGER reject_notification;")
        .unwrap();
    execute(&c, "process", "record", &who, &r).unwrap();
    execute(&c, "process", "record", &who, &r).unwrap();
    crate::api::notifications::drain_notifications_for_test(&c).unwrap();
    let recipients = c
        .prepare("SELECT recipient_uid FROM mx_notifications ORDER BY recipient_uid")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(recipients, vec!["admin", "reviewer"]);
    let preferences = crate::api::preferences::UserPreferences {
        notifications: crate::api::preferences::NotificationPreferences {
            workflow_changes: Some(false),
            ..Default::default()
        },
        ..Default::default()
    };
    c.execute("INSERT INTO mx_user_preferences(user_uid,preferences_json,updated_at) VALUES('reviewer',?1,0)",[serde_json::to_string(&preferences).unwrap()]).unwrap();
    c.execute(
        "UPDATE mx_records SET revision=revision+1 WHERE uid='record'",
        [],
    )
    .unwrap();
    execute(&c, "process", "record", &who, &request(&c, &who, "submit")).unwrap();
    crate::api::notifications::drain_notifications_for_test(&c).unwrap();
    assert_eq!(
        c.query_row(
            "SELECT COUNT(*) FROM mx_notifications WHERE recipient_uid='reviewer'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1,
        "The muted reviewer does not receive the second decision notification"
    );
    c.execute("UPDATE mx_modules SET config_json='{\"record_visibility\":\"creator\"}' WHERE uid='process'",[]).unwrap();
    let admin = actor("admin", 0);
    execute(
        &c,
        "process",
        "record",
        &admin,
        &request(&c, &admin, "approve"),
    )
    .unwrap();
    crate::api::notifications::drain_notifications_for_test(&c).unwrap();
    assert_eq!(
        c.query_row(
            "SELECT COUNT(*) FROM mx_notifications WHERE recipient_uid='author'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        c.query_row(
            "SELECT COUNT(*) FROM mx_notifications WHERE recipient_uid='viewer'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn required_scalar_and_attachment_fields_cannot_be_bypassed_and_archive_is_explained() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    c.execute_batch("INSERT INTO mx_fields(uid,field_key,module_key,module_uid,label,field_type) VALUES('amount','amount','amount','process','Amount','decimal'),('evidence','evidence','evidence','process','Receipt','attachments');").unwrap();
    let mut d = definition("accounting");
    d.actions[0].required_fields = vec!["amount".into(), "evidence".into()];
    save(
        &c,
        "process",
        &actor("admin", 0),
        SaveConfiguration {
            revision: 0,
            definition: d,
            confirm_existing_records: true,
        },
    )
    .unwrap();
    let who = actor("author", 2);
    let r = request(&c, &who, "submit");
    let error = execute(&c, "process", "record", &who, &r)
        .unwrap_err()
        .to_string();
    assert!(error.contains("Amount") && error.contains("Receipt"));
    // Numeric zero is present; an empty/missing field is not the same as zero.
    c.execute(
        "INSERT INTO mx_record_values(record_uid,field_uid,value_real) VALUES('record','amount',0)",
        [],
    )
    .unwrap();
    assert_eq!(
        missing(
            &c,
            "process",
            "record",
            &load(&c, "process").unwrap().definition.actions[0]
        )
        .unwrap(),
        vec!["Receipt"]
    );
    let check = crate::api::mx::field_archive::check(&c, "evidence").unwrap();
    assert!(!check.can_archive);
    assert!(
        check
            .blockers
            .iter()
            .any(|b| b.section == "workflow" && b.reason.contains("Submit"))
    );
    c.execute("UPDATE mx_fields SET active=0 WHERE uid='evidence'", [])
        .unwrap();
    assert!(
        validate_module_fields(&c, "process")
            .unwrap_err()
            .to_string()
            .contains("MX_CONFIG")
    );
    assert!(execute(&c, "process", "record", &who, &r).is_err());
}

#[test]
fn decision_history_is_cursor_paged_and_configuration_changes_keep_historical_labels() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    let mut d = definition("crm");
    d.actions.push(Action {
        key: "reopen".into(),
        name: "Reopen".into(),
        from: "review".into(),
        to: "draft".into(),
        access_levels: vec![0, 1, 2],
        creator_policy: "any".into(),
        required_fields: vec![],
        note_required: false,
    });
    save(
        &c,
        "process",
        &actor("admin", 0),
        SaveConfiguration {
            revision: 0,
            definition: d.clone(),
            confirm_existing_records: true,
        },
    )
    .unwrap();
    let who = actor("author", 2);
    for i in 0..45 {
        execute(
            &c,
            "process",
            "record",
            &who,
            &request(&c, &who, if i % 2 == 0 { "submit" } else { "reopen" }),
        )
        .unwrap();
    }
    d.states[1].name = "New review label".into();
    d.actions[0].name = "New submit label".into();
    save(
        &c,
        "process",
        &actor("admin", 0),
        SaveConfiguration {
            revision: 1,
            definition: d,
            confirm_existing_records: true,
        },
    )
    .unwrap();
    let first = history(&c, "process", "record", &who, i64::MAX).unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 40);
    assert_eq!(first["has_next"], true);
    assert_eq!(first["items"][0]["action_name"], "Submit");
    assert_eq!(first["items"][0]["to_name"], "In review");
    let second = history(
        &c,
        "process",
        "record",
        &who,
        first["next_cursor"].as_i64().unwrap(),
    )
    .unwrap();
    assert_eq!(second["items"].as_array().unwrap().len(), 5);
    assert_eq!(second["has_next"], false);
}

#[test]
fn recalculated_saved_values_advance_revision_and_require_fresh_review() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    enable(&c, "accounting");
    c.execute_batch("INSERT INTO mx_fields(uid,field_key,module_key,module_uid,label,field_type,config_json) VALUES('amount','amount','amount','process','Amount','decimal','{}'),('total','total','total','process','Total','formula','{\"expression\":\"amount * 2\",\"decimals\":2}');
      INSERT INTO mx_record_values(record_uid,field_uid,value_real) VALUES('record','amount',10),('record','total',20);").unwrap();
    let who = actor("author", 2);
    execute(&c, "process", "record", &who, &request(&c, &who, "submit")).unwrap();
    c.execute("UPDATE mx_fields SET config_json='{\"expression\":\"amount * 3\",\"decimals\":2}' WHERE uid='total'",[]).unwrap();
    crate::api::mx::records::recalculate_module_formulas(&c, "process", "admin").unwrap();
    let s = status(&c, "process", "record", &who).unwrap();
    assert_eq!(s["record_revision"], 2);
    assert_eq!(s["restarted"], true);
    assert_eq!(c.query_row("SELECT value_real FROM mx_record_values WHERE record_uid='record' AND field_uid='total'",[],|r|r.get::<_,f64>(0)).unwrap(),30.0);
    crate::api::mx::records::recalculate_module_formulas(&c, "process", "admin").unwrap();
    assert_eq!(
        status(&c, "process", "record", &who).unwrap()["record_revision"],
        2,
        "Unchanged calculated data must not create spurious revisions"
    );
}

#[test]
fn disabling_preserves_access_to_decisions_without_permitting_new_actions() {
    let c = Connection::open_in_memory().unwrap();
    setup(&c);
    enable(&c, "erp");
    let who = actor("author", 2);
    execute(&c, "process", "record", &who, &request(&c, &who, "submit")).unwrap();
    let action = request(&c, &actor("reviewer", 1), "approve");
    let mut d = definition("erp");
    d.enabled = false;
    save(
        &c,
        "process",
        &actor("admin", 0),
        SaveConfiguration {
            revision: 1,
            definition: d,
            confirm_existing_records: false,
        },
    )
    .unwrap();
    let status = status(&c, "process", "record", &who).unwrap();
    assert_eq!(status["enabled"], false);
    assert_eq!(status["has_history"], true);
    assert_eq!(status["actions"], json!([]));
    assert_eq!(
        history(&c, "process", "record", &who, i64::MAX).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(execute(&c, "process", "record", &actor("reviewer", 1), &action).is_err());
}
