use super::{
    records::{calculate_formula_json_values, load_record_values},
    schema::FieldDefinition,
};
use rusqlite::{Connection, params_from_iter, types::Value as SqlValue};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct ColumnSummary {
    pub function: String,
    pub value: Option<f64>,
    pub count: usize,
    pub decimals: u8,
}

pub fn validate(field_type: &str, input: &Value, mut clean: Value) -> Result<Value, String> {
    let method = input
        .get("table_summary")
        .and_then(Value::as_str)
        .unwrap_or("none");
    if input
        .get("table_summary")
        .is_some_and(|v| !v.is_string() && !v.is_null())
    {
        return Err("Column summary must be a calculation name.".into());
    }
    if method == "none" || method.is_empty() {
        return Ok(clean);
    }
    if !matches!(
        field_type,
        "integer" | "decimal" | "formula" | "auto_number"
    ) || !matches!(method, "sum" | "average" | "min" | "max" | "count")
        || (field_type == "auto_number" && method != "count")
    {
        return Err("Choose sum, average, minimum, maximum or count for a numeric field; auto numbers support count only.".into());
    }
    let decimals = match input.get("summary_decimals") {
        Some(value) => value
            .as_u64()
            .filter(|v| *v <= 12)
            .ok_or("Summary decimal places must be between 0 and 12.")?,
        None => {
            if matches!(field_type, "integer" | "auto_number") {
                0
            } else {
                input
                    .get("decimals")
                    .and_then(Value::as_u64)
                    .unwrap_or(2)
                    .min(12)
            }
        }
    };
    clean["table_summary"] = json!(method);
    clean["summary_decimals"] = json!(decimals);
    Ok(clean)
}

