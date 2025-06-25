use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc};

use crate::query_parsing::Condition;
use crate::query_parsing::Expr;
use crate::query_parsing::Operator;
use crate::query_parsing::Value;
use crate::FieldAccess;
use crate::LogEntry;
use crate::LogLevel;
use crate::Prop;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

static REGEX_CACHE: LazyLock<Mutex<HashMap<String, Regex>>> =
	LazyLock::new(|| Mutex::new(HashMap::new()));

fn cached_regex(pattern: &str) -> Result<Regex, regex::Error> {
	let mut cache = REGEX_CACHE.lock().unwrap();
	if let Some(re) = cache.get(pattern) {
		return Ok(re.clone());
	}
	let re = Regex::new(pattern)?;
	cache.insert(pattern.to_string(), re.clone());
	Ok(re)
}

#[derive(Debug)]
enum FieldType {
	Timestamp,
	Level,
	Msg,
	Prop(String, String),
}

fn find_field(v: &str, logline: &LogEntry) -> Option<FieldType> {
	if v == "timestamp" {
		return Some(FieldType::Timestamp);
	}

	if v == "level" {
		return Some(FieldType::Level);
	}

	if v == "msg" {
		return Some(FieldType::Msg);
	}

	for prop in &logline.props {
		if prop.key == v {
			return Some(FieldType::Prop(prop.key.clone(), prop.value.clone()));
		}
	}

	None
}

fn magic_cmp<V, R>(left: V, right: R, op: &Operator) -> bool
where
	V: PartialEq<R> + PartialOrd<R>,
	R: PartialEq<V> + PartialOrd<V>,
{
	match op {
		Operator::Equal => left == right,
		Operator::NotEqual => left != right,
		Operator::GreaterThan => left > right,
		Operator::GreaterThanOrEqual => left >= right,
		Operator::LessThan => left < right,
		Operator::LessThanOrEqual => left <= right,
		_ => todo!("operator {:?} not supported yet", op),
	}
}

fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
	let mut it = v.split('.');
	let major = it.next()?.parse().ok()?;
	let minor_part = it.next().unwrap_or("0");
	let minor = minor_part
		.split(|c| c == '-' || c == '+')
		.next()
		.unwrap_or(minor_part)
		.parse()
		.ok()?;
	let patch_part = it.next().unwrap_or("0");
	let patch = patch_part
		.split(|c| c == '-' || c == '+')
		.next()
		.unwrap_or(patch_part)
		.parse()
		.ok()?;
	Some((major, minor, patch))
}

fn semver_cmp(left: &str, right: &str, op: &Operator) -> Option<bool> {
	let left_v = parse_semver(left)?;
	let right_v = parse_semver(right)?;
	Some(match op {
		Operator::Equal => left_v == right_v,
		Operator::NotEqual => left_v != right_v,
		Operator::GreaterThan => left_v > right_v,
		Operator::GreaterThanOrEqual => left_v >= right_v,
		Operator::LessThan => left_v < right_v,
		Operator::LessThanOrEqual => left_v <= right_v,
		_ => return None,
	})
}

fn cmp_semver_or_string(left: &str, right: &str, op: &Operator) -> bool {
	semver_cmp(left, right, op).unwrap_or_else(|| magic_cmp(left, right, op))
}

fn any(
	field: &FieldType,
	values: &[Value],
	op: &Operator,
	logline: &LogEntry,
	tz: &FixedOffset,
) -> Result<bool, String> {
	for value in values {
		if does_field_match(field, value, op, logline, tz)? {
			return Ok(true);
		}
	}
	Ok(false)
}

fn does_field_match(
	field: &FieldType,
	value: &Value,
	operator: &Operator,
	logline: &LogEntry,
	tz: &FixedOffset,
) -> Result<bool, String> {
	match (field, value, operator) {
		(FieldType::Msg, Value::String(val), Operator::Like) => {
			Ok(logline.msg.to_lowercase().contains(&val.to_lowercase()))
		}
		(FieldType::Msg, Value::String(val), Operator::NotLike) => {
			Ok(!logline.msg.to_lowercase().contains(&val.to_lowercase()))
		}
		(FieldType::Msg, Value::Regex(regex), Operator::Matches) => match cached_regex(regex) {
			Ok(re) => Ok(re.is_match(&logline.msg)),
			Err(e) => Err(e.to_string()),
		},
		(FieldType::Msg, Value::Regex(regex), Operator::NotMatches) => match cached_regex(regex) {
			Ok(re) => Ok(!re.is_match(&logline.msg)),
			Err(e) => Err(e.to_string()),
		},
		(FieldType::Timestamp, Value::Date(val), op) => {
			Ok(magic_cmp(logline.timestamp.with_timezone(tz), *val, op))
		}
		(FieldType::Timestamp, _, _) => Err(format!("Invalid value for timestamp {:?}", value)),
		(FieldType::Level, Value::String(val), op) => {
			Ok(magic_cmp(&logline.level, &LogLevel::from_string(&val), op))
		}
		(FieldType::Level, Value::Date(d), _) => Err(format!("Invalid value for level {:?}", d)),
		(FieldType::Level, Value::Number(l), op) => {
			Ok(magic_cmp(&logline.level, &LogLevel::from_i64(*l), op))
		}
		(FieldType::Msg, Value::String(val), op) => Ok(cmp_semver_or_string(&logline.msg, val, op)),
		(FieldType::Msg, Value::Number(n), op) => Ok(magic_cmp(&logline.msg, &n.to_string(), op)),
		(FieldType::Msg, Value::Date(d), _) => Err(format!("Invalid value for msg {:?}", d)),
		(FieldType::Prop(_, val1), Value::String(val2), Operator::Like) => {
			Ok(val1.contains(&val2.to_string()))
		}
		(FieldType::Prop(_, val1), Value::String(val2), Operator::NotLike) => {
			Ok(!val1.contains(&val2.to_string()))
		}
		(FieldType::Prop(_, val1), Value::Regex(regex), Operator::Matches) => {
			match cached_regex(regex) {
				Ok(re) => Ok(re.is_match(val1)),
				Err(e) => Err(e.to_string()),
			}
		}
		(FieldType::Prop(_, val1), Value::Regex(regex), Operator::NotMatches) => {
			match cached_regex(regex) {
				Ok(re) => Ok(!re.is_match(val1)),
				Err(e) => Err(e.to_string()),
			}
		}
		(FieldType::Prop(_, val1), Value::String(val2), op) => {
			Ok(cmp_semver_or_string(val1, val2, op))
		}
		(FieldType::Prop(_, val1), Value::Number(val2), op) => {
			Ok(magic_cmp(val1, &val2.to_string(), op))
		}
		(FieldType::Prop(_, _), Value::Date(_), _) => todo!(),
		(field_type, Value::List(vec), Operator::In) => {
			any(field_type, vec, &Operator::Equal, logline, tz)
		}
		(field_type, Value::List(vec), Operator::NotIn) => {
			Ok(!any(field_type, vec, &Operator::Equal, logline, tz)?)
		}
		_ => Err(format!(
			"Invalid comparison {:?} {:?} {:?}",
			field, value, operator
		)),
	}
}

