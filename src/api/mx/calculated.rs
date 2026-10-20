//! Bounded, deterministic calculated values. No scripts or SQL supplied by users.
use super::schema::FieldDefinition;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn text(config: &Value) -> bool {
    config["result_type"] == "text"
}
pub(crate) fn numeric(field: &FieldDefinition) -> bool {
    matches!(
        field.field_type.as_str(),
        "integer" | "decimal" | "auto_number" | "rollup"
    ) || (field.field_type == "formula" && !text(&field.config))
        || (field.field_type == "select" && !option_numbers(field).is_empty())
}
pub(crate) fn references(config: &Value) -> BTreeSet<String> {
    if text(config) {
        config["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    } else {
        super::formula::referenced_fields(config["expression"].as_str().unwrap_or(""))
    }
}
pub(crate) fn normalize_text(config: &Value) -> Result<Value, String> {
    let parts = config["parts"]
        .as_array()
        .ok_or("Choose the fields to combine, in order.")?;
    if parts.is_empty()
        || parts.len() > 64
        || parts
            .iter()
            .any(|p| p.as_str().is_none_or(|s| s.is_empty() || s.len() > 96))
    {
        return Err("Choose between 1 and 64 fields to combine.".into());
    }
    let separator = config
        .get("separator")
        .and_then(Value::as_str)
        .unwrap_or(" ");
    if separator.len() > 64 {
        return Err("The separator cannot exceed 64 bytes.".into());
    }
    Ok(
        json!({"result_type":"text","parts":parts,"separator":separator,"skip_empty":config.get("skip_empty").and_then(Value::as_bool).unwrap_or(true)}),
    )
}
pub(crate) fn option_numbers(field: &FieldDefinition) -> BTreeMap<String, f64> {
    field.config["option_details"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|option| {
            Some((
                option["value"].as_str()?.to_owned(),
                option.get("number")?.as_f64()?,
            ))
        })
        .collect()
}
fn number(field: &FieldDefinition, value: Option<&Value>) -> Result<f64, String> {
    let Some(value) =
        value.filter(|v| !v.is_null() && v.as_str().is_none_or(|s| !s.trim().is_empty()))
    else {
        return Ok(0.0);
    };
    if field.field_type == "select" {
        return option_numbers(field).iter().find(|(key,_)|key.eq_ignore_ascii_case(value.as_str().unwrap_or(""))).map(|(_,number)|*number).ok_or_else(|| format!("{}: the selected option has no numeric value. Configure its numeric value before calculating.", field.label));
    }
    value
        .as_f64()
        .or_else(|| value.as_bool().map(|v| if v { 1.0 } else { 0.0 }))
        .or_else(|| value.as_str().and_then(|s| s.parse::<f64>().ok()))
        .filter(|n| n.is_finite())
        .ok_or_else(|| {
            format!(
                "{} must contain a finite numeric value for this calculation.",
                field.label
            )
        })
}
fn visit<'a>(
    key: &'a str,
    fields: &'a [FieldDefinition],
    visiting: &mut BTreeSet<&'a str>,
    done: &mut BTreeSet<&'a str>,
    order: &mut Vec<&'a FieldDefinition>,
) -> Result<(), String> {
    if done.contains(key) {
        return Ok(());
    }
    if !visiting.insert(key) {
        return Err(format!(
            "Calculation cycle includes '{key}'. Remove the circular dependency."
        ));
    }
    if visiting.len() > 64 {
        return Err("Calculation chains cannot exceed 64 fields.".into());
    }
    if let Some(field) = fields
        .iter()
        .find(|f| f.key == key && f.field_type == "formula")
    {
        for dependency in references(&field.config) {
            let source = fields
                .iter()
                .find(|f| f.key == dependency)
                .ok_or_else(|| format!("{}: unknown source field '{dependency}'.", field.label))?;
            visit(&source.key, fields, visiting, done, order)?;
        }
        order.push(field);
    }
    visiting.remove(key);
    done.insert(key);
    Ok(())
}
pub(crate) fn order(fields: &[FieldDefinition]) -> Result<Vec<&FieldDefinition>, String> {
    let mut order = Vec::new();
    let mut visiting = BTreeSet::new();
    let mut done = BTreeSet::new();
    for field in fields.iter().filter(|f| f.field_type == "formula") {
        visit(&field.key, fields, &mut visiting, &mut done, &mut order)?;
    }
    Ok(order)
}
pub(crate) fn live(field: &FieldDefinition, fields: &[FieldDefinition]) -> bool {
    fn depends(
        field: &FieldDefinition,
        fields: &[FieldDefinition],
        seen: &mut BTreeSet<String>,
    ) -> bool {
        if !seen.insert(field.key.clone()) {
            return false;
        }
        matches!(field.field_type.as_str(), "lookup" | "rollup")
            || (field.field_type == "formula"
                && references(&field.config).iter().any(|key| {
                    fields
                        .iter()
                        .find(|f| f.key == *key)
                        .is_some_and(|f| depends(f, fields, seen))
                }))
    }
    depends(field, fields, &mut BTreeSet::new())
}
pub(crate) fn evaluate(
    fields: &[FieldDefinition],
    values: &mut BTreeMap<String, Value>,
    include_live: bool,
) -> Result<(), String> {
    for field in order(fields)? {
        if !include_live && live(field, fields) {
            values.remove(&field.key);
            continue;
        }
        if include_live
            && references(&field.config).iter().any(|key| {
                fields
                    .iter()
                    .find(|f| f.key == *key)
                    .is_some_and(|source| live(source, fields))
                    && values.get(key).is_none_or(Value::is_null)
            })
        {
            values.insert(field.key.clone(), Value::Null);
            continue;
        }
        let result = if text(&field.config) {
            let skip_empty = field.config["skip_empty"].as_bool().unwrap_or(true);
            let mut parts = Vec::new();
            for key in field.config["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let value = values.get(key).unwrap_or(&Value::Null);
                let part = match value {
                    Value::Null => String::new(),
                    Value::String(s) => s.trim().to_owned(),
                    Value::Number(_) | Value::Bool(_) => value.to_string(),
                    _ => {
                        return Err(format!(
                            "{}: choose scalar fields to combine, not lists or record links.",
                            field.label
                        ));
                    }
                };
                if !skip_empty || !part.is_empty() {
                    parts.push(part);
                }
            }
            let joined = parts.join(field.config["separator"].as_str().unwrap_or(" "));
            if joined.len() > 65_536 {
                return Err(format!("{}: combined text exceeds 64 KiB.", field.label));
            }
            Value::String(joined)
        } else {
            let mut variables = BTreeMap::new();
            for key in references(&field.config) {
                let source = fields
                    .iter()
                    .find(|f| f.key == key)
                    .ok_or_else(|| format!("Unknown field '{key}'."))?;
                variables.insert(key, number(source, values.get(&source.key))?);
            }
            let raw = super::formula::evaluate_expression(
                field.config["expression"].as_str().unwrap_or(""),
                Some(&variables),
            )
            .map_err(|e| format!("{}: {e}", field.label))?;
            let factor = 10_f64.powi(field.config["decimals"].as_u64().unwrap_or(2).min(12) as i32);
            let rounded = (raw * factor).round() / factor;
            if !rounded.is_finite() {
                return Err(format!("{}: rounded result is not finite.", field.label));
            }
            json!(rounded)
        };
        values.insert(field.key.clone(), result);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(key: &str, kind: &str, config: Value) -> FieldDefinition {
        FieldDefinition {
            uid: key.into(),
            key: key.into(),
            label: key.into(),
            field_type: kind.into(),
            required: false,
            unique_value: false,
            searchable: true,
            sortable: true,
            table_visible: true,
            table_priority: 50,
            position: 0,
            active: true,
            config,
        }
    }
    #[test]
    fn text_configuration_is_bounded_and_explicit() {
        assert!(normalize_text(&json!({"parts":[]})).is_err());
        assert_eq!(
            normalize_text(&json!({"parts":["first","middle","last"]})).unwrap()["separator"],
            " "
        );
        assert!(normalize_text(&json!({"parts":["first"],"separator":"x".repeat(65)})).is_err());
    }
    #[test]
    fn names_skip_blanks_and_preserve_unicode_without_extra_spaces() {
        let fields = vec![
            field("first", "text", json!({})),
            field("middle", "text", json!({})),
            field("last", "text", json!({})),
            field(
                "full",
                "formula",
                normalize_text(&json!({"parts":["first","middle","last"]})).unwrap(),
            ),
        ];
        let mut values = BTreeMap::from([
            ("first".into(), json!("  José ")),
            ("middle".into(), Value::Null),
            ("last".into(), json!("Dela Cruz")),
        ]);
        evaluate(&fields, &mut values, false).unwrap();
        assert_eq!(values["full"], "José Dela Cruz");
        values.insert("middle".into(), json!(" María "));
        evaluate(&fields, &mut values, false).unwrap();
        assert_eq!(values["full"], "José María Dela Cruz");
    }
    #[test]
    fn numeric_choices_are_independent_from_their_labels_and_saved_codes() {
        let fields = vec![
            field(
                "kind",
                "select",
                json!({"options":["SL","VL"],"option_details":[{"value":"sl","label":"Sick leave","number":5},{"value":"VL","number":2.5}]}),
            ),
            field("days", "decimal", json!({})),
            field(
                "cost",
                "formula",
                json!({"expression":"kind * days","decimals":2}),
            ),
        ];
        let mut values = BTreeMap::from([("kind".into(), json!("SL")), ("days".into(), json!(2))]);
        evaluate(&fields, &mut values, false).unwrap();
        assert_eq!(values["cost"], 10.0);
        assert_eq!(values["kind"], "SL");
        values.insert("kind".into(), json!("VL"));
        evaluate(&fields, &mut values, false).unwrap();
        assert_eq!(values["cost"], 5.0);
        values.insert("kind".into(), json!("missing"));
        assert!(
            evaluate(&fields, &mut values, false)
                .unwrap_err()
                .contains("no numeric value")
        );
    }
    #[test]
    fn dependencies_are_ordered_and_cycles_rejected_even_without_records() {
        let mut fields = vec![
            field("a", "formula", json!({"expression":"b+1"})),
            field("b", "formula", json!({"expression":"c*2"})),
            field("c", "decimal", json!({})),
        ];
        let mut values = BTreeMap::from([("c".into(), json!(3))]);
        evaluate(&fields, &mut values, false).unwrap();
        assert_eq!(values["a"], 7.0);
        fields[1].config = json!({"expression":"a*2"});
        assert!(order(&fields).unwrap_err().contains("cycle"));
    }
    #[test]
    fn linked_totals_are_deferred_until_presentation() {
        let fields = vec![
            field("allowance", "decimal", json!({})),
            field("used", "rollup", json!({})),
            field(
                "remaining",
                "formula",
                json!({"expression":"allowance-used","decimals":2}),
            ),
        ];
        let mut values = BTreeMap::from([("allowance".into(), json!(15))]);
        evaluate(&fields, &mut values, false).unwrap();
        assert!(!values.contains_key("remaining"));
        values.insert("used".into(), json!(5));
        evaluate(&fields, &mut values, true).unwrap();
        assert_eq!(values["remaining"], 10.0);
    }
}
