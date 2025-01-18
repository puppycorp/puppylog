use chrono::{DateTime, NaiveDate, Utc};
use puppylog::LogEntry;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Equal,
    Like,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Value {
    Date(DateTime<Utc>),
    String(String),
    Number(i64),
}

// impl Ord for Value {
//     fn cmp(&self, other: &Self) -> Ordering {
//         // You can customize the comparison logic here
//         self.value.cmp(&other.value)
//     }
// }

// #[derive(Debug, Clone, PartialEq)]
// pub enum LogLevel {
//     Info,
//     Warning,
//     Error,
//     Debug,
// }

#[derive(Debug, PartialEq)]
pub struct Condition {
    pub left: Box<Expr>,
    pub operator: Operator,
    pub right: Box<Expr>,
}

#[derive(Debug, PartialEq)]
pub enum Expr {
    Condition(Condition),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Value(Value),
    Empty,
}

impl Default for Expr {
    fn default() -> Self {
        Expr::Empty
    }
}

#[derive(Debug, PartialEq)]
pub enum OrderDir {
    Asc,
    Desc,
}

#[derive(Debug, PartialEq)]
pub struct OrderBy {
    fields: Vec<String>,
    direction: OrderDir,
}

#[derive(Debug, PartialEq, Default)]
pub struct QueryAst {
    pub root: Expr,
    pub order_by: Option<OrderBy>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl QueryAst {
    pub fn matches(&self, entry: &LogEntry) -> bool {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    OpenParen,
    CloseParen,
    And,
    Or,
    Field(String),
    Operator(Operator),
    Value(Value),
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    
    while let Some(&c) = chars.peek() {
        match c {
            '(' => {
                tokens.push(Token::OpenParen);
                chars.next();
            },
            ')' => {
                tokens.push(Token::CloseParen);
                chars.next();
            },
            ' ' | '\t' | '\n' => {
                chars.next();
            },
            '"' => {
                chars.next(); // consume opening quote
                let mut value = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '"' {
                        chars.next();
                        break;
                    }
                    value.push(chars.next().unwrap());
                }
                tokens.push(Token::Value(Value::String(value)));
            },
            _ => {
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '(' || c == ')' {
                        break;
                    }
                    word.push(chars.next().unwrap());
                }
                
                match word.as_str() {
                    "and" => tokens.push(Token::And),
                    "or" => tokens.push(Token::Or),
                    ">" => tokens.push(Token::Operator(Operator::GreaterThan)),
                    "<" => tokens.push(Token::Operator(Operator::LessThan)),
                    ">=" => tokens.push(Token::Operator(Operator::GreaterThanOrEqual)),
                    "<=" => tokens.push(Token::Operator(Operator::LessThanOrEqual)),
                    "=" => tokens.push(Token::Operator(Operator::Equal)),
                    "like" => tokens.push(Token::Operator(Operator::Like)),
                    _ => {
						let value = if let Ok(date) = NaiveDate::parse_from_str(&word, "%d.%m.%Y") {
							Value::Date(DateTime::<Utc>::from_naive_utc_and_offset(date.into(), Utc))
						} else if let Ok(num) = word.parse::<i64>() {
							Value::Number(num)
						} else {
							Value::String(word)
						};
						tokens.push(Token::Value(value));
                    }
                }
            }
        }
    }
    Ok(tokens)
}