fn check_field_access(
	field_access: &FieldAccess,
	right: &Expr,
	op: &Operator,
	logline: &LogEntry,
	tz: &FixedOffset,
) -> Result<bool, String> {
	match field_access.expr.as_ref() {
		Expr::Value(Value::String(obj)) => match obj.as_str() {
			"timestamp" => {
				let num = match right {
					Expr::Value(Value::Number(num)) => *num as i32,
					_ => return Err("Invalid value for timestamp field".to_string()),
				};

				match field_access.field.as_str() {
					"year" => Ok(magic_cmp(
						logline.timestamp.with_timezone(tz).year(),
						num,
						op,
					)),
					"month" => Ok(magic_cmp(
						logline.timestamp.with_timezone(tz).month(),
						num as u32,
						op,
					)),
					"day" => Ok(magic_cmp(
						logline.timestamp.with_timezone(tz).day(),
						num as u32,
						op,
					)),
					"hour" => Ok(magic_cmp(
						logline.timestamp.with_timezone(tz).hour(),
						num as u32,
						op,
					)),
					"minute" => Ok(magic_cmp(
						logline.timestamp.with_timezone(tz).minute(),
						num as u32,
						op,
					)),
					"second" => Ok(magic_cmp(
						logline.timestamp.with_timezone(tz).second(),
						num as u32,
						op,
					)),
					_ => Err(format!("Field not found: {}", field_access.field)),
				}
			}
			_ => Err(format!("does not have fields: {}", obj)),
		},
		_ => Err(format!("unsupported field access: {:?}", field_access)),
	}
}

fn check_condition(cond: &Condition, logline: &LogEntry, tz: &FixedOffset) -> Result<bool, String> {
	fn match_field(
		field: &str,
		val: &Value,
		op: &Operator,
		logline: &LogEntry,
		tz: &FixedOffset,
	) -> Result<bool, String> {
		match find_field(field, logline) {
			Some(field) => does_field_match(&field, val, op, logline, tz),
			None => Ok(false),
		}
	}
	match (cond.left.as_ref(), cond.right.as_ref(), &cond.operator) {
		(Expr::Value(Value::String(left)), Expr::Value(val), op) => {
			match_field(left, val, op, logline, tz)
		}
		(Expr::Value(val), Expr::Value(Value::String(right)), op) => {
			match_field(right, val, op, logline, tz)
		}
		(Expr::Value(Value::String(left)), Expr::Empty, Operator::Exists) => {
			Ok(find_field(left, logline).is_some())
		}
		(Expr::Value(Value::String(left)), Expr::Empty, Operator::NotExists) => {
			Ok(find_field(left, logline).is_none())
		}
		(Expr::FieldAccess(field), right, op) => check_field_access(field, right, op, logline, tz),
		(left, Expr::FieldAccess(field), op) => check_field_access(field, left, op, logline, tz),
		_ => panic!(
			"Nothing makes sense anymore {:?} logline: {:?}",
			cond, logline
		),
	}
}

pub fn check_expr(expr: &Expr, logline: &LogEntry, tz: &FixedOffset) -> Result<bool, String> {
	match expr {
		Expr::Condition(cond) => check_condition(&cond, logline, tz),
		Expr::And(expr, expr1) => {
			Ok(check_expr(expr, &logline, tz)? && check_expr(expr1, logline, tz)?)
		}
		Expr::Or(expr, expr1) => {
			Ok(check_expr(expr, &logline, tz)? || check_expr(expr1, logline, tz)?)
		}
		Expr::Value(value) => match value {
			Value::String(value) => Ok(value != ""),
			Value::Regex(_) => Ok(true),
			Value::Number(value) => Ok(*value > 0),
			Value::Date(_) => Ok(true),
			Value::List(_) => Err("This is not javascript".to_string()),
		},
		Expr::Empty => Ok(true),
		_ => todo!("expr {:?} not supported yet", expr),
	}
}

