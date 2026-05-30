use anyhow::{Result, anyhow, bail};
use sqlparser::ast::{
    BinaryOperator, Expr as SqlExpr, Query, SelectItem, SetExpr, Statement, UnaryOperator,
    Value as SqlValue, ValueWithSpan,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

#[derive(Debug, Clone)]
pub struct TaskSearch {
    pub clauses: Vec<String>,
}

impl TaskSearch {
    pub fn new(clauses: Vec<String>) -> Self {
        Self { clauses }
    }

    pub fn parse(&self) -> Result<Option<SearchExpr>> {
        let mut parsed = Vec::with_capacity(self.clauses.len());
        for clause in &self.clauses {
            parsed.push(parse_where_clause(clause)?);
        }

        Ok(match parsed.len() {
            0 => None,
            1 => parsed.into_iter().next(),
            _ => Some(SearchExpr::And(parsed)),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchExpr {
    And(Vec<SearchExpr>),
    Or(Vec<SearchExpr>),
    Not(Box<SearchExpr>),
    Compare {
        field: FieldRef,
        op: CompareOp,
        value: Literal,
    },
    Like {
        field: FieldRef,
        pattern: String,
        negated: bool,
    },
    In {
        field: FieldRef,
        values: Vec<Literal>,
        negated: bool,
    },
    Between {
        field: FieldRef,
        low: Literal,
        high: Literal,
        negated: bool,
    },
    IsNull {
        field: FieldRef,
        negated: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldRef {
    Core(CoreField),
    Plugin { plugin: String, path: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreField {
    Status,
    Title,
    Description,
    Project,
    Tag,
    TargetDate,
    Deadline,
    LaunchDate,
    TargetTimeHint,
    DeadlineTimeHint,
    LaunchTimeHint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    String(String),
    Number(String),
    Boolean(bool),
    Null,
}

#[derive(Debug, Clone)]
pub struct CompiledSqliteQuery {
    pub sql: String,
    pub params: Vec<rusqlite::types::Value>,
}

#[derive(Debug)]
pub struct CompiledPostgresQuery {
    pub sql: String,
    pub params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>>,
}

pub fn parse_where_clause(clause: &str) -> Result<SearchExpr> {
    let dialect = GenericDialect {};
    let wrapped = format!("SELECT * FROM tasks WHERE {clause}");
    let mut statements = Parser::parse_sql(&dialect, &wrapped)?;
    let statement = statements
        .pop()
        .ok_or_else(|| anyhow!("empty WHERE clause"))?;

    let selection = extract_selection(statement)?;
    lower_expr(&selection)
}

pub fn compile_sqlite(expr: Option<&SearchExpr>) -> Result<CompiledSqliteQuery> {
    let mut compiler = SqliteCompiler::default();
    let mut sql = String::from(TASK_SELECT_PREFIX);
    if let Some(expr) = expr {
        sql.push_str(" WHERE ");
        sql.push_str(&compiler.compile_expr(expr)?);
    }
    sql.push_str(TASK_ORDER_BY);

    Ok(CompiledSqliteQuery {
        sql,
        params: compiler.params,
    })
}

pub fn compile_postgres(expr: Option<&SearchExpr>) -> Result<CompiledPostgresQuery> {
    let mut compiler = PostgresCompiler::default();
    let mut sql = String::from(TASK_SELECT_PREFIX);
    if let Some(expr) = expr {
        sql.push_str(" WHERE ");
        sql.push_str(&compiler.compile_expr(expr)?);
    }
    sql.push_str(TASK_ORDER_BY);

    Ok(CompiledPostgresQuery {
        sql,
        params: compiler.params,
    })
}

fn extract_selection(statement: Statement) -> Result<SqlExpr> {
    let Statement::Query(query) = statement else {
        bail!("expected a SELECT statement");
    };

    let Query { body, .. } = *query;
    let SetExpr::Select(select) = *body else {
        bail!("expected a SELECT body");
    };

    if !matches!(select.projection.as_slice(), [SelectItem::Wildcard(_)]) {
        bail!("unexpected SELECT projection");
    }

    select
        .selection
        .ok_or_else(|| anyhow!("WHERE clause is required"))
}

fn lower_expr(expr: &SqlExpr) -> Result<SearchExpr> {
    match expr {
        SqlExpr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And => flatten_expr(expr, BinaryOperator::And),
            BinaryOperator::Or => flatten_expr(expr, BinaryOperator::Or),
            BinaryOperator::Eq
            | BinaryOperator::NotEq
            | BinaryOperator::Lt
            | BinaryOperator::LtEq
            | BinaryOperator::Gt
            | BinaryOperator::GtEq => Ok(SearchExpr::Compare {
                field: lower_field(left)?,
                op: lower_compare_op(op.clone())?,
                value: lower_literal(right)?,
            }),
            other => bail!("unsupported operator in WHERE clause: {other:?}"),
        },
        SqlExpr::Nested(expr) => lower_expr(expr),
        SqlExpr::UnaryOp { op, expr } => match op {
            UnaryOperator::Not => Ok(SearchExpr::Not(Box::new(lower_expr(expr)?))),
            other => bail!("unsupported unary operator in WHERE clause: {other:?}"),
        },
        SqlExpr::Like {
            negated,
            expr,
            pattern,
            ..
        } => Ok(SearchExpr::Like {
            field: lower_field(expr)?,
            pattern: expect_string_literal(pattern)?,
            negated: *negated,
        }),
        SqlExpr::InList {
            expr,
            list,
            negated,
        } => Ok(SearchExpr::In {
            field: lower_field(expr)?,
            values: list.iter().map(lower_literal).collect::<Result<_>>()?,
            negated: *negated,
        }),
        SqlExpr::Between {
            expr,
            negated,
            low,
            high,
        } => Ok(SearchExpr::Between {
            field: lower_field(expr)?,
            low: lower_literal(low)?,
            high: lower_literal(high)?,
            negated: *negated,
        }),
        SqlExpr::IsNull(expr) => Ok(SearchExpr::IsNull {
            field: lower_field(expr)?,
            negated: false,
        }),
        SqlExpr::IsNotNull(expr) => Ok(SearchExpr::IsNull {
            field: lower_field(expr)?,
            negated: true,
        }),
        other => bail!("unsupported WHERE clause expression: {other:?}"),
    }
}

fn flatten_expr(expr: &SqlExpr, target: BinaryOperator) -> Result<SearchExpr> {
    let mut items = Vec::new();
    collect_flattened(expr, &target, &mut items)?;
    Ok(match target {
        BinaryOperator::And => SearchExpr::And(items),
        BinaryOperator::Or => SearchExpr::Or(items),
        _ => unreachable!(),
    })
}

fn collect_flattened(
    expr: &SqlExpr,
    target: &BinaryOperator,
    items: &mut Vec<SearchExpr>,
) -> Result<()> {
    match expr {
        SqlExpr::BinaryOp { left, op, right } if op == target => {
            collect_flattened(left, target, items)?;
            collect_flattened(right, target, items)?;
            Ok(())
        }
        _ => {
            items.push(lower_expr(expr)?);
            Ok(())
        }
    }
}

fn lower_field(expr: &SqlExpr) -> Result<FieldRef> {
    let names = match expr {
        SqlExpr::Identifier(ident) => vec![ident.value.clone()],
        SqlExpr::CompoundIdentifier(idents) => {
            idents.iter().map(|ident| ident.value.clone()).collect()
        }
        SqlExpr::Nested(expr) => return lower_field(expr),
        other => bail!("unsupported field reference: {other:?}"),
    };

    match names.as_slice() {
        [name] => Ok(FieldRef::Core(parse_core_field(name)?)),
        [plugin, path @ ..] if !path.is_empty() => Ok(FieldRef::Plugin {
            plugin: plugin.clone(),
            path: path.to_vec(),
        }),
        _ => bail!("invalid field reference"),
    }
}

fn parse_core_field(name: &str) -> Result<CoreField> {
    match name.to_ascii_lowercase().as_str() {
        "status" => Ok(CoreField::Status),
        "title" => Ok(CoreField::Title),
        "description" => Ok(CoreField::Description),
        "project" => Ok(CoreField::Project),
        "tag" | "tags" => Ok(CoreField::Tag),
        "target_date" => Ok(CoreField::TargetDate),
        "deadline" => Ok(CoreField::Deadline),
        "launch_date" => Ok(CoreField::LaunchDate),
        "target_time_hint" => Ok(CoreField::TargetTimeHint),
        "deadline_time_hint" => Ok(CoreField::DeadlineTimeHint),
        "launch_time_hint" => Ok(CoreField::LaunchTimeHint),
        _ => bail!("unknown search field `{name}`"),
    }
}

fn lower_compare_op(op: BinaryOperator) -> Result<CompareOp> {
    match op {
        BinaryOperator::Eq => Ok(CompareOp::Eq),
        BinaryOperator::NotEq => Ok(CompareOp::NotEq),
        BinaryOperator::Lt => Ok(CompareOp::Lt),
        BinaryOperator::LtEq => Ok(CompareOp::LtEq),
        BinaryOperator::Gt => Ok(CompareOp::Gt),
        BinaryOperator::GtEq => Ok(CompareOp::GtEq),
        other => bail!("unsupported comparison operator: {other:?}"),
    }
}

fn lower_literal(expr: &SqlExpr) -> Result<Literal> {
    match expr {
        SqlExpr::Value(value) => lower_value(value),
        SqlExpr::Identifier(ident) => Ok(Literal::String(ident.value.clone())),
        SqlExpr::CompoundIdentifier(idents) => Ok(Literal::String(
            idents
                .iter()
                .map(|ident| ident.value.as_str())
                .collect::<Vec<_>>()
                .join("."),
        )),
        SqlExpr::Nested(expr) => lower_literal(expr),
        other => bail!("unsupported literal: {other:?}"),
    }
}

fn expect_string_literal(expr: &SqlExpr) -> Result<String> {
    match lower_literal(expr)? {
        Literal::String(value) => Ok(value),
        Literal::Number(value) => Ok(value),
        Literal::Boolean(value) => Ok(value.to_string()),
        Literal::Null => bail!("null is not valid here"),
    }
}

fn lower_value(value: &ValueWithSpan) -> Result<Literal> {
    match &value.value {
        SqlValue::SingleQuotedString(text) => Ok(Literal::String(text.clone())),
        SqlValue::DoubleQuotedString(text) => Ok(Literal::String(text.clone())),
        SqlValue::TripleSingleQuotedString(text) => Ok(Literal::String(text.clone())),
        SqlValue::TripleDoubleQuotedString(text) => Ok(Literal::String(text.clone())),
        SqlValue::Number(text, _) => Ok(Literal::Number(text.clone())),
        SqlValue::Boolean(value) => Ok(Literal::Boolean(*value)),
        SqlValue::Null => Ok(Literal::Null),
        other => bail!("unsupported literal value: {other:?}"),
    }
}

#[derive(Default)]
struct SqliteCompiler {
    params: Vec<rusqlite::types::Value>,
}

impl SqliteCompiler {
    fn compile_expr(&mut self, expr: &SearchExpr) -> Result<String> {
        match expr {
            SearchExpr::And(items) => self.compile_joined("AND", items),
            SearchExpr::Or(items) => self.compile_joined("OR", items),
            SearchExpr::Not(expr) => Ok(format!("NOT ({})", self.compile_expr(expr)?)),
            SearchExpr::Compare { field, op, value } => self.compile_compare(field, *op, value),
            SearchExpr::Like {
                field,
                pattern,
                negated,
            } => {
                let lhs = self.field_expr(field)?;
                let rhs = self.push_string(pattern.clone());
                let op = if *negated { "NOT LIKE" } else { "LIKE" };
                Ok(format!("{lhs} {op} {rhs}"))
            }
            SearchExpr::In {
                field,
                values,
                negated,
            } => self.compile_in(field, values, *negated),
            SearchExpr::Between {
                field,
                low,
                high,
                negated,
            } => {
                let lhs = self.field_expr(field)?;
                let low = self.push_literal(low.clone())?;
                let high = self.push_literal(high.clone())?;
                let not = if *negated { " NOT" } else { "" };
                Ok(format!("{lhs}{not} BETWEEN {low} AND {high}"))
            }
            SearchExpr::IsNull { field, negated } => {
                let lhs = self.field_expr(field)?;
                let op = if *negated { "IS NOT NULL" } else { "IS NULL" };
                Ok(format!("{lhs} {op}"))
            }
        }
    }

    fn compile_joined(&mut self, op: &str, items: &[SearchExpr]) -> Result<String> {
        let parts = items
            .iter()
            .map(|item| self.compile_expr(item).map(|sql| format!("({sql})")))
            .collect::<Result<Vec<_>>>()?;
        Ok(parts.join(&format!(" {op} ")))
    }

    fn compile_compare(
        &mut self,
        field: &FieldRef,
        op: CompareOp,
        value: &Literal,
    ) -> Result<String> {
        if matches!(field, FieldRef::Core(CoreField::Tag)) {
            return self.compile_tag_membership(op, std::slice::from_ref(value));
        }

        let lhs = self.field_expr(field)?;
        let rhs = self.push_literal(value.clone())?;
        Ok(format!("{lhs} {} {rhs}", compare_sql(op)))
    }

    fn compile_in(
        &mut self,
        field: &FieldRef,
        values: &[Literal],
        negated: bool,
    ) -> Result<String> {
        if values.is_empty() {
            bail!("IN requires at least one value");
        }

        if matches!(field, FieldRef::Core(CoreField::Tag)) {
            let clause = self.compile_tag_membership(CompareOp::Eq, values)?;
            return if negated {
                Ok(format!("NOT ({clause})"))
            } else {
                Ok(clause)
            };
        }

        let lhs = self.field_expr(field)?;
        let rhs = values
            .iter()
            .map(|value| self.push_literal(value.clone()))
            .collect::<Result<Vec<_>>>()?;
        let not = if negated { " NOT" } else { "" };
        Ok(format!("{lhs}{not} IN ({})", rhs.join(", ")))
    }

    fn compile_tag_membership(&mut self, op: CompareOp, values: &[Literal]) -> Result<String> {
        if op != CompareOp::Eq {
            bail!("tag only supports = and IN comparisons");
        }

        let rhs = values
            .iter()
            .map(|value| match value {
                Literal::String(value) => Ok(self.push_string(value.clone())),
                _ => bail!("tag comparisons require string literals"),
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(format!(
            "EXISTS (SELECT 1 FROM json_each(tasks.tags_json) AS tag_value WHERE tag_value.value IN ({}))",
            rhs.join(", ")
        ))
    }

    fn field_expr(&self, field: &FieldRef) -> Result<String> {
        Ok(match field {
            FieldRef::Core(field) => match field {
                CoreField::Status => "task_statuses.name".to_string(),
                CoreField::Title => "tasks.title".to_string(),
                CoreField::Description => "tasks.description".to_string(),
                CoreField::Project => "tasks.project".to_string(),
                CoreField::Tag => bail!("tag requires special handling"),
                CoreField::TargetDate => "tasks.target_date".to_string(),
                CoreField::Deadline => "tasks.deadline".to_string(),
                CoreField::LaunchDate => "tasks.launch_date".to_string(),
                CoreField::TargetTimeHint => "tasks.target_time_hint".to_string(),
                CoreField::DeadlineTimeHint => "tasks.deadline_time_hint".to_string(),
                CoreField::LaunchTimeHint => "tasks.launch_time_hint".to_string(),
            },
            FieldRef::Plugin { plugin, path } => {
                let json_path = std::iter::once(plugin.as_str())
                    .chain(path.iter().map(String::as_str))
                    .fold(String::from("$"), |mut acc, segment| {
                        acc.push('.');
                        acc.push_str(segment);
                        acc
                    });
                format!(
                    "json_extract(tasks.extra_json, '{}')",
                    escape_sql_string(&json_path)
                )
            }
        })
    }

    fn push_literal(&mut self, value: Literal) -> Result<String> {
        match value {
            Literal::String(value) => Ok(self.push_string(value)),
            Literal::Number(value) => Ok(self.push_string(value)),
            Literal::Boolean(value) => Ok(self.push_string(value.to_string())),
            Literal::Null => bail!("null is not valid with this operator"),
        }
    }

    fn push_string(&mut self, value: String) -> String {
        self.params.push(rusqlite::types::Value::Text(value));
        format!("?{}", self.params.len())
    }
}

#[derive(Default)]
struct PostgresCompiler {
    params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>>,
}

impl PostgresCompiler {
    fn compile_expr(&mut self, expr: &SearchExpr) -> Result<String> {
        match expr {
            SearchExpr::And(items) => self.compile_joined("AND", items),
            SearchExpr::Or(items) => self.compile_joined("OR", items),
            SearchExpr::Not(expr) => Ok(format!("NOT ({})", self.compile_expr(expr)?)),
            SearchExpr::Compare { field, op, value } => self.compile_compare(field, *op, value),
            SearchExpr::Like {
                field,
                pattern,
                negated,
            } => {
                let lhs = self.field_expr(field)?;
                let rhs = self.push_string(pattern.clone());
                let op = if *negated { "NOT LIKE" } else { "LIKE" };
                Ok(format!("{lhs} {op} {rhs}"))
            }
            SearchExpr::In {
                field,
                values,
                negated,
            } => self.compile_in(field, values, *negated),
            SearchExpr::Between {
                field,
                low,
                high,
                negated,
            } => {
                let lhs = self.field_expr(field)?;
                let low = self.push_literal(low.clone())?;
                let high = self.push_literal(high.clone())?;
                let not = if *negated { " NOT" } else { "" };
                Ok(format!("{lhs}{not} BETWEEN {low} AND {high}"))
            }
            SearchExpr::IsNull { field, negated } => {
                let lhs = self.field_expr(field)?;
                let op = if *negated { "IS NOT NULL" } else { "IS NULL" };
                Ok(format!("{lhs} {op}"))
            }
        }
    }

    fn compile_joined(&mut self, op: &str, items: &[SearchExpr]) -> Result<String> {
        let parts = items
            .iter()
            .map(|item| self.compile_expr(item).map(|sql| format!("({sql})")))
            .collect::<Result<Vec<_>>>()?;
        Ok(parts.join(&format!(" {op} ")))
    }

    fn compile_compare(
        &mut self,
        field: &FieldRef,
        op: CompareOp,
        value: &Literal,
    ) -> Result<String> {
        if matches!(field, FieldRef::Core(CoreField::Tag)) {
            return self.compile_tag_membership(op, std::slice::from_ref(value));
        }

        let lhs = self.field_expr(field)?;
        let rhs = self.push_literal(value.clone())?;
        Ok(format!("{lhs} {} {rhs}", compare_sql(op)))
    }

    fn compile_in(
        &mut self,
        field: &FieldRef,
        values: &[Literal],
        negated: bool,
    ) -> Result<String> {
        if values.is_empty() {
            bail!("IN requires at least one value");
        }

        if matches!(field, FieldRef::Core(CoreField::Tag)) {
            let clause = self.compile_tag_membership(CompareOp::Eq, values)?;
            return if negated {
                Ok(format!("NOT ({clause})"))
            } else {
                Ok(clause)
            };
        }

        let lhs = self.field_expr(field)?;
        let rhs = values
            .iter()
            .map(|value| self.push_literal(value.clone()))
            .collect::<Result<Vec<_>>>()?;
        let not = if negated { " NOT" } else { "" };
        Ok(format!("{lhs}{not} IN ({})", rhs.join(", ")))
    }

    fn compile_tag_membership(&mut self, op: CompareOp, values: &[Literal]) -> Result<String> {
        if op != CompareOp::Eq {
            bail!("tag only supports = and IN comparisons");
        }

        let rhs = values
            .iter()
            .map(|value| match value {
                Literal::String(value) => Ok(self.push_string(value.clone())),
                _ => bail!("tag comparisons require string literals"),
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(format!(
            "EXISTS (SELECT 1 FROM jsonb_array_elements_text(tasks.tags_json) AS tag_value(value) WHERE tag_value.value IN ({}))",
            rhs.join(", ")
        ))
    }

    fn field_expr(&self, field: &FieldRef) -> Result<String> {
        Ok(match field {
            FieldRef::Core(field) => match field {
                CoreField::Status => "task_statuses.name".to_string(),
                CoreField::Title => "tasks.title".to_string(),
                CoreField::Description => "tasks.description".to_string(),
                CoreField::Project => "tasks.project".to_string(),
                CoreField::Tag => bail!("tag requires special handling"),
                CoreField::TargetDate => "tasks.target_date::text".to_string(),
                CoreField::Deadline => "tasks.deadline::text".to_string(),
                CoreField::LaunchDate => "tasks.launch_date::text".to_string(),
                CoreField::TargetTimeHint => "tasks.target_time_hint".to_string(),
                CoreField::DeadlineTimeHint => "tasks.deadline_time_hint".to_string(),
                CoreField::LaunchTimeHint => "tasks.launch_time_hint".to_string(),
            },
            FieldRef::Plugin { plugin, path } => {
                let mut expr = format!("tasks.extra_json -> '{}'", escape_sql_string(plugin));
                for segment in &path[..path.len().saturating_sub(1)] {
                    expr.push_str(&format!(" -> '{}'", escape_sql_string(segment)));
                }
                let last = path
                    .last()
                    .ok_or_else(|| anyhow!("plugin field path is empty"))?;
                expr.push_str(&format!(" ->> '{}'", escape_sql_string(last)));
                expr
            }
        })
    }

    fn push_literal(&mut self, value: Literal) -> Result<String> {
        match value {
            Literal::String(value) => Ok(self.push_string(value)),
            Literal::Number(value) => Ok(self.push_string(value)),
            Literal::Boolean(value) => Ok(self.push_string(value.to_string())),
            Literal::Null => bail!("null is not valid with this operator"),
        }
    }

    fn push_string(&mut self, value: String) -> String {
        self.params.push(Box::new(value));
        format!("${}", self.params.len())
    }
}

fn compare_sql(op: CompareOp) -> &'static str {
    match op {
        CompareOp::Eq => "=",
        CompareOp::NotEq => "!=",
        CompareOp::Lt => "<",
        CompareOp::LtEq => "<=",
        CompareOp::Gt => ">",
        CompareOp::GtEq => ">=",
    }
}

fn escape_sql_string(value: &str) -> String {
    value.replace('\'', "''")
}

const TASK_SELECT_PREFIX: &str = r#"
SELECT
  tasks.id,
  tasks.uuid,
  tasks.title,
  tasks.description,
  task_statuses.name AS status_name,
  tasks.created_at,
  tasks.updated_at,
  tasks.target_date,
  tasks.deadline,
  tasks.launch_date,
  tasks.target_time_hint,
  tasks.deadline_time_hint,
  tasks.launch_time_hint,
  tasks.project,
  tasks.tags_json,
  tasks.extra_json
FROM tasks
JOIN task_statuses ON task_statuses.id = tasks.status_id
"#;

const TASK_ORDER_BY: &str = r#"
 ORDER BY
  CASE task_statuses.name
    WHEN 'active' THEN 0
    WHEN 'unstarted' THEN 1
    WHEN 'waiting' THEN 2
    WHEN 'suspended' THEN 3
    WHEN 'done' THEN 4
    WHEN 'abandoned' THEN 5
    WHEN 'mistaken' THEN 6
    WHEN 'duplicated' THEN 7
    ELSE 8
  END ASC,
  CASE WHEN deadline IS NULL THEN 1 ELSE 0 END,
  deadline ASC,
  CASE WHEN target_date IS NULL THEN 1 ELSE 0 END,
  target_date ASC,
  created_at ASC
"#;

#[cfg(test)]
mod tests {
    use super::{
        CompareOp, CoreField, FieldRef, Literal, SearchExpr, TaskSearch, compile_postgres,
        compile_sqlite, parse_where_clause,
    };

    #[test]
    fn parses_boolean_logic_and_plugin_fields() {
        let expr = parse_where_clause(
            "(status = \"active\" or status = \"waiting\") and chatwork.requester = \"石井\"",
        )
        .expect("parse");

        assert!(matches!(expr, SearchExpr::And(_)));
    }

    #[test]
    fn treats_double_quoted_values_as_strings() {
        let expr = parse_where_clause("status = \"waiting\"").expect("parse");
        assert_eq!(
            expr,
            SearchExpr::Compare {
                field: FieldRef::Core(CoreField::Status),
                op: CompareOp::Eq,
                value: Literal::String("waiting".to_string()),
            }
        );
    }

    #[test]
    fn compiles_tag_membership_for_sqlite() {
        let compiled = compile_sqlite(Some(&SearchExpr::Compare {
            field: FieldRef::Core(CoreField::Tag),
            op: CompareOp::Eq,
            value: Literal::String("ops".to_string()),
        }))
        .expect("compile");

        assert!(compiled.sql.contains("json_each(tasks.tags_json)"));
        assert_eq!(compiled.params.len(), 1);
    }

    #[test]
    fn compiles_plugin_fields_for_postgres() {
        let search = TaskSearch::new(vec!["chatwork.requester = '石井'".to_string()]);
        let expr = search.parse().expect("parse");
        let compiled = compile_postgres(expr.as_ref()).expect("compile");

        assert!(
            compiled
                .sql
                .contains("tasks.extra_json -> 'chatwork' ->> 'requester'")
        );
        assert_eq!(compiled.params.len(), 1);
    }
}