fn parse_tokens(tokens: &[Token]) -> Result<Expr, String> {
    fn parse_condition(tokens: &[Token], start: usize) -> Result<(Expr, usize), String> {
        if tokens.len() - start < 3 {
			log::info!("tokens: {:?} start: {}", tokens, start);
            return Err("Condition requires 3 tokens format: FIELD OPERATOR VALUE".to_string()); 
        }
        
        match (&tokens[start], &tokens[start + 1], &tokens[start + 2]) {
            (Token::Value(left), Token::Operator(op), Token::Value(right)) => {
                Ok((Expr::Condition(Condition {
                    left: Box::new(Expr::Value(left.clone())),
                    operator: op.clone(),
                    right: Box::new(Expr::Value(right.clone())),
                }), start + 3))
            },
            (Token::Value(left), Token::Operator(op), Token::OpenParen) => {
                let (expr, pos) = parse_expression(tokens, start + 3)?;
                if pos >= tokens.len() || tokens[pos] != Token::CloseParen {
                    return Err("Missing closing parenthesis".to_string());
                }
                Ok((Expr::Condition(Condition {
                    left: Box::new(Expr::Value(left.clone())),
                    operator: op.clone(),
                    right: Box::new(expr),
                }), pos + 1))
            },
            (Token::OpenParen, Token::Operator(op), Token::Value(right)) => {
                let (expr, pos) = parse_expression(tokens, start + 1)?;
                if pos >= tokens.len() || tokens[pos] != Token::CloseParen {
                    return Err("Missing closing parenthesis".to_string());
                }
                Ok((Expr::Condition(Condition {
                    left: Box::new(expr),
                    operator: op.clone(),
                    right: Box::new(Expr::Value(right.clone())),
                }), pos + 1))
            },
            _ => {
				Err(format!("Could not parse condition: {:?} {:?} {:?}", tokens[start], tokens[start + 1], tokens[start + 2]))
			},
        }
    }

    fn parse_expression(tokens: &[Token], start: usize) -> Result<(Expr, usize), String> {
        if start >= tokens.len() {
            return Err("Unexpected end of input".to_string());
        }

        let (mut left, mut pos) = match &tokens[start] {
            Token::OpenParen => {
                let (expr, next_pos) = parse_expression(tokens, start + 1)?;
                if next_pos >= tokens.len() || tokens[next_pos] != Token::CloseParen {
                    return Err("Missing closing parenthesis".to_string());
                }
                (expr, next_pos + 1)
            },
            _ => parse_condition(tokens, start)?,
        };

        while pos < tokens.len() {
            match &tokens[pos] {
                Token::And => {
                    let (right, next_pos) = parse_expression(tokens, pos + 1)?;
                    left = Expr::And(Box::new(left), Box::new(right));
                    pos = next_pos;
                },
                Token::Or => {
                    let (right, next_pos) = parse_expression(tokens, pos + 1)?;
                    left = Expr::Or(Box::new(left), Box::new(right));
                    pos = next_pos;
                },
                Token::CloseParen => break,
                _ => return Err("Expected AND or OR operator".to_string()),
            }
        }

        Ok((left, pos))
    }

    let (expr, pos) = parse_expression(tokens, 0)?;
    if pos < tokens.len() {
        return Err("Unexpected tokens after expression".to_string());
    }
    Ok(expr)
}