pub fn check_props(expr: &Expr, props: &[Prop]) -> Result<bool, String> {
	fn is_negative_operator(op: &Operator) -> bool {
		matches!(
			op,
			Operator::NotEqual
				| Operator::NotLike
				| Operator::NotIn
				| Operator::NotExists
				| Operator::NotMatches
		)
	}

	fn check_condition(cond: &Condition, props: &[Prop]) -> Result<bool, String> {
		fn compare(prop_val: &String, query_val: &Value, op: &Operator) -> Result<bool, String> {
			match (query_val, op) {
				(Value::Regex(pattern), Operator::Matches) => {
					let re = cached_regex(pattern).map_err(|e| e.to_string())?;
					Ok(re.is_match(prop_val))
				}
				(Value::Regex(pattern), Operator::NotMatches) => {
					let re = cached_regex(pattern).map_err(|e| e.to_string())?;
					Ok(!re.is_match(prop_val))
				}
				(Value::String(query_str), _) => Ok(cmp_semver_or_string(prop_val, query_str, op)),
				(Value::Number(num), _) => Ok(magic_cmp(prop_val, &num.to_string(), op)),
				(Value::List(list), Operator::In) => any(list, prop_val, &Operator::Equal),
				_ => Ok(false),
			}
		}

		fn match_field(
			field: &String,
			val: &Value,
			op: &Operator,
			props: &[Prop],
		) -> Result<bool, String> {
			if field == "msg" || field == "timestamp" {
				return Ok(true);
			}
			for prop in props {
				if prop.key != *field {
					continue;
				}

				if compare(&prop.value, val, op)? {
					return Ok(true);
				}
			}
			return Ok(false);
		}

		fn any(list: &[Value], left: &String, op: &Operator) -> Result<bool, String> {
			for value in list {
				if compare(left, value, op)? {
					return Ok(true);
				}
			}
			return Ok(false);
		}

		fn is_ts_access(expr: &Expr) -> bool {
			match expr {
				Expr::FieldAccess(FieldAccess { expr, .. }) => {
					matches!(expr.as_ref(), Expr::Value(Value::String(s)) if s == "timestamp")
				}
				_ => false,
			}
		}

		match (cond.left.as_ref(), cond.right.as_ref(), &cond.operator) {
			(_, _, op) if is_negative_operator(op) => Ok(true),
			(Expr::Value(Value::String(left)), Expr::Value(val), op) => {
				match_field(left, val, op, props)
			}
			(left, Expr::Value(_), _) if is_ts_access(left) => Ok(true),
			(Expr::Value(_), right, _) if is_ts_access(right) => Ok(true),
			_ => Ok(false),
		}
	}

	match expr {
		Expr::Condition(cond) => check_condition(cond, props),
		Expr::And(expr, expr1) => Ok(check_props(expr, props)? && check_props(expr1, props)?),
		Expr::Or(expr, expr1) => Ok(check_props(expr, props)? || check_props(expr1, props)?),
		Expr::Value(value) => match value {
			Value::String(value) => Ok(value != ""),
			Value::Regex(_) => Ok(true),
			Value::Number(value) => Ok(*value > 0),
			Value::Date(_) => Ok(true),
			Value::List(_) => Err("This is not javascript".to_string()),
		},
		Expr::Empty => Ok(true),
		_ => todo!("expr {:?} not supported yet", expr),
	}
}

pub fn extract_date_conditions(expr: &Expr) -> Vec<Condition> {
	fn is_timestamp_field(expr: &Expr) -> bool {
		match expr {
			Expr::Value(Value::String(s)) => s == "timestamp",
			Expr::FieldAccess(FieldAccess { expr, .. }) => {
				matches!(expr.as_ref(), Expr::Value(Value::String(s)) if s == "timestamp")
			}
			_ => false,
		}
	}

	let mut out = Vec::new();

	match expr {
		Expr::Condition(cond) => {
			if is_timestamp_field(cond.left.as_ref()) || is_timestamp_field(cond.right.as_ref()) {
				out.push(cond.clone());
			}
		}
		Expr::And(left, right) | Expr::Or(left, right) => {
			out.extend(extract_date_conditions(left));
			out.extend(extract_date_conditions(right));
		}
		_ => {}
	}

	out
}

pub fn extract_device_ids(expr: &Expr) -> Vec<String> {
	fn add_id_from_value(vec: &mut Vec<String>, val: &Value) {
		match val {
			Value::String(s) => vec.push(s.clone()),
			Value::Number(n) => vec.push(n.to_string()),
			Value::List(list) => {
				for v in list {
					add_id_from_value(vec, v);
				}
			}
			_ => {}
		}
	}

	let mut ids = Vec::new();
	match expr {
		Expr::Condition(cond) => {
			if let (Expr::Value(Value::String(left)), Expr::Value(right)) =
				(cond.left.as_ref(), cond.right.as_ref())
			{
				if left == "deviceId" {
					if matches!(cond.operator, Operator::Equal | Operator::In) {
						add_id_from_value(&mut ids, right);
					}
				}
			} else if let (Expr::Value(left), Expr::Value(Value::String(right))) =
				(cond.left.as_ref(), cond.right.as_ref())
			{
				if right == "deviceId" {
					if matches!(cond.operator, Operator::Equal | Operator::In) {
						add_id_from_value(&mut ids, left);
					}
				}
			}
		}
		Expr::And(l, r) | Expr::Or(l, r) => {
			ids.extend(extract_device_ids(l));
			ids.extend(extract_device_ids(r));
		}
		_ => {}
	}

	ids.sort();
	ids.dedup();
	ids
}

