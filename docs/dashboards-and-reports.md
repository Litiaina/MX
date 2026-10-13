# Dashboards and reports reference

[Documentation index](README.md) · [Project overview](../README.md)

## Custom Dashboard Builder

MX does not ship with organization-specific dashboard assumptions.

A new deployment starts with:

```text
Dashboard

No dashboard widgets configured.
```

Administrators build the dashboard from Record Structure.

### Widget types

MX supports:

```text
Summary number / KPI
Bar chart
Line chart / trend
Pie chart
Donut chart
Grouped progress
Detailed statistical table
```

### Measure rules

A widget can measure records using rules such as:

```text
Every record
Field equals a value
Field does not equal a value
Field is empty
Field is not empty
File Attachment field has files
```

The Administrator chooses the fields. MX does not hard-code business meaning into these rules.

### Grouping

Categorical widgets can group results by a deployment-defined field.

Example:

```text
Title: Records by Department
Display: Pie chart
Measure: Every record
Category: Department
```

### Completion / match statistics

Example:

```text
Title: Completion by Division
Display: Bar chart

Measure:
Action Taken is not empty

Group by:
Division
```

If a division has 100 records and 50 match the configured rule, the report can represent:

```text
Total          100
Matched         50
Not matched     50
Match rate      50%
```

`Action Taken` and `Division` are merely example field names. MX does not require them.

### Time-series statistics

Line charts can use any `Date` field as the time axis.

Supported aggregation intervals:

```text
Day
Week
Month
Quarter
Year
```

Example:

```text
Title: Monthly Completed Requests
Display: Line chart

Date field:
Date Received

Interval:
Month

Measure:
Status equals Completed
```

### Chart options

Categorical charts can limit the number of displayed categories.

Current category limits include:

```text
5
8
10
15
25
All
```

For count-based views, remaining categories can be combined into `Other`.

Pie and donut charts use count-based values so their slices represent a meaningful whole.

### Dashboard persistence

Dashboard configuration is stored by MX.

Administrators can:

- add widgets;
- edit widgets;
- reorder widgets;
- remove widgets;
- save the final dashboard configuration.

Only configured widgets appear on the dashboard.

---

## Reporting and CSV Export

MX performs reporting on the backend instead of fetching the entire records database into the browser.

The reporting engine supports:

- grouping by dynamic fields;
- match rules;
- empty/non-empty conditions;
- exact values;
- attachment presence;
- date ranges;
- time buckets;
- total counts;
- matched counts;
- not-matched counts;
- match rates.

### Detailed Records export

The detailed CSV export includes active Record Structure fields.

File Attachment fields receive their own attachment filename columns, together with attachment totals.

### Attachment inventory

Attachment inventory can include attachment metadata such as:

- record UID;
- attachment field UID;
- filename;
- size;
- N1 object key.

### User performance

Administrator reporting can derive activity metrics from the audit log, including:

```text
records created
records updated
attachments uploaded
records deleted
attachment downloads
successful actions
failed actions
unique records touched
last activity
```

These are audit-derived operational statistics. They are not treated as business completion metrics unless the deployment explicitly configures a business field/rule for that purpose.

---

