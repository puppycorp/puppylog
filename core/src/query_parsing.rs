use crate::LogEntry;
use chrono::{DateTime, NaiveDate, Utc};
use serde::de::value;
use std::str::FromStr;

use crate::query_eval::check_expr;

#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
	GreaterThan,
	GreaterThanOrEqual,
	LessThan,
	LessThanOrEqual,
	Equal,
	NotEqual,
	Like,
	NotLike,
	In,
	NotIn,
	Exists,
	NotExists,
	Matches,
	NotMatches,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Value {
	Date(DateTime<Utc>),
	String(String),
	Regex(String),
	Number(i64),
	List(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
	pub left: Box<Expr>,
	pub operator: Operator,
	pub right: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldAccess {
	pub expr: Box<Expr>,
	pub field: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
	Condition(Condition),
	And(Box<Expr>, Box<Expr>),
	Or(Box<Expr>, Box<Expr>),
	FieldAccess(FieldAccess),
	Value(Value),
	Empty,
}

impl Default for Expr {
	fn default() -> Self {
		Expr::Empty
	}
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderDir {
	Asc,
	Desc,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderBy {
	fields: Vec<String>,
	direction: OrderDir,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct QueryAst {
	pub root: Expr,
	pub order_by: Option<OrderBy>,
	pub limit: Option<usize>,
	pub offset: Option<usize>,
	pub end_date: Option<DateTime<Utc>>,
}

impl QueryAst {
	pub fn matches(&self, entry: &LogEntry) -> Result<bool, String> {
		check_expr(&self.root, entry)
	}
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
	OpenParen,
	CloseParen,
	And,
	Or,
	Dot,
	Field(String),
	Operator(Operator),
	Value(Value),
	Comma,
}

/// Tokenize the input string into a sequence of `Token`s.
fn tokenize(input: &str) -> Result<Vec<Token>, String> {
	let mut tokens = Vec::new();
	let mut chars = input.chars().peekable();

	while let Some(&c) = chars.peek() {
		match c {
			'(' => {
				tokens.push(Token::OpenParen);
				chars.next();
			}
			')' => {
				tokens.push(Token::CloseParen);
				chars.next();
			}
			'.' => {
				tokens.push(Token::Dot);
				chars.next();
			}
			' ' | '\t' | '\n' => {
				chars.next();
			}
			'\"' => {
				chars.next(); // consume opening quote
				let mut value = String::new();
				while let Some(c) = chars.next() {
					match c {
						'\\' => {
							if let Some(next_c) = chars.next() {
								match next_c {
									'\\' => value.push('\\'),
									'"' => value.push('"'),
									'n' => value.push('\n'),
									't' => value.push('\t'),
									'r' => value.push('\r'),
									other => {
										value.push('\\');
										value.push(other);
									}
								}
							} else {
								value.push('\\');
								break;
							}
						}
						'"' => break,
						other => value.push(other),
					}
				}
				tokens.push(Token::Value(Value::String(value)));
			}
			'/' => {
				chars.next(); // consume opening slash
				let mut pattern = String::new();
				while let Some(c) = chars.next() {
					if c == '/' {
						break;
					} else {
						pattern.push(c);
					}
				}
				tokens.push(Token::Value(Value::Regex(pattern)));
			}
			_ => {
				let mut word = String::new();
				while let Some(&c) = chars.peek() {
					// Break on whitespace or parentheses or dot
					if c.is_whitespace() || c == '(' || c == ')' || c == '.' {
						break;
					}
					word.push(chars.next().unwrap());
				}

				match word.as_str() {
					"," => tokens.push(Token::Comma),
					"and" => tokens.push(Token::And),
					"or" => tokens.push(Token::Or),
					"&&" => tokens.push(Token::And),
					"||" => tokens.push(Token::Or),
					">" => tokens.push(Token::Operator(Operator::GreaterThan)),
					"<" => tokens.push(Token::Operator(Operator::LessThan)),
					">=" => tokens.push(Token::Operator(Operator::GreaterThanOrEqual)),
					"<=" => tokens.push(Token::Operator(Operator::LessThanOrEqual)),
					"=" => tokens.push(Token::Operator(Operator::Equal)),
					"!=" => tokens.push(Token::Operator(Operator::NotEqual)),
					"like" => tokens.push(Token::Operator(Operator::Like)),
					"in" => tokens.push(Token::Operator(Operator::In)),
					"exists" => tokens.push(Token::Operator(Operator::Exists)),
					"matches" => tokens.push(Token::Operator(Operator::Matches)),
					"not" => {
						// could be not like / not in / not exists / not matches
						chars.next(); // consume the whitespace after "not"
						let mut next_word = String::new();
						while let Some(&c) = chars.peek() {
							if c.is_whitespace() || c == '(' || c == ')' || c == '.' {
								break;
							}
							next_word.push(chars.next().unwrap());
						}
						match next_word.as_str() {
							"like" => tokens.push(Token::Operator(Operator::NotLike)),
							"in" => tokens.push(Token::Operator(Operator::NotIn)),
							"exists" => tokens.push(Token::Operator(Operator::NotExists)),
							"matches" => tokens.push(Token::Operator(Operator::NotMatches)),
							other => {
								return Err(format!("Unexpected token after 'not': {}", other))
							}
						}
					}
					_ => {
						// Attempt to parse as date (dd.mm.yyyy), then number, else string
						if let Ok(date) = NaiveDate::parse_from_str(&word, "%d.%m.%Y") {
							tokens.push(Token::Value(Value::Date(DateTime::<Utc>::from_utc(
								date.and_hms_opt(0, 0, 0).unwrap(),
								Utc,
							))));
						} else if let Ok(num) = word.parse::<i64>() {
							tokens.push(Token::Value(Value::Number(num)));
						} else {
							tokens.push(Token::Value(Value::String(word)));
						}
					}
				}
			}
		}
	}
	Ok(tokens)
}

/// Parse a possible chain of field accesses (e.g. `timestamp.hour`).
/// If there's just a single token, it remains a `Value` expression.
/// If there's a chain of dots, build up `FieldAccess` nodes.
fn parse_field_chain(tokens: &[Token], start: usize) -> Result<(Expr, usize), String> {
	if start >= tokens.len() {
		return Err("No tokens to parse for field/value".into());
	}

	let (mut expr, mut pos) = match &tokens[start] {
		Token::OpenParen => {
			// parse sub-expression in parentheses
			let (subexpr, next_pos) = parse_expression(tokens, start + 1)?;
			if next_pos >= tokens.len() {
				return Err("Missing closing parenthesis".into());
			}
			if tokens[next_pos] != Token::CloseParen {
				return Err("Expected ')'".into());
			}
			(subexpr, next_pos + 1)
		}
		Token::Value(val) => (Expr::Value(val.clone()), start + 1),
		other => {
			return Err(format!(
				"Unexpected token {:?} while expecting value or '('",
				other
			));
		}
	};

	// Possibly parse .field .another etc
	while pos < tokens.len() {
		if let Token::Dot = tokens[pos] {
			let next_pos = pos + 1;
			if next_pos >= tokens.len() {
				return Err("Expected field name after '.'".into());
			}
			match &tokens[next_pos] {
				Token::Value(Value::String(field_name)) => {
					expr = Expr::FieldAccess(FieldAccess {
						expr: Box::new(expr),
						field: field_name.clone(),
					});
					pos = next_pos + 1;
				}
				other => {
					return Err(format!(
						"Expected identifier after '.', but found: {:?}",
						other
					));
				}
			}
		} else {
			break;
		}
	}

	Ok((expr, pos))
}

/// Parse a condition of the form `<expr> <operator> <expr>`.
/// Handles special cases like `field EXISTS` or `field IN ( ... )`.
fn parse_condition(tokens: &[Token], start: usize) -> Result<(Expr, usize), String> {
	let len = tokens.len();
	if start >= len {
		return Err("No tokens left for condition".into());
	}

	// Check for `<expr> EXISTS` / `<expr> NOT EXISTS`
	if len - start >= 2 {
		if let (
			Token::Value(left_val),
			Token::Operator(ref op @ (Operator::Exists | Operator::NotExists)),
		) = (&tokens[start], &tokens[start + 1])
		{
			return Ok((
				Expr::Condition(Condition {
					left: Box::new(Expr::Value(left_val.clone())),
					operator: op.clone(),
					right: Box::new(Expr::Empty),
				}),
				start + 2,
			));
		}
	}

	// For a normal condition, parse the left side (potentially a field chain).
	let (left_expr, mut pos) = parse_field_chain(tokens, start)?;
	// If the next token is a boolean operator or we have reached the end, and the left_expr is a bare string,
	// wrap it as a default text search on the "msg" field.
	if pos >= tokens.len() || matches!(tokens[pos], Token::And | Token::Or | Token::CloseParen) {
		if let Expr::Value(Value::String(text)) = left_expr {
			return Ok((
				Expr::Condition(Condition {
					left: Box::new(Expr::Value(Value::String("msg".to_string()))),
					operator: Operator::Like,
					right: Box::new(Expr::Value(Value::String(text))),
				}),
				pos,
			));
		}
	}
	if pos >= len {
		return Err("Missing operator".into());
	}

	let op_token = &tokens[pos];
	pos += 1;

	match op_token {
		Token::Operator(op) => {
			let operator = op.clone();
			// handle <expr> IN (...) or <expr> NOT IN (...)
			if operator == Operator::In || operator == Operator::NotIn {
				if pos >= len {
					return Err("Expected '(' after IN".into());
				}
				if tokens[pos] != Token::OpenParen {
					return Err("Expected '(' after IN".into());
				}
				pos += 1; // consume '('
				let mut values = Vec::new();
				while pos < len {
					match &tokens[pos] {
						Token::Value(v) => {
							values.push(v.clone());
							pos += 1;
						}
						Token::Comma => {
							pos += 1;
						}
						Token::CloseParen => {
							// end of list
							pos += 1; // consume ')'
							break;
						}
						other => {
							return Err(format!("Unexpected token in IN list: {:?}", other));
						}
					}
				}
				Ok((
					Expr::Condition(Condition {
						left: Box::new(left_expr),
						operator,
						right: Box::new(Expr::Value(Value::List(values))),
					}),
					pos,
				))
			} else {
				// parse the right side (potential field chain)
				let (right_expr, new_pos) = parse_field_chain(tokens, pos)?;
				Ok((
					Expr::Condition(Condition {
						left: Box::new(left_expr),
						operator,
						right: Box::new(right_expr),
					}),
					new_pos,
				))
			}
		}
		other => Err(format!("Expected operator, found {:?}", other)),
	}
}

/// Parse an expression, which can consist of conditions combined with AND/OR,
/// or a parenthesized sub-expression.
fn parse_expression(tokens: &[Token], start: usize) -> Result<(Expr, usize), String> {
	let len = tokens.len();
	if start >= len {
		return Err("Unexpected end of tokens".into());
	}

	let (mut left_expr, mut pos) = match &tokens[start] {
		Token::OpenParen => {
			let (expr, new_pos) = parse_expression(tokens, start + 1)?;
			if new_pos >= len || tokens[new_pos] != Token::CloseParen {
				return Err("Missing closing parenthesis".into());
			}
			(expr, new_pos + 1)
		}
		_ => parse_condition(tokens, start)?,
	};

	while pos < len {
		match &tokens[pos] {
			Token::And => {
				pos += 1;
				let (right_expr, new_pos) = parse_expression(tokens, pos)?;
				left_expr = Expr::And(Box::new(left_expr), Box::new(right_expr));
				pos = new_pos;
			}
			Token::Or => {
				pos += 1;
				let (right_expr, new_pos) = parse_expression(tokens, pos)?;
				left_expr = Expr::Or(Box::new(left_expr), Box::new(right_expr));
				pos = new_pos;
			}
			Token::CloseParen => break,
			Token::Dot => break,
			_ => break,
		}
	}

	Ok((left_expr, pos))
}

fn parse_tokens(tokens: &[Token]) -> Result<Expr, String> {
	let (expr, pos) = parse_expression(tokens, 0)?;
	if pos < tokens.len() {
		return Err("Unexpected tokens after expression".into());
	}
	Ok(expr)
}

pub fn parse_log_query(src: &str) -> Result<QueryAst, String> {
	let tokens = tokenize(src)?;
	let root = parse_tokens(&tokens)?;
	Ok(QueryAst {
		root,
		..Default::default()
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn datetime(year: i32, month: u32, day: u32) -> DateTime<Utc> {
		DateTime::<Utc>::from_utc(NaiveDate::from_ymd(year, month, day).and_hms(0, 0, 0), Utc)
	}

	#[test]
	fn test_simple_query() {
		let query = r#"(timestamp.year >= 2024 and timestamp.year <= 2025)"#;
		let ast = parse_log_query(query).unwrap();

		match ast.root {
			Expr::And(left, right) => {
				match *left {
					Expr::Condition(c) => {
						assert_eq!(
							c.left,
							Box::new(Expr::FieldAccess(FieldAccess {
								expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
								field: "year".to_string(),
							}))
						);
						assert_eq!(c.operator, Operator::GreaterThanOrEqual);
						assert_eq!(c.right, Box::new(Expr::Value(Value::Number(2024))));
					}
					_ => panic!("Expected Condition"),
				}
				match *right {
					Expr::Condition(c) => {
						assert_eq!(
							c.left,
							Box::new(Expr::FieldAccess(FieldAccess {
								expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
								field: "year".to_string(),
							}))
						);
						assert_eq!(c.operator, Operator::LessThanOrEqual);
						assert_eq!(c.right, Box::new(Expr::Value(Value::Number(2025))));
					}
					_ => panic!("Expected Condition"),
				}
			}
			_ => panic!("Expected And expression"),
		}
	}

	#[test]
	fn test_complex_query() {
		let query = r#"(timestamp.year >= 2024 and timestamp.year <= 2025) or (level = info and msg like "error")"#;
		let ast = parse_log_query(query).unwrap();

		match ast.root {
			Expr::Or(left, right) => {
				match *left {
					Expr::And(left, right) => {
						match *left {
							Expr::Condition(c) => {
								assert_eq!(
									c.left,
									Box::new(Expr::FieldAccess(FieldAccess {
										expr: Box::new(Expr::Value(Value::String(
											"timestamp".to_string()
										))),
										field: "year".to_string(),
									}))
								);
								assert_eq!(c.operator, Operator::GreaterThanOrEqual);
								assert_eq!(c.right, Box::new(Expr::Value(Value::Number(2024))));
							}
							_ => panic!("Expected Condition"),
						}
						match *right {
							Expr::Condition(c) => {
								assert_eq!(
									c.left,
									Box::new(Expr::FieldAccess(FieldAccess {
										expr: Box::new(Expr::Value(Value::String(
											"timestamp".to_string()
										))),
										field: "year".to_string(),
									}))
								);
								assert_eq!(c.operator, Operator::LessThanOrEqual);
								assert_eq!(c.right, Box::new(Expr::Value(Value::Number(2025))));
							}
							_ => panic!("Expected Condition"),
						}
					}
					_ => panic!("Expected And expression"),
				}
				match *right {
					Expr::And(left, right) => {
						match *left {
							Expr::Condition(c) => {
								assert_eq!(
									c.left,
									Box::new(Expr::Value(Value::String("level".to_string())))
								);
								assert_eq!(c.operator, Operator::Equal);
								assert_eq!(
									c.right,
									Box::new(Expr::Value(Value::String("info".to_string())))
								);
							}
							_ => panic!("Expected Condition"),
						}
						match *right {
							Expr::Condition(c) => {
								assert_eq!(
									c.left,
									Box::new(Expr::Value(Value::String("msg".to_string())))
								);
								assert_eq!(c.operator, Operator::Like);
								assert_eq!(
									c.right,
									Box::new(Expr::Value(Value::String("error".to_string())))
								);
							}
							_ => panic!("Expected Condition"),
						}
					}
					_ => panic!("Expected And expression"),
				}
			}
			_ => panic!("Expected Or expression"),
		}
	}
	#[test]
	fn test_right_side_nested_parentheses() {
		let query =
			r#"(timestamp.year >= 2024 and (level = info or level = error)) and msg like "test""#;
		let ast = parse_log_query(query).unwrap();

		match ast.root {
			Expr::And(ref left, ref right) => {
				match **left {
					Expr::And(ref left_inner, ref right_inner) => {
						match **left_inner {
							Expr::Condition(ref c) => {
								assert_eq!(
									*c.left,
									Expr::FieldAccess(FieldAccess {
										expr: Box::new(Expr::Value(Value::String(
											"timestamp".to_string()
										))),
										field: "year".to_string(),
									})
								);
								assert_eq!(c.operator, Operator::GreaterThanOrEqual);
								assert_eq!(*c.right, Expr::Value(Value::Number(2024)));
							}
							_ => panic!("Expected Condition"),
						}
						match **right_inner {
							Expr::Or(ref left_or, ref right_or) => {
								match **left_or {
									Expr::Condition(ref c) => {
										assert_eq!(
											*c.left,
											Expr::Value(Value::String("level".to_string()))
										);
										assert_eq!(c.operator, Operator::Equal);
										assert_eq!(
											*c.right,
											Expr::Value(Value::String("info".to_string()))
										);
									}
									_ => panic!("Expected Condition"),
								}
								match **right_or {
									Expr::Condition(ref c) => {
										assert_eq!(
											*c.left,
											Expr::Value(Value::String("level".to_string()))
										);
										assert_eq!(c.operator, Operator::Equal);
										assert_eq!(
											*c.right,
											Expr::Value(Value::String("error".to_string()))
										);
									}
									_ => panic!("Expected Condition"),
								}
							}
							_ => panic!("Expected Or"),
						}
					}
					_ => panic!("Expected And"),
				}

				match **right {
					Expr::Condition(ref c) => {
						assert_eq!(*c.left, Expr::Value(Value::String("msg".to_string())));
						assert_eq!(c.operator, Operator::Like);
						assert_eq!(*c.right, Expr::Value(Value::String("test".to_string())));
					}
					_ => panic!("Expected Condition"),
				}
			}
			_ => panic!("Expected top-level And"),
		}
	}

	#[test]
	fn test_left_side_nested_parentheses() {
		let query =
			r#"((level = info or level = error) and timestamp.year >= 2024) and msg like "test""#;
		let ast = parse_log_query(query).unwrap();

		match ast.root {
			Expr::And(ref left, ref right) => {
				match **left {
					Expr::And(ref left_inner, ref right_inner) => {
						match **left_inner {
							Expr::Or(ref left_or, ref right_or) => {
								match **left_or {
									Expr::Condition(ref c) => {
										assert_eq!(
											*c.left,
											Expr::Value(Value::String("level".to_string()))
										);
										assert_eq!(c.operator, Operator::Equal);
										assert_eq!(
											*c.right,
											Expr::Value(Value::String("info".to_string()))
										);
									}
									_ => panic!("Expected Condition"),
								}
								match **right_or {
									Expr::Condition(ref c) => {
										assert_eq!(
											*c.left,
											Expr::Value(Value::String("level".to_string()))
										);
										assert_eq!(c.operator, Operator::Equal);
										assert_eq!(
											*c.right,
											Expr::Value(Value::String("error".to_string()))
										);
									}
									_ => panic!("Expected Condition"),
								}
							}
							_ => panic!("Expected Or"),
						}
						match **right_inner {
							Expr::Condition(ref c) => {
								assert_eq!(
									*c.left,
									Expr::FieldAccess(FieldAccess {
										expr: Box::new(Expr::Value(Value::String(
											"timestamp".to_string()
										))),
										field: "year".to_string(),
									})
								);
								assert_eq!(c.operator, Operator::GreaterThanOrEqual);
								assert_eq!(*c.right, Expr::Value(Value::Number(2024)));
							}
							_ => panic!("Expected Condition"),
						}
					}
					_ => panic!("Expected And"),
				}
				match **right {
					Expr::Condition(ref c) => {
						assert_eq!(*c.left, Expr::Value(Value::String("msg".to_string())));
						assert_eq!(c.operator, Operator::Like);
						assert_eq!(*c.right, Expr::Value(Value::String("test".to_string())));
					}
					_ => panic!("Expected Condition"),
				}
			}
			_ => panic!("Expected top-level And"),
		}
	}

	#[test]
	fn test_or_with_two_nested_parantheses() {
		let query = r#"(level = info and msg like "test") or (level = error and msg like "error")"#;
		let ast = parse_log_query(query).unwrap();

		match ast.root {
			Expr::Or(ref left, ref right) => {
				match **left {
					Expr::And(ref left_inner, ref right_inner) => {
						match **left_inner {
							Expr::Condition(ref c) => {
								assert_eq!(
									*c.left,
									Expr::Value(Value::String("level".to_string()))
								);
								assert_eq!(c.operator, Operator::Equal);
								assert_eq!(
									*c.right,
									Expr::Value(Value::String("info".to_string()))
								);
							}
							_ => panic!("Expected Condition"),
						}
						match **right_inner {
							Expr::Condition(ref c) => {
								assert_eq!(*c.left, Expr::Value(Value::String("msg".to_string())));
								assert_eq!(c.operator, Operator::Like);
								assert_eq!(
									*c.right,
									Expr::Value(Value::String("test".to_string()))
								);
							}
							_ => panic!("Expected Condition"),
						}
					}
					_ => panic!("Expected And"),
				}

				match **right {
					Expr::And(ref left_inner, ref right_inner) => {
						match **left_inner {
							Expr::Condition(ref c) => {
								assert_eq!(
									*c.left,
									Expr::Value(Value::String("level".to_string()))
								);
								assert_eq!(c.operator, Operator::Equal);
								assert_eq!(
									*c.right,
									Expr::Value(Value::String("error".to_string()))
								);
							}
							_ => panic!("Expected Condition"),
						}
						match **right_inner {
							Expr::Condition(ref c) => {
								assert_eq!(*c.left, Expr::Value(Value::String("msg".to_string())));
								assert_eq!(c.operator, Operator::Like);
								assert_eq!(
									*c.right,
									Expr::Value(Value::String("error".to_string()))
								);
							}
							_ => panic!("Expected Condition"),
						}
					}
					_ => panic!("Expected And"),
				}
			}
			_ => panic!("Expected top-level Or"),
		}
	}

	#[test]
	fn test_invalid_parentheses() {
		let query = r#"(start>=01.10.2024"#;
		assert!(parse_log_query(query).is_err());
	}

	#[test]
	fn test_invalid_operator_combination() {
		let query = r#"start >= 01.10.2024 or or end <= 12.12.2024"#;
		assert!(parse_log_query(query).is_err());
	}

	#[test]
	fn parse_equal() {
		let query = r#"level = "info""#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("level".to_string())))
				);
				assert_eq!(c.operator, Operator::Equal);
				assert_eq!(
					c.right,
					Box::new(Expr::Value(Value::String("info".to_string())))
				);
			}
			_ => panic!("Expected Condition"),
		}
		let query = r#"level != "info""#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("level".to_string())))
				);
				assert_eq!(c.operator, Operator::NotEqual);
				assert_eq!(
					c.right,
					Box::new(Expr::Value(Value::String("info".to_string())))
				);
			}
			_ => panic!("Expected Condition"),
		}
	}

	#[test]
	fn parse_like() {
		let query = r#"msg like "error""#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("msg".to_string())))
				);
				assert_eq!(c.operator, Operator::Like);
				assert_eq!(
					c.right,
					Box::new(Expr::Value(Value::String("error".to_string())))
				);
			}
			_ => panic!("Expected Condition"),
		}
		let query = r#"msg not like "error""#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("msg".to_string())))
				);
				assert_eq!(c.operator, Operator::NotLike);
				assert_eq!(
					c.right,
					Box::new(Expr::Value(Value::String("error".to_string())))
				);
			}
			_ => panic!("Expected Condition"),
		}
		let query = r#"msg like "error \"oops\"""#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("msg".to_string())))
				);
				assert_eq!(c.operator, Operator::Like);
				assert_eq!(
					c.right,
					Box::new(Expr::Value(Value::String("error \"oops\"".to_string())))
				);
			}
			_ => panic!("Expected Condition"),
		}
	}

	#[test]
	fn parse_exists() {
		let query = r#"msg exists"#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("msg".to_string())))
				);
				assert_eq!(c.operator, Operator::Exists);
				assert_eq!(c.right, Box::new(Expr::Empty));
			}
			_ => panic!("Expected Condition"),
		}
		let query = r#"msg not exists"#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("msg".to_string())))
				);
				assert_eq!(c.operator, Operator::NotExists);
				assert_eq!(c.right, Box::new(Expr::Empty));
			}
			_ => panic!("Expected Condition"),
		}
	}

	#[test]
	fn parse_matches() {
		let query = r#"deviceId matches /^device-[0-9]+$/"#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("deviceId".to_string())))
				);
				assert_eq!(c.operator, Operator::Matches);
				assert_eq!(
					c.right,
					Box::new(Expr::Value(Value::Regex("^device-[0-9]+$".to_string())))
				);
			}
			_ => panic!("Expected Condition"),
		}
		let query = r#"deviceId not matches /^device-[0-9]+$/"#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("deviceId".to_string())))
				);
				assert_eq!(c.operator, Operator::NotMatches);
				assert_eq!(
					c.right,
					Box::new(Expr::Value(Value::Regex("^device-[0-9]+$".to_string())))
				);
			}
			_ => panic!("Expected Condition"),
		}
	}

	#[test]
	fn parse_in() {
		let query = r#"level in ("info", "error")"#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("level".to_string())))
				);
				assert_eq!(c.operator, Operator::In);
				assert_eq!(
					c.right,
					Box::new(Expr::Value(Value::List(vec![
						Value::String("info".to_string()),
						Value::String("error".to_string()),
					])))
				);
			}
			_ => panic!("Expected Condition"),
		}
		let query = r#"level not in ("info", "error")"#;
		let ast = parse_log_query(query).unwrap();
		match ast.root {
			Expr::Condition(c) => {
				assert_eq!(
					c.left,
					Box::new(Expr::Value(Value::String("level".to_string())))
				);
				assert_eq!(c.operator, Operator::NotIn);
				assert_eq!(
					c.right,
					Box::new(Expr::Value(Value::List(vec![
						Value::String("info".to_string()),
						Value::String("error".to_string()),
					])))
				);
			}
			_ => panic!("Expected Condition"),
		}
	}

	#[test]
	fn parse_field_access() {
		let query = r#"timestamp.hour < 5"#;
		let tokens = tokenize(query).unwrap();
		let ast = parse_log_query(query).unwrap();
		assert_eq!(
			ast.root,
			Expr::Condition(Condition {
				left: Box::new(Expr::FieldAccess(FieldAccess {
					expr: Box::new(Expr::Value(Value::String("timestamp".to_string()))),
					field: "hour".to_string(),
				})),
				operator: Operator::LessThan,
				right: Box::new(Expr::Value(Value::Number(5))),
			})
		);
	}

	#[test]
	fn strings_without_codition_are_treated_as_text_search() {
		let query = r#"error"#;
		let ast = parse_log_query(query).unwrap();
		assert_eq!(
			ast.root,
			Expr::Condition(Condition {
				left: Box::new(Expr::Value(Value::String("msg".to_string()))),
				operator: Operator::Like,
				right: Box::new(Expr::Value(Value::String("error".to_string()))),
			})
		);
	}

	#[test]
	fn test_and_or() {
		let query =
			r#"(level = "info" and msg like "error") || (level = "debug" && msg like "jyrki")"#;
		let ast = parse_log_query(query).unwrap();
		assert_eq!(
			ast.root,
			Expr::Or(
				Box::new(Expr::And(
					Box::new(Expr::Condition(Condition {
						left: Box::new(Expr::Value(Value::String("level".to_string()))),
						operator: Operator::Equal,
						right: Box::new(Expr::Value(Value::String("info".to_string()))),
					})),
					Box::new(Expr::Condition(Condition {
						left: Box::new(Expr::Value(Value::String("msg".to_string()))),
						operator: Operator::Like,
						right: Box::new(Expr::Value(Value::String("error".to_string()))),
					})),
				)),
				Box::new(Expr::And(
					Box::new(Expr::Condition(Condition {
						left: Box::new(Expr::Value(Value::String("level".to_string()))),
						operator: Operator::Equal,
						right: Box::new(Expr::Value(Value::String("debug".to_string()))),
					})),
					Box::new(Expr::Condition(Condition {
						left: Box::new(Expr::Value(Value::String("msg".to_string()))),
						operator: Operator::Like,
						right: Box::new(Expr::Value(Value::String("jyrki".to_string()))),
					})),
				)),
			)
		);
	}

	#[test]
	fn bare_strings_with_parentheses_are_text_searches() {
		let query = r#"("openDoor" or "DoorEvent")"#;
		let ast = parse_log_query(query).unwrap();
		assert_eq!(
			ast.root,
			Expr::Or(
				Box::new(Expr::Condition(Condition {
					left: Box::new(Expr::Value(Value::String("msg".to_string()))),
					operator: Operator::Like,
					right: Box::new(Expr::Value(Value::String("openDoor".to_string()))),
				})),
				Box::new(Expr::Condition(Condition {
					left: Box::new(Expr::Value(Value::String("msg".to_string()))),
					operator: Operator::Like,
					right: Box::new(Expr::Value(Value::String("DoorEvent".to_string()))),
				})),
			)
		);
	}

	#[test]
	fn line_breaks_are_treated_as_whitespace() {
		let query = "level = info\nor level = error";
		let ast = parse_log_query(query).unwrap();
		assert_eq!(
			ast.root,
			Expr::Or(
				Box::new(Expr::Condition(Condition {
					left: Box::new(Expr::Value(Value::String("level".to_string()))),
					operator: Operator::Equal,
					right: Box::new(Expr::Value(Value::String("info".to_string()))),
				})),
				Box::new(Expr::Condition(Condition {
					left: Box::new(Expr::Value(Value::String("level".to_string()))),
					operator: Operator::Equal,
					right: Box::new(Expr::Value(Value::String("error".to_string()))),
				})),
			)
		);
	}
}