pub fn timestamp_bounds(expr: &Expr) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
	let mut start: Option<DateTime<Utc>> = None;
	let mut end: Option<DateTime<Utc>> = None;

	for cond in extract_date_conditions(expr) {
		if let (Expr::Value(Value::String(f)), Expr::Value(Value::Date(d))) =
			(cond.left.as_ref(), cond.right.as_ref())
		{
			if f == "timestamp" {
				match cond.operator {
					Operator::GreaterThan | Operator::GreaterThanOrEqual => {
						if start.map_or(true, |s| *d > s) {
							start = Some(*d);
						}
					}
					Operator::LessThan | Operator::LessThanOrEqual => {
						if end.map_or(true, |e| *d < e) {
							end = Some(*d);
						}
					}
					Operator::Equal => {
						start = Some(*d);
						end = Some(*d);
					}
					_ => {}
				}
			}
		} else if let (Expr::Value(Value::Date(d)), Expr::Value(Value::String(f))) =
			(cond.left.as_ref(), cond.right.as_ref())
		{
			if f == "timestamp" {
				let op = match cond.operator {
					Operator::GreaterThan => Operator::LessThan,
					Operator::GreaterThanOrEqual => Operator::LessThanOrEqual,
					Operator::LessThan => Operator::GreaterThan,
					Operator::LessThanOrEqual => Operator::GreaterThanOrEqual,
					o => o,
				};
				match op {
					Operator::GreaterThan | Operator::GreaterThanOrEqual => {
						if start.map_or(true, |s| *d > s) {
							start = Some(*d);
						}
					}
					Operator::LessThan | Operator::LessThanOrEqual => {
						if end.map_or(true, |e| *d < e) {
							end = Some(*d);
						}
					}
					Operator::Equal => {
						start = Some(*d);
						end = Some(*d);
					}
					_ => {}
				}
			}
		}
	}

	(start, end)
}
pub fn match_date_range(
	expr: &Expr,
	first: chrono::DateTime<Utc>,
	last: chrono::DateTime<Utc>,
	tz: &FixedOffset,
) -> bool {
	fn cond_matches(
		cond: &Condition,
		first: chrono::DateTime<Utc>,
		last: chrono::DateTime<Utc>,
		tz: &FixedOffset,
	) -> bool {
		fn range_cmp(min: i64, max: i64, val: i64, op: &Operator) -> bool {
			match op {
				Operator::Equal => val >= min && val <= max,
				Operator::GreaterThan => max > val,
				Operator::GreaterThanOrEqual => max >= val,
				Operator::LessThan => min < val,
				Operator::LessThanOrEqual => min <= val,
				_ => true,
			}
		}

		match (cond.left.as_ref(), cond.right.as_ref()) {
			(Expr::Value(Value::String(f)), Expr::Value(Value::Date(d))) if f == "timestamp" => {
				match cond.operator {
					Operator::Equal => *d >= first && *d <= last,
					Operator::GreaterThan => last > *d,
					Operator::GreaterThanOrEqual => last >= *d,
					Operator::LessThan => first < *d,
					Operator::LessThanOrEqual => first <= *d,
					_ => true,
				}
			}
			(Expr::FieldAccess(FieldAccess { expr, field }), Expr::Value(Value::Number(n))) if matches!(expr.as_ref(), Expr::Value(Value::String(s)) if s == "timestamp") =>
			{
				let f = first.with_timezone(tz);
				let l = last.with_timezone(tz);
				let (min, max) = match field.as_str() {
					"year" => (f.year() as i64, l.year() as i64),
					"month" => (f.month() as i64, l.month() as i64),
					"day" => (f.day() as i64, l.day() as i64),
					"hour" => (f.hour() as i64, l.hour() as i64),
					"minute" => (f.minute() as i64, l.minute() as i64),
					"second" => (f.second() as i64, l.second() as i64),
					_ => return true,
				};
				range_cmp(min, max, *n, &cond.operator)
			}
			_ => true,
		}
	}

	for c in extract_date_conditions(expr) {
		if !cond_matches(&c, first, last, tz) {
			return false;
		}
	}
	true
}
#[cfg(test)]
mod tests {
	use super::*;
	use crate::FieldAccess;
	use crate::Prop;
	use chrono::DateTime;
	use chrono::Utc;

