use puppylog::{LogEntry, LogLevel};
use crate::log_query::{Condition, Expr, Operator, Value};

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

    for (key, val) in &logline.props {
        if key == v {
            return Some(FieldType::Prop(key.clone(), val.clone()));
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
        Operator::GreaterThan => left > right,
        Operator::GreaterThanOrEqual => left >= right,
        Operator::LessThan => left < right,
        Operator::LessThanOrEqual => left <= right,
        _ => todo!("operator {:?} not supported yet", op),
    }
}

fn any(field: &FieldType, values: &[Value], op: &Operator, logline: &LogEntry) -> Result<bool, String> {
    for value in values {
        if does_field_match(field, value, op, logline)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn does_field_match(field: &FieldType, value: &Value, operator: &Operator, logline: &LogEntry) -> Result<bool, String> {
    match (field, value, operator) {
        (FieldType::Msg, Value::String(val), Operator::Like) => Ok(logline.msg.to_lowercase().contains(&val.to_lowercase())),
        (FieldType::Msg, Value::String(val), Operator::NotLike) => Ok(!logline.msg.to_lowercase().contains(&val.to_lowercase())),
        (FieldType::Timestamp, Value::Date(val), op) => Ok(magic_cmp(logline.timestamp, *val, op)),
        (FieldType::Timestamp, _ , _) => Err(format!("Invalid value for timestamp {:?}", value)),
        (FieldType::Level, Value::String(val), op) => Ok(magic_cmp(&logline.level, &LogLevel::from_string(&val), op)),
        (FieldType::Level, Value::Date(d), _) => Err(format!("Invalid value for level {:?}", d)),
        (FieldType::Level, Value::Number(l), op) => Ok(magic_cmp(&logline.level, &LogLevel::from_i64(*l), op)),
        (FieldType::Msg, Value::String(val), op) => Ok(magic_cmp(&logline.msg, val, op)),
        (FieldType::Msg, Value::Number(n), op) => Ok(magic_cmp(&logline.msg, &n.to_string(), op)),
        (FieldType::Msg, Value::Date(d), _) => Err(format!("Invalid value for msg {:?}", d)),
        (FieldType::Prop(_, val1), Value::String(val2), op) => Ok(magic_cmp(val1, val2, op)),
        (FieldType::Prop(_, val1), Value::Number(val2), op) => Ok(magic_cmp(val1, &val2.to_string(), op)),
        (FieldType::Prop(_, _), Value::Date(_), _) => todo!(),
        (field_type, Value::List(vec), Operator::In) => any(field_type, vec, &Operator::Equal, logline),
        (field_type, Value::List(vec), Operator::NotIn) => Ok(!any(field_type, vec, &Operator::Equal, logline)?),
        _ => Err(format!("Invalid comparison {:?} {:?} {:?}", field, value, operator))
    }
}

fn check_condition(cond: &Condition, logline: &LogEntry) -> Result<bool, String> {
    fn match_field(field: &str, val: &Value, op: &Operator, logline: &LogEntry) -> Result<bool, String> {
        match find_field(field, logline) {
            Some(field) => does_field_match(&field, val, op, logline),
            None => Ok(false)
        }
    }
    match (cond.left.as_ref(), cond.right.as_ref(), &cond.operator) {
        (Expr::Value(Value::String(left)), Expr::Value(val), op) => match_field(left, val, op, logline),
        (Expr::Value(val), Expr::Value(Value::String(right)), op) => match_field(right, val, op, logline),
        (Expr::Value(Value::String(left)), Expr::Empty, Operator::Exists) => Ok(find_field(left, logline).is_some()),
        (Expr::Value(Value::String(left)), Expr::Empty, Operator::NotExists) => Ok(find_field(left, logline).is_none()), 
        _ => panic!("Nothing makes sense anymore {:?} logline: {:?}", cond, logline)
    }
}


    // match (cond.left.as_ref(), cond.right.as_ref()) {
    //     (Expr::Value(Value::String(left)), Expr::Value(val)) => {
    //         match find_field(&left, logline) {
    //             Some(field) => does_field_match(field , val, &cond.operator, logline),
    //             None => Ok(false)
    //         }
    //     },
    //     (Expr::Value(val), Expr::Value(Value::String(right))) => {
    //         match find_field(&right, logline) {
    //             Some(field) => does_field_match(field, val, &cond.operator, logline),
    //             None => Ok(false)
    //         }
    //     },
    //     (Expr::Value(Value::String(left)), Expr::List(list)) => {
    //         match find_field(&left, logline) {
    //             Some(_) => todo!(),
    //             None => todo!(),
    //         }

    //         match find_field(&left, logline) {
    //             Some(field) => {
    //                 for expr in list {
    //                     if check_expr(expr, logline)? {
    //                         return Ok(true);
    //                     }
    //                 }
    //                 Ok(false)
    //             },
    //             None => Ok(false)
    //         }
    //     },
    //     _ => {
    //         panic!("Nothing makes sense anymore {:?} logline: {:?}", cond, logline)
    //     }
    // }

pub fn check_expr(expr: &Expr, logline: &LogEntry) -> Result<bool, String> {
	match expr {
		Expr::Condition(cond) => check_condition(&cond, logline),
		Expr::And(expr, expr1) => Ok(check_expr(expr, &logline)? && check_expr(expr1, logline)?),
		Expr::Or(expr, expr1) => Ok(check_expr(expr, &logline)? || check_expr(expr1, logline)?),
		Expr::Value(value) => match value {
			Value::String(value) => Ok(value != ""),
			Value::Number(value) => Ok(*value > 0),
			Value::Date(value) => Ok(true),
            Value::List(_) => Err("This is not javascript".to_string())
		},
		Expr::Empty => Ok(true),
        _ => todo!("expr {:?} not supported yet", expr),
	}
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn msg_does_not_match() {
        let logline = LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            props: vec![("key".to_string(), "value".to_string())],
            msg: "Hello, world!".to_string()
        };

        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("msg".to_string()))),
            operator: Operator::Equal,
            right: Box::new(Expr::Value(Value::String("Hello".to_string())))
        });
        assert!(!check_expr(&expr, &logline).unwrap());
    }

    #[test]
    fn msg_matches() {
        let logline = LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            props: vec![("key".to_string(), "value".to_string())],
            msg: "Hello, world!".to_string()
        };

        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("msg".to_string()))),
            operator: Operator::Equal,
            right: Box::new(Expr::Value(Value::String("Hello, world!".to_string())))
        });
        assert!(check_expr(&expr, &logline).unwrap());
    }

    fn create_test_log_entry() -> LogEntry {
        LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            props: vec![
                ("service".to_string(), "auth".to_string()),
                ("user_id".to_string(), "123".to_string()),
                ("duration_ms".to_string(), "150".to_string()),
            ],
            msg: "User login successful".to_string()
        }
    }

    #[test]
    fn test_match_field() {
        let log = create_test_log_entry();
        
        assert!(matches!(find_field("timestamp", &log), Some(FieldType::Timestamp)));
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
            right: Box::new(Expr::Value(Value::String("INFO".to_string())))
        });
        assert!(check_expr(&expr, &log).unwrap());
        
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("level".to_string()))),
            operator: Operator::Equal,
            right: Box::new(Expr::Value(Value::String("ERROR".to_string())))
        });
        assert!(!check_expr(&expr, &log).unwrap());
    }

    #[test]
    fn test_property_matching() {
        let log = create_test_log_entry();
        
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("service".to_string()))),
            operator: Operator::Equal,
            right: Box::new(Expr::Value(Value::String("auth".to_string())))
        });
        assert!(check_expr(&expr, &log).unwrap());
        
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("duration_ms".to_string()))),
            operator: Operator::Equal,
            right: Box::new(Expr::Value(Value::Number(150)))
        });
        assert!(check_expr(&expr, &log).unwrap());
    }

    #[test]
    fn test_message_like_operator() {
        let log = create_test_log_entry();
        
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("msg".to_string()))),
            operator: Operator::Like,
            right: Box::new(Expr::Value(Value::String("login".to_string())))
        });
        assert!(check_expr(&expr, &log).unwrap());
        
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("msg".to_string()))),
            operator: Operator::Like,
            right: Box::new(Expr::Value(Value::String("logout".to_string())))
        });
        assert!(!check_expr(&expr, &log).unwrap());

        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("msg".to_string()))),
            operator: Operator::NotLike,
            right: Box::new(Expr::Value(Value::String("login".to_string())))
        });
        assert!(!check_expr(&expr, &log).unwrap());

        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("msg".to_string()))),
            operator: Operator::NotLike,
            right: Box::new(Expr::Value(Value::String("logout".to_string())))
        });
        assert!(check_expr(&expr, &log).unwrap());

    }

    #[test]
    fn test_compound_expressions() {
        let log = create_test_log_entry();
        
        // Test AND expression
        let expr = Expr::And(
            Box::new(Expr::Condition(Condition {
                left: Box::new(Expr::Value(Value::String("service".to_string()))),
                operator: Operator::Equal,
                right: Box::new(Expr::Value(Value::String("auth".to_string())))
            })),
            Box::new(Expr::Condition(Condition {
                left: Box::new(Expr::Value(Value::String("user_id".to_string()))),
                operator: Operator::Equal,
                right: Box::new(Expr::Value(Value::String("123".to_string())))
            }))
        );
        assert!(check_expr(&expr, &log).unwrap());
        
        // Test OR expression
        let expr = Expr::Or(
            Box::new(Expr::Condition(Condition {
                left: Box::new(Expr::Value(Value::String("service".to_string()))),
                operator: Operator::Equal,
                right: Box::new(Expr::Value(Value::String("wrong".to_string())))
            })),
            Box::new(Expr::Condition(Condition {
                left: Box::new(Expr::Value(Value::String("user_id".to_string()))),
                operator: Operator::Equal,
                right: Box::new(Expr::Value(Value::String("123".to_string())))
            }))
        );
        assert!(check_expr(&expr, &log).unwrap());
    }

    #[test]
    fn test_invalid_comparisons() {
        let log = create_test_log_entry();
        
        // Test invalid level comparison
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("level".to_string()))),
            operator: Operator::Equal,
            right: Box::new(Expr::Value(Value::Date(Utc::now())))
        });
        assert!(check_expr(&expr, &log).is_err());
        
        // Test invalid message comparison
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("msg".to_string()))),
            operator: Operator::Equal,
            right: Box::new(Expr::Value(Value::Date(Utc::now())))
        });
        assert!(check_expr(&expr, &log).is_err());
    }

    #[test]
    fn test_empty_and_value_expressions() {
        let log = create_test_log_entry();
        
        assert!(check_expr(&Expr::Empty, &log).unwrap());
        assert!(check_expr(&Expr::Value(Value::String("nonempty".to_string())), &log).unwrap());
        assert!(!check_expr(&Expr::Value(Value::String("".to_string())), &log).unwrap());
        assert!(check_expr(&Expr::Value(Value::Number(1)), &log).unwrap());
        assert!(!check_expr(&Expr::Value(Value::Number(0)), &log).unwrap());
        assert!(check_expr(&Expr::Value(Value::Date(Utc::now())), &log).unwrap());
    }


    #[test]
    fn test_in_eval() {
        let logline = LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            props: vec![("key".to_string(), "value".to_string())],
            msg: "Hello, world!".to_string()
        };
        
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("level".to_string()))),
            operator: Operator::In,
            right: Box::new(Expr::Value(Value::List(vec![
                Value::String("info".to_string()),
                Value::String("debug".to_string())
            ])))
        });
        assert!(check_expr(&expr, &logline).unwrap());

        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("level".to_string()))),
            operator: Operator::In,
            right: Box::new(Expr::Value(Value::List(vec![
                Value::String("error".to_string()),
                Value::String("warn".to_string())
            ])))
        });
        assert!(!check_expr(&expr, &logline).unwrap());
    }

    #[test]
    fn test_exsits() {
        let logline = LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            props: vec![("key".to_string(), "value".to_string())],
            msg: "Hello, world!".to_string()
        };
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("key".to_string()))),
            operator: Operator::Exists,
            right: Box::new(Expr::Empty)
        });
        assert!(check_expr(&expr, &logline).unwrap());
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("nonexistent".to_string()))),
            operator: Operator::Exists,
            right: Box::new(Expr::Empty)
        });
        assert!(!check_expr(&expr, &logline).unwrap());
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("nonexistent".to_string()))),
            operator: Operator::NotExists,
            right: Box::new(Expr::Empty)
        });
        assert!(check_expr(&expr, &logline).unwrap());
        let expr = Expr::Condition(Condition {
            left: Box::new(Expr::Value(Value::String("key".to_string()))),
            operator: Operator::NotExists,
            right: Box::new(Expr::Empty)
        });
        assert!(!check_expr(&expr, &logline).unwrap());
    }
}