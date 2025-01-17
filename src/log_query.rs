use chrono::NaiveDate;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    GreaterThanOrEqual,
    LessThanOrEqual,
    Equal,
    Like,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Date(NaiveDate),
    String(String),
    Number(i64),
    Level(LogLevel),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
    Debug,
}

#[derive(Debug, PartialEq)]
pub struct Condition {
    field: String,
    operator: Operator,
    value: Value,
}

#[derive(Debug, PartialEq)]
pub enum Expression {
    Condition(Condition),
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
}

#[derive(Debug, PartialEq)]
pub struct Query {
    root: Expression,
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "info" => Ok(LogLevel::Info),
            "warning" => Ok(LogLevel::Warning),
            "error" => Ok(LogLevel::Error),
            "debug" => Ok(LogLevel::Debug),
            _ => Err(format!("Invalid log level: {}", s)),
        }
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
                    ">=" => tokens.push(Token::Operator(Operator::GreaterThanOrEqual)),
                    "<=" => tokens.push(Token::Operator(Operator::LessThanOrEqual)),
                    "=" => tokens.push(Token::Operator(Operator::Equal)),
                    "like" => tokens.push(Token::Operator(Operator::Like)),
                    _ => {
                        // Try to parse as a field or value
                        if tokens.last().map_or(true, |t| matches!(t, Token::OpenParen | Token::And | Token::Or)) {
                            tokens.push(Token::Field(word));
                        } else {
                            // Parse value based on the field type
                            if let Some(Token::Field(field)) = tokens.iter().last().cloned() {
                                let value = match field.as_str() {
                                    "start" | "end" => {
                                        let date = NaiveDate::parse_from_str(&word, "%d.%m.%Y")
                                            .map_err(|e| format!("Invalid date format: {}", e))?;
                                        Value::Date(date)
                                    },
                                    "level" => {
                                        let level = LogLevel::from_str(&word)?;
                                        Value::Level(level)
                                    },
                                    "msg" => Value::String(word),
                                    _ => Value::String(word)
                                };
                                tokens.push(Token::Value(value));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(tokens)
}

fn parse_tokens(tokens: &[Token]) -> Result<Expression, String> {
    fn parse_condition(tokens: &[Token], start: usize) -> Result<(Expression, usize), String> {
        if tokens.len() - start < 3 {
            return Err("Invalid condition format".to_string());
        }
        
        match (&tokens[start], &tokens[start + 1], &tokens[start + 2]) {
            (Token::Field(field), Token::Operator(op), Token::Value(val)) => {
                Ok((Expression::Condition(Condition {
                    field: field.clone(),
                    operator: op.clone(),
                    value: val.clone(),
                }), start + 3))
            },
            _ => Err("Invalid condition format".to_string()),
        }
    }

    fn parse_expression(tokens: &[Token], start: usize) -> Result<(Expression, usize), String> {
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
                    left = Expression::And(Box::new(left), Box::new(right));
                    pos = next_pos;
                },
                Token::Or => {
                    let (right, next_pos) = parse_expression(tokens, pos + 1)?;
                    left = Expression::Or(Box::new(left), Box::new(right));
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

pub fn parse_log_query(src: &str) -> Result<Query, String> {
    let tokens = tokenize(src)?;
    let root = parse_tokens(&tokens)?;
    Ok(Query { root })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_query() {
        let query = r#"start>=01.10.2024 and end<=12.12.2024"#;
        let ast = parse_log_query(query).unwrap();
        
        match ast.root {
            Expression::And(left, right) => {
                match *left {
                    Expression::Condition(c) => {
                        assert_eq!(c.field, "start");
                        assert_eq!(c.operator, Operator::GreaterThanOrEqual);
                    },
                    _ => panic!("Expected Condition"),
                }
                match *right {
                    Expression::Condition(c) => {
                        assert_eq!(c.field, "end");
                        assert_eq!(c.operator, Operator::LessThanOrEqual);
                    },
                    _ => panic!("Expected Condition"),
                }
            },
            _ => panic!("Expected And expression"),
        }
    }

    #[test]
    fn test_complex_query() {
        let query = r#"(start>=01.10.2024 and end<=12.12.2024) or (level=info and msg like "error")"#;
        let ast = parse_log_query(query).unwrap();
        
        match ast.root {
            Expression::Or(left, right) => {
                match *left {
                    Expression::And(_, _) => (),
                    _ => panic!("Expected And expression"),
                }
                match *right {
                    Expression::And(_, _) => (),
                    _ => panic!("Expected And expression"),
                }
            },
            _ => panic!("Expected Or expression"),
        }
    }

    #[test]
    fn test_nested_parentheses() {
        let query = r#"(start>=01.10.2024 and (level=info or level=error)) and msg like "test""#;
        assert!(parse_log_query(query).is_ok());
    }

    #[test]
    fn test_invalid_parentheses() {
        let query = r#"(start>=01.10.2024"#;
        assert!(parse_log_query(query).is_err());
    }

    #[test]
    fn test_invalid_operator_combination() {
        let query = r#"start>=01.10.2024 or or end<=12.12.2024"#;
        assert!(parse_log_query(query).is_err());
    }
}