	#[test]
	fn matches_props() {
		let props = vec![
			Prop {
				key: "service".to_string(),
				value: "auth".to_string(),
			},
			Prop {
				key: "user_id".to_string(),
				value: "123".to_string(),
			},
			Prop {
				key: "duration_ms".to_string(),
				value: "150".to_string(),
			},
		];
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::String("auth".to_string()))),
		});
		assert!(check_props(&expr, &props).unwrap());
	}

	#[test]
	fn matches_props_with_many_same_kesy() {
		let props = vec![
			Prop {
				key: "service".to_string(),
				value: "auth".to_string(),
			},
			Prop {
				key: "user_id".to_string(),
				value: "123".to_string(),
			},
			Prop {
				key: "duration_ms".to_string(),
				value: "150".to_string(),
			},
			Prop {
				key: "service".to_string(),
				value: "auth2".to_string(),
			},
		];
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::String("auth2".to_string()))),
		});
		assert!(check_props(&expr, &props).unwrap());
	}

	#[test]
	fn matches_number_props() {
		let props = vec![
			Prop {
				key: "service".to_string(),
				value: "auth".to_string(),
			},
			Prop {
				key: "user_id".to_string(),
				value: "123".to_string(),
			},
			Prop {
				key: "duration_ms".to_string(),
				value: "150".to_string(),
			},
		];
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("duration_ms".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::Number(150))),
		});
		assert!(check_props(&expr, &props).unwrap());
	}

	#[test]
	fn does_not_match_props() {
		let props = vec![
			Prop {
				key: "service".to_string(),
				value: "auth".to_string(),
			},
			Prop {
				key: "user_id".to_string(),
				value: "123".to_string(),
			},
			Prop {
				key: "duration_ms".to_string(),
				value: "150".to_string(),
			},
		];
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::String("wrong_service".to_string()))),
		});
		assert!(!check_props(&expr, &props).unwrap());
	}

	#[test]
	fn matches_and_with_props() {
		let props = vec![
			Prop {
				key: "service".to_string(),
				value: "auth".to_string(),
			},
			Prop {
				key: "user_id".to_string(),
				value: "123".to_string(),
			},
			Prop {
				key: "duration_ms".to_string(),
				value: "150".to_string(),
			},
		];
		let expr = Expr::And(
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("service".to_string()))),
				operator: Operator::Equal,
				right: Box::new(Expr::Value(Value::String("auth".to_string()))),
			})),
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("user_id".to_string()))),
				operator: Operator::Equal,
				right: Box::new(Expr::Value(Value::String("123".to_string()))),
			})),
		);
		assert!(check_props(&expr, &props).unwrap());
	}

	#[test]
	fn ignore_timestamp_fields_in_props_check() {
		let props = vec![Prop {
			key: "deviceId".to_string(),
			value: "237865".to_string(),
		}];

		let expr = Expr::And(
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("deviceId".to_string()))),
				operator: Operator::Equal,
				right: Box::new(Expr::Value(Value::Number(237865))),
			})),
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::FieldAccess(FieldAccess {
					expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
					field: "month".to_string(),
				})),
				operator: Operator::Equal,
				right: Box::new(Expr::Value(Value::Number(4))),
			})),
		);

		assert!(check_props(&expr, &props).unwrap());
	}

	#[test]
	fn extract_date_conditions_from_query() {
		let ast = crate::parse_log_query("deviceId = 237865 and timestamp.month = 4").unwrap();
		let conds = extract_date_conditions(&ast.root);
		assert_eq!(conds.len(), 1);
		if let Expr::FieldAccess(_) = *conds[0].left {}
		assert_eq!(conds[0].operator, Operator::Equal);
	}

	#[test]
	fn extract_device_ids_basic() {
		let ast =
			crate::parse_log_query("deviceId = 1 or 2 = deviceId or deviceId in (3 , 4)").unwrap();
		let ids = extract_device_ids(&ast.root);
		assert_eq!(ids, vec!["1", "2", "3", "4"]);
	}

	#[test]
	fn extract_device_ids_negative() {
		let ast = crate::parse_log_query("deviceId != 5 and deviceId not in (6)").unwrap();
		let ids = extract_device_ids(&ast.root);
		assert!(ids.is_empty());
	}
	#[test]
	fn match_date_range_month() {
		let expr = crate::parse_log_query("timestamp.month = 4").unwrap();
		let start = DateTime::<Utc>::from_utc(
			chrono::NaiveDate::from_ymd_opt(2025, 4, 1)
				.unwrap()
				.and_hms_opt(0, 0, 0)
				.unwrap(),
			Utc,
		);
		let end = DateTime::<Utc>::from_utc(
			chrono::NaiveDate::from_ymd_opt(2025, 4, 30)
				.unwrap()
				.and_hms_opt(0, 0, 0)
				.unwrap(),
			Utc,
		);
		let tz = chrono::FixedOffset::east_opt(0).unwrap();
		assert!(match_date_range(&expr.root, start, end, &tz));
		let start2 = DateTime::<Utc>::from_utc(
			chrono::NaiveDate::from_ymd_opt(2025, 5, 1)
				.unwrap()
				.and_hms_opt(0, 0, 0)
				.unwrap(),
			Utc,
		);
		let end2 = DateTime::<Utc>::from_utc(
			chrono::NaiveDate::from_ymd_opt(2025, 5, 31)
				.unwrap()
				.and_hms_opt(0, 0, 0)
				.unwrap(),
			Utc,
		);
		assert!(!match_date_range(&expr.root, start2, end2, &tz));
	}

	#[test]
	fn timestamp_bounds_basic() {
		let date1 = DateTime::<Utc>::from_utc(
			chrono::NaiveDate::from_ymd_opt(2025, 1, 1)
				.unwrap()
				.and_hms_opt(0, 0, 0)
				.unwrap(),
			Utc,
		);
		let date2 = DateTime::<Utc>::from_utc(
			chrono::NaiveDate::from_ymd_opt(2025, 1, 31)
				.unwrap()
				.and_hms_opt(0, 0, 0)
				.unwrap(),
			Utc,
		);
		let ast = Expr::And(
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("timestamp".into()))),
				operator: Operator::GreaterThanOrEqual,
				right: Box::new(Expr::Value(Value::Date(date1))),
			})),
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("timestamp".into()))),
				operator: Operator::LessThanOrEqual,
				right: Box::new(Expr::Value(Value::Date(date2))),
			})),
		);
		let (start, end) = timestamp_bounds(&ast);
		let start_expected = date1;
		let end_expected = date2;
		assert_eq!(start, Some(start_expected));
		assert_eq!(end, Some(end_expected));
	}

	#[test]
	fn timestamp_bounds_reversed() {
		let date1 = DateTime::<Utc>::from_utc(
			chrono::NaiveDate::from_ymd_opt(2025, 2, 1)
				.unwrap()
				.and_hms_opt(0, 0, 0)
				.unwrap(),
			Utc,
		);
		let date2 = DateTime::<Utc>::from_utc(
			chrono::NaiveDate::from_ymd_opt(2025, 3, 1)
				.unwrap()
				.and_hms_opt(0, 0, 0)
				.unwrap(),
			Utc,
		);
		let ast = Expr::And(
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::Date(date2))),
				operator: Operator::GreaterThan,
				right: Box::new(Expr::Value(Value::String("timestamp".into()))),
			})),
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::Date(date1))),
				operator: Operator::LessThanOrEqual,
				right: Box::new(Expr::Value(Value::String("timestamp".into()))),
			})),
		);
		let (start, end) = timestamp_bounds(&ast);
		let start_expected = date1;
		let end_expected = date2;
		assert_eq!(start, Some(start_expected));
		assert_eq!(end, Some(end_expected));
	}
	#[test]
	fn msg_does_not_match() {
		let logline = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "key".to_string(),
				value: "value".to_string(),
			}],
			msg: "Hello, world!".to_string(),
			..Default::default()
		};

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("msg".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::String("Hello".to_string()))),
		});
		assert!(!check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn msg_matches() {
		let logline = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "key".to_string(),
				value: "value".to_string(),
			}],
			msg: "Hello, world!".to_string(),
			..Default::default()
		};

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("msg".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::String("Hello, world!".to_string()))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	fn create_test_log_entry() -> LogEntry {
		LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![
				Prop {
					key: "service".to_string(),
					value: "auth".to_string(),
				},
				Prop {
					key: "user_id".to_string(),
					value: "123".to_string(),
				},
				Prop {
					key: "duration_ms".to_string(),
					value: "150".to_string(),
				},
			],
			msg: "User login successful".to_string(),
			..Default::default()
		}
	}

	#[test]
	fn test_match_field() {
		let log = create_test_log_entry();

		assert!(matches!(
			find_field("timestamp", &log),
			Some(FieldType::Timestamp)
		));
		assert!(matches!(find_field("level", &log), Some(FieldType::Level)));
		assert!(matches!(find_field("msg", &log), Some(FieldType::Msg)));

		if let Some(FieldType::Prop(key, val)) = find_field("service", &log) {
			assert_eq!(key, "service");
			assert_eq!(val, "auth");
		} else {
			panic!("Expected Prop field type for 'service'");
		}

		assert!(find_field("nonexistent", &log).is_none());
	}

	#[test]
	fn test_magic_cmp() {
		assert!(magic_cmp(5, 5, &Operator::Equal));
		assert!(magic_cmp(6, 5, &Operator::GreaterThan));
		assert!(magic_cmp(5, 5, &Operator::GreaterThanOrEqual));
		assert!(magic_cmp(4, 5, &Operator::LessThan));
		assert!(magic_cmp(5, 5, &Operator::LessThanOrEqual));

		assert!(!magic_cmp(5, 6, &Operator::Equal));
		assert!(!magic_cmp(5, 6, &Operator::GreaterThan));
	}

	#[test]
	fn test_level_comparison() {
		let log = create_test_log_entry();

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("level".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::String("INFO".to_string()))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("level".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::String("ERROR".to_string()))),
		});
		assert!(!check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn test_property_matching() {
		let log = create_test_log_entry();

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::String("auth".to_string()))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("duration_ms".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::Number(150))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".to_string()))),
			operator: Operator::NotEqual,
			right: Box::new(Expr::Value(Value::String("auth".to_string()))),
		});
		assert!(!check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("duration_ms".to_string()))),
			operator: Operator::NotEqual,
			right: Box::new(Expr::Value(Value::Number(200))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn test_message_like_operator() {
		let log = create_test_log_entry();

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("msg".to_string()))),
			operator: Operator::Like,
			right: Box::new(Expr::Value(Value::String("login".to_string()))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("msg".to_string()))),
			operator: Operator::Like,
			right: Box::new(Expr::Value(Value::String("logout".to_string()))),
		});
		assert!(!check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("msg".to_string()))),
			operator: Operator::NotLike,
			right: Box::new(Expr::Value(Value::String("login".to_string()))),
		});
		assert!(!check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("msg".to_string()))),
			operator: Operator::NotLike,
			right: Box::new(Expr::Value(Value::String("logout".to_string()))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn message_like_with_quotes() {
		let mut log = create_test_log_entry();
		log.msg = "An \"error\" occurred".to_string();
		let ast = crate::parse_log_query(r#"msg like "\"error\"""#).unwrap();
		assert!(check_expr(&ast.root, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn test_prop_like_operator() {
		let log = create_test_log_entry();

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".to_string()))),
			operator: Operator::Like,
			right: Box::new(Expr::Value(Value::String("au".to_string()))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".to_string()))),
			operator: Operator::Like,
			right: Box::new(Expr::Value(Value::String("asdf".to_string()))),
		});
		assert!(!check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".to_string()))),
			operator: Operator::NotLike,
			right: Box::new(Expr::Value(Value::String("au".to_string()))),
		});
		assert!(!check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".to_string()))),
			operator: Operator::NotLike,
			right: Box::new(Expr::Value(Value::String("asdf".to_string()))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn test_regex_operators() {
		let log = create_test_log_entry();

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("msg".to_string()))),
			operator: Operator::Matches,
			right: Box::new(Expr::Value(Value::Regex("^User.*success".to_string()))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".to_string()))),
			operator: Operator::NotMatches,
			right: Box::new(Expr::Value(Value::Regex("^auth$".to_string()))),
		});
		assert!(!check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn test_compound_expressions() {
		let log = create_test_log_entry();

		// Test AND expression
		let expr = Expr::And(
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("service".to_string()))),
				operator: Operator::Equal,
				right: Box::new(Expr::Value(Value::String("auth".to_string()))),
			})),
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("user_id".to_string()))),
				operator: Operator::Equal,
				right: Box::new(Expr::Value(Value::String("123".to_string()))),
			})),
		);
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		// Test OR expression
		let expr = Expr::Or(
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("service".to_string()))),
				operator: Operator::Equal,
				right: Box::new(Expr::Value(Value::String("wrong".to_string()))),
			})),
			Box::new(Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("user_id".to_string()))),
				operator: Operator::Equal,
				right: Box::new(Expr::Value(Value::String("123".to_string()))),
			})),
		);
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn test_invalid_comparisons() {
		let log = create_test_log_entry();

		// Test invalid level comparison
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("level".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::Date(Utc::now()))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).is_err());

		// Test invalid message comparison
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("msg".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::Date(Utc::now()))),
		});
		assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).is_err());
	}

	#[test]
	fn test_empty_and_value_expressions() {
		let log = create_test_log_entry();

		let tz = chrono::FixedOffset::east_opt(0).unwrap();
		assert!(check_expr(&Expr::Empty, &log, &tz).unwrap());
		assert!(check_expr(
			&Expr::Value(Value::String("nonempty".to_string())),
			&log,
			&tz
		)
		.unwrap());
		assert!(!check_expr(&Expr::Value(Value::String("".to_string())), &log, &tz).unwrap());
		assert!(check_expr(&Expr::Value(Value::Number(1)), &log, &tz).unwrap());
		assert!(!check_expr(&Expr::Value(Value::Number(0)), &log, &tz).unwrap());
		assert!(check_expr(&Expr::Value(Value::Date(Utc::now())), &log, &tz).unwrap());
	}

	#[test]
	fn test_in_eval() {
		let logline = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "key".to_string(),
				value: "value".to_string(),
			}],
			msg: "Hello, world!".to_string(),
			..Default::default()
		};

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("level".to_string()))),
			operator: Operator::In,
			right: Box::new(Expr::Value(Value::List(vec![
				Value::String("info".to_string()),
				Value::String("debug".to_string()),
			]))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("level".to_string()))),
			operator: Operator::In,
			right: Box::new(Expr::Value(Value::List(vec![
				Value::String("error".to_string()),
				Value::String("warn".to_string()),
			]))),
		});
		assert!(!check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn test_exsits() {
		let logline = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "key".to_string(),
				value: "value".to_string(),
			}],
			msg: "Hello, world!".to_string(),
			..Default::default()
		};
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("key".to_string()))),
			operator: Operator::Exists,
			right: Box::new(Expr::Empty),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("nonexistent".to_string()))),
			operator: Operator::Exists,
			right: Box::new(Expr::Empty),
		});
		assert!(!check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("nonexistent".to_string()))),
			operator: Operator::NotExists,
			right: Box::new(Expr::Empty),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("key".to_string()))),
			operator: Operator::NotExists,
			right: Box::new(Expr::Empty),
		});
		assert!(!check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn test_timestamp_fields() {
		let logline = LogEntry {
			timestamp: DateTime::from_utc(
				chrono::NaiveDate::from_ymd_opt(2024, 5, 15)
					.unwrap()
					.and_hms_opt(0, 0, 0)
					.unwrap(),
				Utc,
			),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "key".to_string(),
				value: "value".to_string(),
			}],
			msg: "Hello, world!".to_string(),
			..Default::default()
		};
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::FieldAccess(FieldAccess {
				expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
				field: "year".to_string(),
			})),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::Number(2024))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::FieldAccess(FieldAccess {
				expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
				field: "month".to_string(),
			})),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::Number(5))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::FieldAccess(FieldAccess {
				expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
				field: "day".to_string(),
			})),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::Number(15))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::FieldAccess(FieldAccess {
				expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
				field: "hour".to_string(),
			})),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::Number(0))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::FieldAccess(FieldAccess {
				expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
				field: "minute".to_string(),
			})),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::Number(0))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::FieldAccess(FieldAccess {
				expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
				field: "second".to_string(),
			})),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::Number(0))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn test_semver_comparison() {
		let logline = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			props: vec![Prop {
				key: "version".to_string(),
				value: "1.10.0".to_string(),
			}],
			msg: "".to_string(),
			..Default::default()
		};

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("version".to_string()))),
			operator: Operator::GreaterThan,
			right: Box::new(Expr::Value(Value::String("1.2.0".to_string()))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("version".to_string()))),
			operator: Operator::LessThan,
			right: Box::new(Expr::Value(Value::String("2.0.0".to_string()))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());

		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("version".to_string()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::String("1.10.0".to_string()))),
		});
		assert!(check_expr(&expr, &logline, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
	}

	#[test]
	fn performance_regex_evaluation() {
		let log = create_test_log_entry();
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("msg".to_string()))),
			operator: Operator::Matches,
			right: Box::new(Expr::Value(Value::Regex("^User.*success".to_string()))),
		});
		let start = std::time::Instant::now();
		for _ in 0..1000 {
			assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
		}
		assert!(start.elapsed().as_secs_f32() < 1.0);
	}

	#[test]
	fn performance_like_evaluation() {
		let log = create_test_log_entry();
		let expr = Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("msg".to_string()))),
			operator: Operator::Like,
			right: Box::new(Expr::Value(Value::String("User".to_string()))),
		});
		let start = std::time::Instant::now();
		for _ in 0..1000 {
			assert!(check_expr(&expr, &log, &chrono::FixedOffset::east_opt(0).unwrap()).unwrap());
		}
		assert!(start.elapsed().as_secs_f32() < 1.0);
	}
}

