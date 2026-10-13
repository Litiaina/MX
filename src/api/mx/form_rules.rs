//! Declarative, module-local form behavior. Ordinary record values and versions
//! remain the only storage; rules never create editing sessions or run scripts.
use super::schema::{FieldDefinition, load_module_fields_initialized_db};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub field: String,
    pub operator: String,
    #[serde(default)]
    pub value: Value,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub r#match: String,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub groups: Vec<Group>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_when: Option<Group>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_when: Option<Group>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_when: Option<Group>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_group: Option<String>,
    #[serde(default)]
    pub exclusive_required: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OptionDetail {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_when: Option<Group>,
    #[serde(default)]
    pub fill: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Serialize)]
pub struct State {
    pub visible: bool,
    pub enabled: bool,
    pub required: bool,
}
fn rules(field: &FieldDefinition) -> Rules {
    serde_json::from_value(field.config.get("form_rules").cloned().unwrap_or(json!({})))
        .unwrap_or_default()
}
fn options(field: &FieldDefinition) -> Vec<OptionDetail> {
    serde_json::from_value(
        field
            .config
            .get("option_details")
            .cloned()
            .unwrap_or(json!([])),
    )
    .unwrap_or_default()
}
pub fn writable(kind: &str) -> bool {
    matches!(
        kind,
        "text"
            | "long_text"
            | "integer"
            | "decimal"
            | "boolean"
            | "date"
            | "select"
            | "relationship"
    )
}
pub fn configured(fields: &[FieldDefinition]) -> bool {
    fields
        .iter()
        .any(|f| f.config.get("form_rules").is_some() || f.config.get("option_details").is_some())
}
pub fn empty(value: &Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(|v| v.trim().is_empty())
        || value.as_array().is_some_and(Vec::is_empty)
}
fn equal(a: &Value, b: &Value) -> bool {
    if let (Some(a), Some(b)) = (a.as_str(), b.as_str()) {
        a.eq_ignore_ascii_case(b)
    } else if let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) {
        a == b
    } else {
        a == b
    }
}
pub fn matches(group: &Group, values: &BTreeMap<String, Value>) -> bool {
    let items = group
        .conditions
        .iter()
        .map(|c| {
            let a = values.get(&c.field).unwrap_or(&Value::Null);
            let b = &c.value;
            match c.operator.as_str() {
                "eq" => equal(a, b),
                "neq" => !equal(a, b),
                "in" => b
                    .as_array()
                    .is_some_and(|items| items.iter().any(|v| equal(a, v))),
                "not_in" => b
                    .as_array()
                    .is_some_and(|items| !items.iter().any(|v| equal(a, v))),
                "empty" => empty(a),
                "not_empty" => !empty(a),
                op => match (a.as_f64(), b.as_f64()) {
                    (Some(a), Some(b)) => match op {
                        "gt" => a > b,
                        "gte" => a >= b,
                        "lt" => a < b,
                        "lte" => a <= b,
                        _ => false,
                    },
                    _ => false,
                },
            }
        })
        .chain(group.groups.iter().map(|g| matches(g, values)))
        .collect::<Vec<_>>();
    !items.is_empty()
        && if group.r#match == "any" {
            items.into_iter().any(|v| v)
        } else {
            items.into_iter().all(|v| v)
        }
}
pub fn state(field: &FieldDefinition, values: &BTreeMap<String, Value>) -> State {
    let r = rules(field);
    let visible = r.visible_when.as_ref().is_none_or(|g| matches(g, values));
    let enabled = visible && r.enabled_when.as_ref().is_none_or(|g| matches(g, values));
    State {
        visible,
        enabled,
        required: visible
            && enabled
            && (field.required || r.required_when.as_ref().is_some_and(|g| matches(g, values))),
    }
}
fn group_refs(group: &Group, out: &mut BTreeSet<String>) {
    for c in &group.conditions {
        out.insert(c.field.clone());
    }
    for g in &group.groups {
        group_refs(g, out);
    }
}
pub fn references(config: &Value) -> Vec<(String, String)> {
    let r: Rules = serde_json::from_value(config.get("form_rules").cloned().unwrap_or(json!({})))
        .unwrap_or_default();
    let mut refs = Vec::new();
    for (name, group) in [
        ("visibility", r.visible_when),
        ("editability", r.enabled_when),
        ("requirements", r.required_when),
    ] {
        if let Some(g) = group {
            let mut keys = BTreeSet::new();
            group_refs(&g, &mut keys);
            refs.extend(keys.into_iter().map(|key| (key, format!("{name} rule"))));
        }
    }
    let details: Vec<OptionDetail> =
        serde_json::from_value(config.get("option_details").cloned().unwrap_or(json!([])))
            .unwrap_or_default();
    for d in details {
        if let Some(g) = d.available_when {
            let mut keys = BTreeSet::new();
            group_refs(&g, &mut keys);
            refs.extend(keys.into_iter().map(|key| {
                (
                    key,
                    format!(
                        "availability of option {}",
                        d.label.as_deref().unwrap_or(&d.value)
                    ),
                )
            }));
        }
        refs.extend(d.fill.into_keys().map(|key| {
            (
                key,
                format!(
                    "automatic values of option {}",
                    d.label.as_deref().unwrap_or(&d.value)
                ),
            )
        }));
    }
    refs
}
fn validate_group(group: &Group, depth: usize, nodes: &mut usize) -> Result<(), String> {
    *nodes += group.conditions.len() + 1;
    if depth > 4 || *nodes > 256 {
        return Err("Form conditions support at most four group levels and 256 conditions/groups per field.".into());
    }
    if !matches!(group.r#match.as_str(), "all" | "any")
        || group.conditions.is_empty() && group.groups.is_empty()
    {
        return Err("Choose All/Any and add at least one condition.".into());
    }
    for c in &group.conditions {
        if c.field.is_empty()
            || !matches!(
                c.operator.as_str(),
                "eq" | "neq"
                    | "in"
                    | "not_in"
                    | "empty"
                    | "not_empty"
                    | "gt"
                    | "gte"
                    | "lt"
                    | "lte"
            )
        {
            return Err("Choose a control field and a supported comparison.".into());
        }
        if matches!(c.operator.as_str(), "in" | "not_in") {
            if c.value.as_array().is_none_or(|v| {
                v.is_empty() || v.len() > 100 || v.iter().any(|v| v.is_object() || v.is_array())
            }) {
                return Err("One-of comparisons require 1–100 scalar values.".into());
            }
        } else if c.value.is_array() || c.value.is_object() {
            return Err("Conditions compare scalar values, not objects or lists.".into());
        }
        if c.value.to_string().len() > 4096 {
            return Err("A condition value is too long.".into());
        }
    }
    for g in &group.groups {
        validate_group(g, depth + 1, nodes)?;
    }
    Ok(())
}
pub fn normalize(kind: &str, raw: &Value, mut clean: Value) -> Result<Value, String> {
    if let Some(v) = raw.get("form_rules") {
        let mut r: Rules =
            serde_json::from_value(v.clone()).map_err(|e| format!("Invalid form rules: {e}"))?;
        let mut nodes = 0;
        for g in [&r.visible_when, &r.enabled_when, &r.required_when]
            .into_iter()
            .flatten()
        {
            validate_group(g, 1, &mut nodes)?;
        }
        if !writable(kind) && (r.enabled_when.is_some() || r.required_when.is_some()) {
            return Err("Calculated fields, auto numbers and attachments support visibility rules only. File uploads are a separate operation; rules cannot require or delete their bytes atomically.".into());
        }
        if let Some(group) = &mut r.exclusive_group {
            *group = group.trim().to_owned();
            if kind != "boolean" || group.is_empty() || group.chars().count() > 80 {
                return Err("Exclusive checkbox groups need a Boolean field and a group name of 1–80 characters.".into());
            }
        }
        if r.exclusive_required && r.exclusive_group.is_none() {
            return Err("Choose an exclusive group before requiring a group selection.".into());
        }
        if r.visible_when.is_some()
            || r.enabled_when.is_some()
            || r.required_when.is_some()
            || r.exclusive_group.is_some()
        {
            clean["form_rules"] = json!(r);
        }
    }
    if let Some(p) = raw.get("presentation") {
        if kind != "select" || !matches!(p.as_str(), Some("select" | "radio" | "checkbox")) {
            return Err(
                "Select presentation must be dropdown, radio or exclusive checkboxes.".into(),
            );
        }
        clean["presentation"] = p.clone();
    }
    if let Some(v) = raw.get("option_details") {
        if kind != "select" {
            return Err("Option details belong to Select fields.".into());
        }
        let mut details: Vec<OptionDetail> = serde_json::from_value(v.clone())
            .map_err(|e| format!("Invalid option details: {e}"))?;
        if details.len() > 1000 {
            return Err("Configure at most 1,000 option details.".into());
        }
        let mut seen = BTreeSet::new();
        let mut nodes = 0;
        for d in &mut details {
            d.value = d.value.trim().to_owned();
            if !clean["options"].as_array().is_some_and(|opts| {
                opts.iter()
                    .any(|v| v.as_str().is_some_and(|v| v.eq_ignore_ascii_case(&d.value)))
            }) || !seen.insert(d.value.to_ascii_lowercase())
            {
                return Err("Each option detail must refer to one existing, unique option value. Labels can change; saved option values should stay stable.".into());
            }
            if let Some(label) = &mut d.label {
                *label = label.trim().to_owned();
                if label.is_empty() || label.chars().count() > 200 {
                    return Err("Option labels must have 1–200 characters.".into());
                }
            }
            if d.fill.len() > 64 || d.fill.values().any(|v| v.is_array() || v.is_object()) {
                return Err("Each option can fill up to 64 ordinary scalar fields.".into());
            }
            if let Some(g) = &d.available_when {
                validate_group(g, 1, &mut nodes)?;
            }
        }
        if !details.is_empty() {
            clean["option_details"] = json!(details);
        }
    }
    Ok(clean)
}
fn conditions(group: &Group) -> Vec<&Condition> {
    group
        .conditions
        .iter()
        .chain(group.groups.iter().flat_map(conditions))
        .collect()
}
pub fn validate_fields(fields: &[FieldDefinition]) -> Result<(), String> {
    for f in fields {
        let r = rules(f);
        let details = options(f);
        for g in [&r.visible_when, &r.enabled_when, &r.required_when]
            .into_iter()
            .flatten()
            .chain(details.iter().filter_map(|d| d.available_when.as_ref()))
        {
            for condition in conditions(g) {
                let source = fields
                    .iter()
                    .find(|s| s.key == condition.field)
                    .ok_or_else(|| {
                        format!(
                            "{}: control field '{}' is missing or archived.",
                            f.label, condition.field
                        )
                    })?;
                if !writable(&source.field_type) {
                    return Err(format!(
                        "{}: {} is calculated or a file field. Use a writable control field.",
                        f.label, source.label
                    ));
                }
                if !matches!(condition.operator.as_str(), "empty" | "not_empty") {
                    if source.field_type == "relationship" {
                        return Err(format!(
                            "{}: relationship conditions use Is empty / Is not empty; do not hardcode record IDs.",
                            f.label
                        ));
                    }
                    if matches!(condition.operator.as_str(), "gt" | "gte" | "lt" | "lte")
                        && !matches!(source.field_type.as_str(), "integer" | "decimal")
                    {
                        return Err(format!(
                            "{}: numeric comparisons require a numeric control field.",
                            f.label
                        ));
                    }
                    for value in if matches!(condition.operator.as_str(), "in" | "not_in") {
                        condition
                            .value
                            .as_array()
                            .unwrap()
                            .iter()
                            .collect::<Vec<_>>()
                    } else {
                        vec![&condition.value]
                    } {
                        let canonical = super::records::canonical_rule_value(source, value)?;
                        if !equal(&canonical, value) {
                            return Err(format!(
                                "{}: use the correct value type for {} (numbers and booleans must not be quoted).",
                                f.label, source.label
                            ));
                        }
                    }
                }
            }
        }
        for d in details {
            for (key, value) in d.fill {
                let target = fields.iter().find(|s| s.key == key).ok_or_else(|| {
                    format!(
                        "{}: auto-fill destination '{key}' is missing or archived.",
                        f.label
                    )
                })?;
                if !writable(&target.field_type) || target.field_type == "relationship" {
                    return Err(format!(
                        "{}: {} is not an ordinary writable scalar destination.",
                        f.label, target.label
                    ));
                }
                super::records::validate_autofill_value(target, &value)
                    .map_err(|e| format!("{} option {}: {e}", f.label, d.value))?;
                if !equal(
                    &super::records::canonical_rule_value(target, &value)?,
                    &value,
                ) {
                    return Err(format!(
                        "{} option {}: automatic value for {} must use its native value type.",
                        f.label, d.value, target.label
                    ));
                }
                if fields.iter().any(|other| {
                    other.key != f.key
                        && (other.field_type == "relationship"
                            && other.config["auto_fill"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .any(|m| m["target_field"] == key)
                            || options(other).iter().any(|o| o.fill.contains_key(&key)))
                }) {
                    return Err(format!(
                        "{}: {} already has another automatic-value controller.",
                        f.label, target.label
                    ));
                }
            }
        }
    }
    order(fields).map(|_| ())
}
pub fn validate_config(
    c: &Connection,
    module: &str,
    key: &str,
    label: &str,
    kind: &str,
    config: &Value,
) -> rusqlite::Result<()> {
    let mut fields = load_module_fields_initialized_db(c, module, false)?;
    if let Some(f) = fields.iter_mut().find(|f| f.key == key) {
        f.config = config.clone();
    } else {
        fields.push(FieldDefinition {
            uid: String::new(),
            key: key.into(),
            label: label.into(),
            field_type: kind.into(),
            required: false,
            unique_value: false,
            searchable: false,
            sortable: false,
            table_visible: true,
            table_priority: 50,
            position: 0,
            active: true,
            config: config.clone(),
        });
    }
    validate_fields(&fields)
        .map_err(|s| rusqlite::Error::InvalidParameterName(format!("MX_CONFIG:{s}")))
}
pub fn dependencies(fields: &[FieldDefinition]) -> BTreeMap<String, BTreeSet<String>> {
    let mut deps = fields
        .iter()
        .map(|f| (f.key.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for f in fields {
        let r = rules(f);
        for g in [&r.visible_when, &r.enabled_when, &r.required_when]
            .into_iter()
            .flatten()
        {
            group_refs(g, deps.get_mut(&f.key).unwrap());
        }
        for d in options(f) {
            if let Some(g) = d.available_when {
                group_refs(&g, deps.get_mut(&f.key).unwrap());
            }
            for key in d.fill.into_keys() {
                if let Some(set) = deps.get_mut(&key) {
                    set.insert(f.key.clone());
                }
            }
        }
        if f.field_type == "relationship" {
            for m in f.config["auto_fill"].as_array().into_iter().flatten() {
                if let Some(set) = m["target_field"].as_str().and_then(|k| deps.get_mut(k)) {
                    set.insert(f.key.clone());
                }
            }
        }
    }
    deps
}
fn order(fields: &[FieldDefinition]) -> Result<Vec<String>, String> {
    let deps = dependencies(fields);
    let mut done = BTreeSet::new();
    let mut result = Vec::new();
    while done.len() < fields.len() {
        let ready = fields
            .iter()
            .filter(|f| !done.contains(&f.key) && deps[&f.key].iter().all(|d| done.contains(d)))
            .map(|f| f.key.clone())
            .collect::<Vec<_>>();
        if ready.is_empty() {
            let cycle = cycle_keys(&deps);
            return Err(format!(
                "Circular form dependencies: {}. Remove a condition or automatic-value mapping to break the cycle.",
                fields
                    .iter()
                    .filter(|f| cycle.contains(&f.key))
                    .map(|f| f.label.as_str())
                    .collect::<Vec<_>>()
                    .join(" → ")
            ));
        }
        for key in ready {
            done.insert(key.clone());
            result.push(key);
        }
    }
    Ok(result)
}
fn cycle_keys(deps: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    fn visit(
        key: &str,
        deps: &BTreeMap<String, BTreeSet<String>>,
        done: &mut BTreeSet<String>,
        stack: &mut Vec<String>,
    ) -> Option<BTreeSet<String>> {
        if let Some(index) = stack.iter().position(|k| k == key) {
            return Some(stack[index..].iter().cloned().collect());
        }
        if done.contains(key) {
            return None;
        }
        stack.push(key.into());
        for parent in deps.get(key).into_iter().flatten() {
            if let Some(cycle) = visit(parent, deps, done, stack) {
                return Some(cycle);
            }
        }
        stack.pop();
        done.insert(key.into());
        None
    }
    let mut done = BTreeSet::new();
    let mut stack = Vec::new();
    for key in deps.keys() {
        if let Some(cycle) = visit(key, deps, &mut done, &mut stack) {
            return cycle;
        }
    }
    BTreeSet::new()
}
pub fn ancestors(fields: &[FieldDefinition], key: &str) -> BTreeSet<String> {
    let deps = dependencies(fields);
    let mut out = BTreeSet::new();
    let mut pending = deps.get(key).cloned().unwrap_or_default();
    while let Some(key) = pending.pop_first() {
        if out.insert(key.clone()) {
            pending.extend(deps.get(&key).into_iter().flatten().cloned());
        }
    }
    out
}

/// Expand automatic values and branch resets before normal field conflict checks.
pub fn prepare(
    fields: &[FieldDefinition],
    input: &BTreeMap<String, Value>,
    current: &BTreeMap<String, Value>,
    patch: bool,
) -> Result<BTreeMap<String, Value>, String> {
    if !configured(fields) {
        return Ok(input.clone());
    }
    let mut values = if patch {
        current.clone()
    } else {
        BTreeMap::new()
    };
    let mut changes = input.clone();
    for (key, value) in input {
        let f = fields
            .iter()
            .find(|f| f.key == *key)
            .ok_or_else(|| format!("Unknown field '{key}'. Refresh the module."))?;
        if writable(&f.field_type) {
            values.insert(key.clone(), super::records::canonical_rule_value(f, value)?);
        }
    }
    let groups = fields
        .iter()
        .filter_map(|f| rules(f).exclusive_group)
        .collect::<BTreeSet<_>>();
    for group in &groups {
        let members = fields
            .iter()
            .filter(|f| rules(f).exclusive_group.as_ref() == Some(group))
            .collect::<Vec<_>>();
        let selected = members
            .iter()
            .filter(|f| input.contains_key(&f.key) && values.get(&f.key) == Some(&json!(true)))
            .collect::<Vec<_>>();
        if selected.len() > 1 {
            return Err(format!("Choose only one option in {group}."));
        }
        if let Some(selected) = selected.first() {
            for f in &members {
                if f.key != selected.key {
                    values.insert(f.key.clone(), json!(false));
                    changes.insert(f.key.clone(), json!(false));
                }
            }
        }
    }
    let mut automatic = BTreeMap::new();
    for key in order(fields)? {
        let f = fields.iter().find(|f| f.key == key).unwrap();
        let s = state(f, &values);
        if !s.visible && writable(&f.field_type) {
            let v = values.get(&key).unwrap_or(&Value::Null);
            let meaningful = !empty(v) && !(f.field_type == "boolean" && v == &json!(false));
            let controller_changed = ancestors(fields, &key).iter().any(|k| {
                !equal(
                    values.get(k).unwrap_or(&Value::Null),
                    current.get(k).unwrap_or(&Value::Null),
                )
            });
            if meaningful && input.contains_key(&key) && (!patch || !controller_changed) {
                return Err(format!(
                    "{} is not applicable to the current selections. Reload the record or change its control selection.",
                    f.label
                ));
            }
            if !empty(v) {
                values.insert(key.clone(), Value::Null);
                changes.insert(key.clone(), Value::Null);
            }
            continue;
        }
        if f.field_type == "select" && s.visible {
            let details = options(f);
            let value = values.get(&key).cloned().unwrap_or(Value::Null);
            let selected = details.iter().find(|d| equal(&json!(d.value), &value));
            if selected.is_some_and(|d| {
                d.available_when
                    .as_ref()
                    .is_some_and(|g| !matches(g, &values))
            }) {
                if input.contains_key(&key)
                    && !ancestors(fields, &key).iter().any(|k| {
                        !equal(
                            values.get(k).unwrap_or(&Value::Null),
                            current.get(k).unwrap_or(&Value::Null),
                        )
                    })
                {
                    return Err(format!(
                        "{}: this option is not available for the current selections.",
                        f.label
                    ));
                }
                values.insert(key.clone(), Value::Null);
                changes.insert(key.clone(), Value::Null);
            }
            if changes.contains_key(&key)
                && !equal(
                    values.get(&key).unwrap_or(&Value::Null),
                    current.get(&key).unwrap_or(&Value::Null),
                )
            {
                let chosen = details
                    .iter()
                    .find(|d| equal(&json!(d.value), values.get(&key).unwrap_or(&Value::Null)));
                let targets = details
                    .iter()
                    .flat_map(|d| d.fill.keys().cloned())
                    .collect::<BTreeSet<_>>();
                for target in targets {
                    let value = chosen
                        .and_then(|d| d.fill.get(&target))
                        .cloned()
                        .unwrap_or(Value::Null);
                    automatic.insert(target.clone(), value.clone());
                    if !input.contains_key(&target) {
                        values.insert(target.clone(), value.clone());
                        changes.insert(target, value);
                    }
                }
            }
        }
        if !s.enabled
            && writable(&f.field_type)
            && input.contains_key(&key)
            && !equal(
                values.get(&key).unwrap_or(&Value::Null),
                current.get(&key).unwrap_or(&Value::Null),
            )
            && !automatic
                .get(&key)
                .is_some_and(|v| equal(v, values.get(&key).unwrap_or(&Value::Null)))
        {
            return Err(format!(
                "{} is disabled by its configured rules and cannot be edited.",
                f.label
            ));
        }
    }
    for f in fields {
        let s = state(f, &values);
        let r = rules(f);
        if writable(&f.field_type)
            && s.required
            && empty(values.get(&f.key).unwrap_or(&Value::Null))
            && (!patch
                || input.contains_key(&f.key)
                || r.visible_when.is_some()
                || r.enabled_when.is_some()
                || r.required_when.is_some())
        {
            return Err(format!(
                "{} is required for the current selections.",
                f.label
            ));
        }
    }
    for group in groups {
        let members = fields
            .iter()
            .filter(|f| {
                rules(f).exclusive_group.as_ref() == Some(&group) && state(f, &values).visible
            })
            .collect::<Vec<_>>();
        let count = members
            .iter()
            .filter(|f| values.get(&f.key) == Some(&json!(true)))
            .count();
        if count > 1 || count == 0 && members.iter().any(|f| rules(f).exclusive_required) {
            return Err(format!(
                "Choose {} option in {}.",
                if count > 1 { "only one" } else { "one" },
                group
            ));
        }
    }
    Ok(changes)
}
pub fn effective_fields(
    fields: &[FieldDefinition],
    values: &BTreeMap<String, Value>,
) -> Vec<FieldDefinition> {
    fields
        .iter()
        .map(|f| {
            let mut f = f.clone();
            f.required = state(&f, values).required;
            f
        })
        .collect()
}

pub fn validate_attachment(
    c: &Connection,
    record: &str,
    field_uid: &str,
    base: Option<i64>,
) -> rusqlite::Result<()> {
    let module: String = c.query_row(
        "SELECT module_uid FROM mx_records WHERE uid=?1 AND deleted_at IS NULL",
        [record],
        |r| r.get(0),
    )?;
    let fields = load_module_fields_initialized_db(c, &module, false)?;
    let f = fields
        .iter()
        .find(|f| f.uid == field_uid && f.field_type == "attachments")
        .ok_or_else(|| {
            rusqlite::Error::InvalidParameterName("Attachment field is unavailable.".into())
        })?;
    if !configured(&fields) {
        return Ok(());
    }
    let values = super::records::load_record_values(c, &[record.to_owned()])?
        .remove(record)
        .unwrap_or_default();
    if !state(f, &values).visible {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "{} is not applicable to this record's current selections.",
            f.label
        )));
    }
    if let Some(base) = base {
        for key in ancestors(&fields, &f.key) {
            if let Some(parent) = fields.iter().find(|f| f.key == key) {
                if super::records::current_field_revision(c, record, &parent.uid)? > base {
                    return Err(rusqlite::Error::InvalidParameterName(format!(
                        "{} changed while this upload was pending. Reload before uploading {}.",
                        parent.label, f.label
                    )));
                }
            }
        }
    }
    Ok(())
}

pub fn validate_version(c: &Connection, module: &str, version: &str) -> rusqlite::Result<()> {
    let fields = load_module_fields_initialized_db(c, module, false)?;
    if !configured(&fields) {
        return Ok(());
    }
    let values=c.prepare("SELECT COALESCE(f.module_key,f.field_key),f.field_type,v.value_text,v.value_integer,v.value_real,v.value_boolean FROM mx_record_version_values v JOIN mx_fields f ON f.uid=v.field_uid WHERE v.version_uid=?1 AND f.active=1")?.query_map([version],|r|{
        let key:String=r.get(0)?;let kind:String=r.get(1)?;
        let value=match kind.as_str(){"integer"|"auto_number"=>r.get::<_,Option<i64>>(3)?.map_or(Value::Null,|v|json!(v)),"decimal"|"formula"=>r.get::<_,Option<f64>>(4)?.map_or(Value::Null,|v|json!(v)),"boolean"=>r.get::<_,Option<i64>>(5)?.map_or(Value::Null,|v|json!(v!=0)),"relationship"=>r.get::<_,Option<String>>(2)?.map_or(Value::Null,|s|if s.starts_with('['){serde_json::from_str(&s).unwrap_or(Value::Null)}else{json!(s)}),_=>r.get::<_,Option<String>>(2)?.map_or(Value::Null,Value::String)};Ok((key,value))
    })?.collect::<rusqlite::Result<BTreeMap<_,_>>>()?;
    prepare(&fields, &values, &values, true).map_err(|s| {
        rusqlite::Error::InvalidParameterName(format!(
            "Version does not satisfy current form rules: {s}"
        ))
    })?;
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
    fn values(v: Value) -> BTreeMap<String, Value> {
        serde_json::from_value(v).unwrap()
    }
    fn when(key: &str, value: Value) -> Value {
        json!({"match":"all","conditions":[{"field":key,"operator":"eq","value":value}]})
    }
    fn fields() -> Vec<FieldDefinition> {
        vec![
            field(
                "kind",
                "select",
                json!({"options":["A","B"],"option_details":[{"value":"A","fill":{"code":"01","paid":false}},{"value":"B","fill":{"code":"02","paid":true}}]}),
            ),
            field("code", "text", json!({})),
            field("paid", "boolean", json!({})),
            field(
                "amount",
                "decimal",
                json!({"form_rules":{"visible_when":when("kind",json!("B")),"required_when":when("kind",json!("B"))}}),
            ),
            field("notes", "text", json!({})),
        ]
    }
    #[test]
    fn keeps_unconfigured_modules_unchanged() {
        let input = values(json!({"notes":"abc"}));
        assert_eq!(
            prepare(
                &[field("notes", "text", json!({}))],
                &input,
                &BTreeMap::new(),
                true
            )
            .unwrap(),
            input
        );
    }
    #[test]
    fn nested_conditions_preserve_zero_and_false() {
        let g:Group=serde_json::from_value(json!({"match":"all","conditions":[{"field":"amount","operator":"gte","value":0}],"groups":[{"match":"any","conditions":[{"field":"paid","operator":"eq","value":false},{"field":"kind","operator":"in","value":["A","B"]}]}]})).unwrap();
        assert!(matches(&g, &values(json!({"amount":0,"paid":false}))));
        assert!(!empty(&json!(false)));
        assert!(!empty(&json!(0)));
    }
    #[test]
    fn option_fills_multiple_ordinary_cells() {
        let f = fields();
        let r = prepare(&f, &values(json!({"kind":"A"})), &BTreeMap::new(), false).unwrap();
        assert_eq!(r["code"], json!("01"));
        assert_eq!(r["paid"], json!(false));
        assert!(validate_fields(&f).is_ok());
    }
    #[test]
    fn branch_requires_detail_and_rejects_hidden_input() {
        let f = fields();
        assert!(
            prepare(&f, &values(json!({"kind":"B"})), &BTreeMap::new(), false)
                .unwrap_err()
                .contains("amount is required")
        );
        assert!(
            prepare(
                &f,
                &values(json!({"kind":"A","amount":20})),
                &BTreeMap::new(),
                false
            )
            .unwrap_err()
            .contains("not applicable")
        );
    }
    #[test]
    fn branch_switch_clears_details_in_same_change_map() {
        let f = fields();
        let r = prepare(
            &f,
            &values(json!({"kind":"A"})),
            &values(json!({"kind":"B","amount":20,"code":"02","paid":true})),
            true,
        )
        .unwrap();
        assert_eq!(r["amount"], Value::Null);
        assert_eq!(r["code"], json!("01"));
        assert!(!r.contains_key("notes"));
    }
    #[test]
    fn automatic_copies_honor_explicit_overrides() {
        let f = fields();
        let r = prepare(
            &f,
            &values(json!({"kind":"A","code":"custom"})),
            &BTreeMap::new(),
            false,
        )
        .unwrap();
        assert_eq!(r["code"], json!("custom"));
    }
    #[test]
    fn checkbox_groups_are_exclusive_and_can_require_one() {
        let config = json!({"form_rules":{"exclusive_group":"type","exclusive_required":true}});
        let f = vec![
            field("a", "boolean", config.clone()),
            field("b", "boolean", config),
        ];
        assert!(prepare(&f, &BTreeMap::new(), &BTreeMap::new(), false).is_err());
        assert!(
            prepare(
                &f,
                &values(json!({"a":true,"b":true})),
                &BTreeMap::new(),
                false
            )
            .is_err()
        );
        let r = prepare(
            &f,
            &values(json!({"b":true})),
            &values(json!({"a":true})),
            true,
        )
        .unwrap();
        assert_eq!(r["a"], json!(false));
    }
    #[test]
    fn unavailable_options_are_rejected_or_reset_by_parent_change() {
        let f = vec![
            field("parent", "text", json!({})),
            field(
                "choice",
                "select",
                json!({"options":["x"],"option_details":[{"value":"x","available_when":when("parent",json!("yes"))}]}),
            ),
        ];
        assert!(
            prepare(
                &f,
                &values(json!({"choice":"x"})),
                &values(json!({"parent":"no"})),
                true
            )
            .is_err()
        );
        assert_eq!(
            prepare(
                &f,
                &values(json!({"parent":"no"})),
                &values(json!({"parent":"yes","choice":"x"})),
                true
            )
            .unwrap()["choice"],
            Value::Null
        );
    }
    #[test]
    fn disabled_destination_cannot_be_changed_directly() {
        let mut f = fields();
        f.push(field(
            "locked",
            "text",
            json!({"form_rules":{"enabled_when":when("kind",json!("B"))}}),
        ));
        assert!(
            prepare(
                &f,
                &values(json!({"locked":"bad"})),
                &values(json!({"kind":"A"})),
                true
            )
            .unwrap_err()
            .contains("disabled")
        );
    }
    #[test]
    fn cycles_missing_sources_and_competing_controllers_are_named() {
        let mut f = fields();
        f[0].config["form_rules"] = json!({"visible_when":when("code",json!("01"))});
        assert!(validate_fields(&f).unwrap_err().contains("Circular"));
        f[0].config["form_rules"] = json!({"visible_when":when("missing",json!(true))});
        assert!(validate_fields(&f).unwrap_err().contains("missing"));
        let mut f = fields();
        f.push(field(
            "other",
            "select",
            json!({"options":["x"],"option_details":[{"value":"x","fill":{"code":"other"}}]}),
        ));
        assert!(
            validate_fields(&f)
                .unwrap_err()
                .contains("another automatic")
        );
    }
    #[test]
    fn rejects_scripts_empty_groups_and_file_requirements() {
        assert!(normalize("text", &json!({"form_rules":{"script":"evil"}}), json!({})).is_err());
        assert!(
            normalize(
                "text",
                &json!({"form_rules":{"visible_when":{"match":"all","conditions":[]}}}),
                json!({})
            )
            .is_err()
        );
        assert!(
            normalize(
                "attachments",
                &json!({"form_rules":{"required_when":when("kind",json!("A"))}}),
                json!({})
            )
            .is_err()
        );
    }
    #[test]
    fn automatic_choices_cascade_in_dependency_order() {
        let f = vec![
            field("result", "text", json!({})),
            field(
                "second",
                "select",
                json!({"options":["x"],"option_details":[{"value":"x","fill":{"result":"done"}}]}),
            ),
            field(
                "first",
                "select",
                json!({"options":["a"],"option_details":[{"value":"a","fill":{"second":"x"}}]}),
            ),
        ];
        let r = prepare(&f, &values(json!({"first":"a"})), &BTreeMap::new(), false).unwrap();
        assert_eq!(r["result"], json!("done"));
    }
    #[test]
    fn full_replacement_does_not_inherit_omitted_controllers() {
        let f = vec![
            field("mode", "text", json!({})),
            field(
                "detail",
                "text",
                json!({"form_rules":{"visible_when":when("mode",json!("yes"))}}),
            ),
        ];
        assert!(
            prepare(
                &f,
                &values(json!({"detail":"invalid"})),
                &values(json!({"mode":"yes","detail":"old"})),
                false
            )
            .unwrap_err()
            .contains("not applicable")
        );
    }
    #[test]
    fn disabled_final_state_rejects_an_injected_edit_with_branch_change() {
        let f = vec![
            field("mode", "text", json!({})),
            field(
                "locked",
                "text",
                json!({"form_rules":{"enabled_when":when("mode",json!("yes"))}}),
            ),
        ];
        assert!(
            prepare(
                &f,
                &values(json!({"mode":"no","locked":"injected"})),
                &values(json!({"mode":"yes","locked":"old"})),
                true
            )
            .unwrap_err()
            .contains("disabled")
        );
    }
    #[test]
    fn condition_and_fill_numbers_accept_integer_json_but_not_quoted_scalars() {
        let mut f = fields();
        f[3].config = json!({"form_rules":{"required_when":{"match":"all","conditions":[{"field":"amount","operator":"gt","value":100}]}}});
        assert!(validate_fields(&f).unwrap_err().contains("Circular"));
        f[3].config = json!({});
        f.push(field("detail","text",json!({"form_rules":{"visible_when":{"match":"all","conditions":[{"field":"amount","operator":"gt","value":100}]}}})));
        assert!(validate_fields(&f).is_ok());
        f[0].config["option_details"][0]["fill"]["paid"] = json!("false");
        assert!(
            validate_fields(&f)
                .unwrap_err()
                .contains("native value type")
        );
    }
}