pub fn calculate(
    c: &Connection,
    fields: &[FieldDefinition],
    clause: &str,
    parameters: &[SqlValue],
) -> rusqlite::Result<BTreeMap<String, ColumnSummary>> {
    let configured = fields
        .iter()
        .filter(|f| {
            f.active
                && matches!(
                    f.field_type.as_str(),
                    "integer" | "decimal" | "formula" | "auto_number"
                )
                && f.config["table_summary"].as_str().is_some_and(|m| {
                    matches!(m, "sum" | "average" | "min" | "max" | "count")
                        && (f.field_type != "auto_number" || m == "count")
                })
        })
        .collect::<Vec<_>>();
    let mut result = BTreeMap::new();
    for f in &configured {
        let method = f.config["table_summary"].as_str().unwrap();
        result.insert(
            f.key.clone(),
            ColumnSummary {
                function: method.into(),
                value: if matches!(method, "sum" | "count") {
                    Some(0.0)
                } else {
                    None
                },
                count: 0,
                decimals: f.config["summary_decimals"].as_u64().unwrap_or(2).min(12) as u8,
            },
        );
    }
    if configured.is_empty() {
        return Ok(result);
    }
    let stored = configured
        .iter()
        .filter(|f| f.field_type != "formula")
        .map(|f| f.uid.as_str())
        .collect::<Vec<_>>();
    if !stored.is_empty() {
        let sql = format!(
            "SELECT f.uid,COUNT(COALESCE(rv.value_real,rv.value_integer)),SUM(COALESCE(rv.value_real,rv.value_integer)),AVG(COALESCE(rv.value_real,rv.value_integer)),MIN(COALESCE(rv.value_real,rv.value_integer)),MAX(COALESCE(rv.value_real,rv.value_integer)) FROM mx_records e JOIN mx_record_values rv ON rv.record_uid=e.uid JOIN mx_fields f ON f.uid=rv.field_uid {clause} AND f.uid IN (SELECT value FROM json_each(?)) GROUP BY f.uid"
        );
        let mut args = parameters.to_vec();
        args.push(SqlValue::Text(json!(stored).to_string()));
        let mut statement = c.prepare(&sql)?;
        let mut rows = statement.query(params_from_iter(args.iter()))?;
        while let Some(row) = rows.next()? {
            let uid: String = row.get(0)?;
            let f = configured.iter().find(|f| f.uid == uid).unwrap();
            let summary = result.get_mut(&f.key).unwrap();
            summary.count = row.get::<_, i64>(1)? as usize;
            summary.value = match summary.function.as_str() {
                "count" => Some(summary.count as f64),
                "sum" => Some(row.get::<_, Option<f64>>(2)?.unwrap_or(0.0)),
                "average" => row.get(3)?,
                "min" => row.get(4)?,
                _ => row.get(5)?,
            };
        }
    }
    let formulas = configured
        .iter()
        .filter(|f| f.field_type == "formula")
        .collect::<Vec<_>>();
    if !formulas.is_empty() {
        // Formula definitions can change without rewriting old stored values.
        // Recalculate from the current schema in bounded batches, never from
        // the stale stored formula column or from just the current UI page.
        let mut statement = c.prepare(&format!(
            "SELECT e.uid FROM mx_records e {clause} ORDER BY e.uid"
        ))?;
        let mut records = statement.query(params_from_iter(parameters.iter()))?;
        loop {
            let mut ids = Vec::new();
            while ids.len() < 128 {
                let Some(row) = records.next()? else { break };
                ids.push(row.get::<_, String>(0)?);
            }
            if ids.is_empty() {
                break;
            }
            let values = load_record_values(c, &ids)?;
            for uid in ids {
                let mut values = values.get(&uid).cloned().unwrap_or_default();
                calculate_formula_json_values(fields, &mut values).map_err(|e| {
                    rusqlite::Error::InvalidParameterName(format!("MX_FORMULA:{e}"))
                })?;
                for f in &formulas {
                    let Some(number) = values.get(&f.key).and_then(Value::as_f64) else {
                        continue;
                    };
                    let s = result.get_mut(&f.key).unwrap();
                    s.count += 1;
                    s.value = Some(match s.function.as_str() {
                        "count" => s.count as f64,
                        "min" => s.value.map_or(number, |v| v.min(number)),
                        "max" => s.value.map_or(number, |v| v.max(number)),
                        _ => s.value.unwrap_or(0.0) + number,
                    });
                }
            }
        }
        for f in &formulas {
            let s = result.get_mut(&f.key).unwrap();
            if s.function == "average" && s.count > 0 {
                s.value = s.value.map(|v| v / s.count as f64);
            }
        }
    }
    for summary in result.values() {
        if summary.value.is_some_and(|v| !v.is_finite()) {
            return Err(rusqlite::Error::InvalidParameterName(
                "Column summary exceeds the supported numeric range.".into(),
            ));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(uid: &str, kind: &str, method: &str) -> FieldDefinition {
        FieldDefinition {
            uid: uid.into(),
            key: uid.into(),
            label: uid.into(),
            field_type: kind.into(),
            required: false,
            unique_value: false,
            searchable: true,
            sortable: true,
            table_visible: true,
            table_priority: 50,
            position: 0,
            active: true,
            config: json!({"table_summary":method,"summary_decimals":2,"expression":"amount * 2","decimals":2}),
        }
    }
    fn database() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE mx_records(uid TEXT PRIMARY KEY,module_uid TEXT,deleted_at INTEGER);CREATE TABLE mx_fields(uid TEXT PRIMARY KEY,module_key TEXT,field_key TEXT,label TEXT,field_type TEXT,position INTEGER,active INTEGER);CREATE TABLE mx_record_values(record_uid TEXT,field_uid TEXT,value_integer INTEGER,value_real REAL,value_text TEXT,value_boolean INTEGER);").unwrap();
        c
    }
    fn add(c: &Connection, uid: &str, module: &str, value: Option<f64>, deleted: bool) {
        c.execute(
            "INSERT INTO mx_records VALUES(?1,?2,?3)",
            rusqlite::params![uid, module, if deleted { Some(1) } else { None }],
        )
        .unwrap();
        c.execute(
            "INSERT INTO mx_record_values(record_uid,field_uid,value_real) VALUES(?1,'amount',?2)",
            rusqlite::params![uid, value],
        )
        .unwrap();
    }
    #[test]
    fn validation_preserves_type_config_and_rejects_unsupported_calculations() {
        let value = validate(
            "formula",
            &json!({"table_summary":"sum","summary_decimals":4}),
            json!({"expression":"amount * 2","decimals":2}),
        )
        .unwrap();
        assert_eq!(value["expression"], "amount * 2");
        assert_eq!(value["summary_decimals"], 4);
        for (kind, cfg) in [
            ("text", json!({"table_summary":"sum"})),
            ("lookup", json!({"table_summary":"count"})),
            ("auto_number", json!({"table_summary":"sum"})),
            ("decimal", json!({"table_summary":"DROP TABLE"})),
            (
                "decimal",
                json!({"table_summary":"sum","summary_decimals":13}),
            ),
            ("decimal", json!({"table_summary":17})),
        ] {
            assert!(validate(kind, &cfg, json!({})).is_err());
        }
        assert_eq!(
            validate("decimal", &json!({"table_summary":"none"}), json!({})).unwrap(),
            json!({})
        );
    }
    #[test]
    fn numeric_summaries_respect_module_deletions_zero_negative_and_nulls() {
        let c = database();
        c.execute(
            "INSERT INTO mx_fields VALUES('amount','amount','amount','Amount','decimal',0,1)",
            [],
        )
        .unwrap();
        for (uid, value) in [
            ("a", Some(10.0)),
            ("b", Some(-3.0)),
            ("c", Some(0.0)),
            ("d", None),
        ] {
            add(&c, uid, "cash", value, false);
        }
        add(&c, "deleted", "cash", Some(1000.0), true);
        add(&c, "hidden", "other", Some(5000.0), false);
        for (method, expected) in [
            ("sum", 7.0),
            ("average", 7.0 / 3.0),
            ("min", -3.0),
            ("max", 10.0),
            ("count", 3.0),
        ] {
            let result = calculate(
                &c,
                &[field("amount", "decimal", method)],
                "WHERE e.module_uid=? AND e.deleted_at IS NULL",
                &[SqlValue::Text("cash".into())],
            )
            .unwrap();
            assert_eq!(result["amount"].count, 3);
            assert_eq!(result["amount"].value, Some(expected));
        }
    }
    #[test]
    fn empty_summary_has_zero_sum_count_but_no_average_minimum_or_maximum() {
        let c = database();
        for method in ["sum", "count", "average", "min", "max"] {
            let result = calculate(
                &c,
                &[field("amount", "decimal", method)],
                "WHERE e.module_uid=?",
                &[SqlValue::Text("empty".into())],
            )
            .unwrap();
            assert_eq!(
                result["amount"].value,
                if matches!(method, "sum" | "count") {
                    Some(0.0)
                } else {
                    None
                }
            );
        }
    }
    #[test]
    fn formula_totals_use_current_definition_in_batches_not_stale_stored_results() {
        let c = database();
        c.execute_batch("INSERT INTO mx_fields VALUES('amount','amount','amount','Amount','decimal',0,1),('calculated','calculated','calculated','Calculated','formula',1,1)").unwrap();
        for i in 0..300 {
            let uid = format!("r{i}");
            add(&c, &uid, "cash", Some(i as f64), false);
            c.execute("INSERT INTO mx_record_values(record_uid,field_uid,value_real) VALUES(?1,'calculated',99999)",[uid]).unwrap();
        }
        let result = calculate(
            &c,
            &[
                field("amount", "decimal", "sum"),
                field("calculated", "formula", "sum"),
            ],
            "WHERE e.module_uid=? AND e.deleted_at IS NULL",
            &[SqlValue::Text("cash".into())],
        )
        .unwrap();
        assert_eq!(result["amount"].value, Some(44850.0));
        assert_eq!(result["calculated"].value, Some(89700.0));
        assert_eq!(result["calculated"].count, 300);
    }
}