#[test]
fn negative_operator_does_not_skip_props() {
	let props = vec![Prop {
		key: "service".into(),
		value: "auth".into(),
	}];
	let expr = Expr::Condition(Condition {
		left: Box::new(Expr::Value(Value::String("service".into()))),
		operator: Operator::NotEqual,
		right: Box::new(Expr::Value(Value::String("auth".into()))),
	});
	assert!(check_props(&expr, &props).unwrap());
}

#[test]
fn negative_check_and_positive_match() {
	let props = vec![Prop {
		key: "service".into(),
		value: "auth".into(),
	}];
	let expr = Expr::And(
		Box::new(Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".into()))),
			operator: Operator::NotEqual,
			right: Box::new(Expr::Value(Value::String("auth".into()))),
		})),
		Box::new(Expr::Condition(Condition {
			left: Box::new(Expr::Value(Value::String("service".into()))),
			operator: Operator::Equal,
			right: Box::new(Expr::Value(Value::String("auth".into()))),
		})),
	);
	assert!(check_props(&expr, &props).unwrap());
}

#[test]
fn match_date_range_timestamp_greater_than() {
	use crate::query_parsing::{Condition, Operator, Value};
	use crate::Expr;
	use chrono::{DateTime, FixedOffset, NaiveDate, Utc};

	// Build an AST equivalent to: `timestamp > 2025‑05‑01T00:00:00Z`
	let ts_cut = DateTime::<Utc>::from_utc(
		NaiveDate::from_ymd_opt(2025, 5, 1)
			.unwrap()
			.and_hms_opt(0, 0, 0)
			.unwrap(),
		Utc,
	);
	let ast = Expr::Condition(Condition {
		left: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
		operator: Operator::GreaterThan,
		right: Box::new(Expr::Value(Value::Date(ts_cut))),
	});

	let tz = FixedOffset::east_opt(0).unwrap();

	// Segment completely *before* the cut‑off date should not match.
	let seg_start = DateTime::<Utc>::from_utc(
		NaiveDate::from_ymd_opt(2025, 4, 1)
			.unwrap()
			.and_hms_opt(0, 0, 0)
			.unwrap(),
		Utc,
	);
	let seg_end = DateTime::<Utc>::from_utc(
		NaiveDate::from_ymd_opt(2025, 4, 30)
			.unwrap()
			.and_hms_opt(23, 59, 59)
			.unwrap(),
		Utc,
	);
	assert!(
		!match_date_range(&ast, seg_start, seg_end, &tz),
		"segment that ends before cut‑off should not match",
	);

	// Segment entirely *after* the cut‑off date should match.
	let seg_start2 = DateTime::<Utc>::from_utc(
		NaiveDate::from_ymd_opt(2025, 6, 1)
			.unwrap()
			.and_hms_opt(0, 0, 0)
			.unwrap(),
		Utc,
	);
	let seg_end2 = DateTime::<Utc>::from_utc(
		NaiveDate::from_ymd_opt(2025, 6, 30)
			.unwrap()
			.and_hms_opt(23, 59, 59)
			.unwrap(),
		Utc,
	);
	assert!(
		match_date_range(&ast, seg_start2, seg_end2, &tz),
		"segment after cut‑off should match",
	);
}

