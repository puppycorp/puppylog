use chrono::{Datelike, FixedOffset, Timelike};

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

		match (cond.left.as_ref(), cond.right.as_ref(), &cond.operator) {
			(Expr::Value(Value::String(left)), Expr::Value(val), op) => {
				match_field(left, val, op, props)
			}
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