pub fn parse_log_query(src: &str) -> Result<QueryAst, String> {
    let tokens = tokenize(src)?;
    let root = parse_tokens(&tokens)?;
    Ok(QueryAst { root, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn datetime(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        DateTime::<Utc>::from_utc(NaiveDate::from_ymd(year, month, day).and_hms(0, 0, 0), Utc)
    }

    #[test]
    fn test_simple_query() {
        let query = r#"start >= 01.10.2024 and end <= 12.12.2024s"#;
        let ast = parse_log_query(query).unwrap();
        
        match ast.root {
            Expr::And(left, right) => {
                match *left {
                    Expr::Condition(c) => {
                        assert_eq!(c.left, Box::new(Expr::Value(Value::String("start".to_string()))));
                        assert_eq!(c.operator, Operator::GreaterThanOrEqual);
						assert_eq!(c.right, Box::new(Expr::Value(Value::Date(datetime(2024, 10, 1)))));
                    },
                    _ => panic!("Expected Condition"),
                }
                match *right {
                    Expr::Condition(c) => {
                        assert_eq!(c.left, Box::new(Expr::Value(Value::String("end".to_string()))));
                        assert_eq!(c.operator, Operator::LessThanOrEqual);
						assert_eq!(c.right, Box::new(Expr::Value(Value::Date(datetime(2024, 12, 12)))));
                    },
                    _ => panic!("Expected Condition"),
                }
            },
            _ => panic!("Expected And expression"),
        }
    }

    #[test]
    fn test_complex_query() {
        let query = r#"(start >= 01.10.2024 and end <= 12.12.2024) or (level = info and msg like "error")"#;
        let ast = parse_log_query(query).unwrap();
        
        match ast.root {
            Expr::Or(left, right) => {
                match *left {
                    Expr::And(left, right) => {
                        match *left {
                            Expr::Condition(c) => {
                                assert_eq!(c.left, Box::new(Expr::Value(Value::String("start".to_string()))));
                                assert_eq!(c.operator, Operator::GreaterThanOrEqual);
                                assert_eq!(c.right, Box::new(Expr::Value(Value::Date(datetime(2024, 10, 1)))));
                            },
                            _ => panic!("Expected Condition"),
                        }
                        match *right {
                            Expr::Condition(c) => {
                                assert_eq!(c.left, Box::new(Expr::Value(Value::String("end".to_string()))));
                                assert_eq!(c.operator, Operator::LessThanOrEqual);
                                assert_eq!(c.right, Box::new(Expr::Value(Value::Date(datetime(2024, 12, 12)))));
                            },
                            _ => panic!("Expected Condition"),
                        }
                    },
                    _ => panic!("Expected And expression"),
                }
                match *right {
                    Expr::And(left, right) => {
                        match *left {
                            Expr::Condition(c) => {
                                assert_eq!(c.left, Box::new(Expr::Value(Value::String("level".to_string()))));
                                assert_eq!(c.operator, Operator::Equal);
                                assert_eq!(c.right, Box::new(Expr::Value(Value::String("info".to_string()))));
                            },
                            _ => panic!("Expected Condition"),
                        }
                        match *right {
                            Expr::Condition(c) => {
                                assert_eq!(c.left, Box::new(Expr::Value(Value::String("msg".to_string()))));
                                assert_eq!(c.operator, Operator::Like);
                                assert_eq!(c.right, Box::new(Expr::Value(Value::String("error".to_string()))));
                            },
                            _ => panic!("Expected Condition"),
                        }
                    },
                    _ => panic!("Expected And expression"),
                }
            },
            _ => panic!("Expected Or expression"),
        }
    }
    #[test]
    fn test_right_side_nested_parentheses() {
        let query = r#"(start >= 01.10.2024 and (level = info or level = error)) and msg like "test""#;
        let ast = parse_log_query(query).unwrap();

        match ast.root {
            Expr::And(ref left, ref right) => {
                // Left side: And(Condition(...), Or(...))
                match **left {
                    Expr::And(ref left_inner, ref right_inner) => {
                        // left_inner is Condition for start >= 01.10.2024
                        match **left_inner {
                            Expr::Condition(ref c) => {
                                assert_eq!(
                                    *c.left,
                                    Expr::Value(Value::String("start".to_string()))
                                );
                                assert_eq!(c.operator, Operator::GreaterThanOrEqual);
                                assert_eq!(*c.right, Expr::Value(Value::Date(datetime(2024, 10, 1))));
                            },
                            _ => panic!("Expected Condition"),
                        }
                        // right_inner is Or(...)
                        match **right_inner {
                            Expr::Or(ref left_or, ref right_or) => {
                                // left_or is Condition(level = info)
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
                                    },
                                    _ => panic!("Expected Condition"),
                                }
                                // right_or is Condition(level = error)
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
                                    },
                                    _ => panic!("Expected Condition"),
                                }
                            },
                            _ => panic!("Expected Or"),
                        }
                    },
                    _ => panic!("Expected And"),
                }

                // Right side: Condition(msg like "test")
                match **right {
                    Expr::Condition(ref c) => {
                        assert_eq!(*c.left, Expr::Value(Value::String("msg".to_string())));
                        assert_eq!(c.operator, Operator::Like);
                        assert_eq!(*c.right, Expr::Value(Value::String("test".to_string())));
                    },
                    _ => panic!("Expected Condition"),
                }
            },
            _ => panic!("Expected top-level And"),
        }
    }

    #[test]
    fn test_left_side_nested_parentheses() {
        let query = r#"((level = info or level = error) and start >= 01.10.2024) and msg like "test""#;
        let ast = parse_log_query(query).unwrap();

        match ast.root {
            Expr::And(ref left, ref right) => {
                // Left side: And(Condition(...), Or(...))
                match **left {
                    Expr::And(ref left_inner, ref right_inner) => {
                        // left_inner is Condition for start >= 01.10.2024
                        match **left_inner {
                            Expr::Or(ref left_or, ref right_or) => {
                                // left_or is Condition(level = info)
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
                                    },
                                    _ => panic!("Expected Condition"),
                                }
                                // right_or is Condition(level = error)
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
                                    },
                                    _ => panic!("Expected Condition"),
                                }
                            },
                            _ => panic!("Expected Or"),
                        }
                        // right_inner is Or(...)
                        match **right_inner {
                            Expr::Condition(ref c) => {
                                assert_eq!(
                                    *c.left,
                                    Expr::Value(Value::String("start".to_string()))
                                );
                                assert_eq!(c.operator, Operator::GreaterThanOrEqual);
                                assert_eq!(*c.right, Expr::Value(Value::Date(datetime(2024, 10, 1))));
                            },
                            _ => panic!("Expected Condition"),
                        }
                    },
                    _ => panic!("Expected And"),
                }

                // Right side: Condition(msg like "test")
                match **right {
                    Expr::Condition(ref c) => {
                        assert_eq!(*c.left, Expr::Value(Value::String("msg".to_string())));
                        assert_eq!(c.operator, Operator::Like);
                        assert_eq!(*c.right, Expr::Value(Value::String("test".to_string())));
                    },
                    _ => panic!("Expected Condition"),
                }
            },
            _ => panic!("Expected top-level And"),
        }
    }

    #[test]
    fn test_or_with_two_nested_parantheses() {
        let query = r#"(level = info and msg like "test") or (level = error and msg like "error")"#;
        let ast = parse_log_query(query).unwrap();

        match ast.root {
            Expr::Or(ref left, ref right) => {
                // Left side: And(Condition(...), Condition(...))
                match **left {
                    Expr::And(ref left_inner, ref right_inner) => {
                        // left_inner is Condition(level = info)
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
                            },
                            _ => panic!("Expected Condition"),
                        }
                        // right_inner is Condition(msg like "test")
                        match **right_inner {
                            Expr::Condition(ref c) => {
                                assert_eq!(
                                    *c.left,
                                    Expr::Value(Value::String("msg".to_string()))
                                );
                                assert_eq!(c.operator, Operator::Like);
                                assert_eq!(
                                    *c.right,
                                    Expr::Value(Value::String("test".to_string()))
                                );
                            },
                            _ => panic!("Expected Condition"),
                        }
                    },
                    _ => panic!("Expected And"),
                }

                // Right side: And(Condition(...), Condition(...))
                match **right {
                    Expr::And(ref left_inner, ref right_inner) => {
                        // left_inner is Condition(level = error)
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
                            },
                            _ => panic!("Expected Condition"),
                        }
                        // right_inner is Condition(msg like "error")
                        match **right_inner {
                            Expr::Condition(ref c) => {
                                assert_eq!(
                                    *c.left,
                                    Expr::Value(Value::String("msg".to_string()))
                                );
                                assert_eq!(c.operator, Operator::Like);
                                assert_eq!(
                                    *c.right,
                                    Expr::Value(Value::String("error".to_string()))
                                );
                            },
                            _ => panic!("Expected Condition"),
                        }
                    },
                    _ => panic!("Expected And"),
                }
            },
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
}