#[test]
fn match_date_range_year_greater_equal() {
	use crate::query_parsing::{Condition, Operator, Value};
	use crate::Expr;
	use chrono::{DateTime, FixedOffset, NaiveDate, Utc};

	// Equivalent to: `timestamp.year >= 2024`
	let ast = Expr::Condition(Condition {
		left: Box::new(Expr::FieldAccess(crate::FieldAccess {
			expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
			field: "year".to_string(),
		})),
		operator: Operator::GreaterThanOrEqual,
		right: Box::new(Expr::Value(Value::Number(2024))),
	});

	let tz = FixedOffset::east_opt(0).unwrap();

	// Segment wholly in 2023 should NOT match.
	let seg_start = DateTime::<Utc>::from_utc(
		NaiveDate::from_ymd_opt(2023, 12, 1)
			.unwrap()
			.and_hms_opt(0, 0, 0)
			.unwrap(),
		Utc,
	);
	let seg_end = DateTime::<Utc>::from_utc(
		NaiveDate::from_ymd_opt(2023, 12, 31)
			.unwrap()
			.and_hms_opt(23, 59, 59)
			.unwrap(),
		Utc,
	);
	assert!(
		!match_date_range(&ast, seg_start, seg_end, &tz),
		"segment in 2023 should not match year >= 2024 query",
	);

	// Segment in 2025 should match.
	let seg_start2 = DateTime::<Utc>::from_utc(
		NaiveDate::from_ymd_opt(2025, 1, 1)
			.unwrap()
			.and_hms_opt(0, 0, 0)
			.unwrap(),
		Utc,
	);
	let seg_end2 = DateTime::<Utc>::from_utc(
		NaiveDate::from_ymd_opt(2025, 12, 31)
			.unwrap()
			.and_hms_opt(23, 59, 59)
			.unwrap(),
		Utc,
	);
	assert!(
		match_date_range(&ast, seg_start2, seg_end2, &tz),
		"segment in 2025 should satisfy year >= 2024",
	);
}
