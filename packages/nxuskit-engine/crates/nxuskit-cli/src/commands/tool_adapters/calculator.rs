//! Safe arithmetic calculator tool adapter for tool-loop.
//!
//! Recursive descent parser supporting +, -, *, /, ^, parentheses, unary minus.
//! No eval, no exec.

use crate::cli_error::CliError;

/// Evaluate an arithmetic expression safely.
pub fn evaluate(expr: &str) -> Result<f64, CliError> {
    let tokens = tokenize(expr)?;
    let mut pos = 0;
    let result = parse_expr(&tokens, &mut pos)?;
    if pos != tokens.len() {
        return Err(CliError::ParseError {
            message: format!("Unexpected token at position {pos}"),
        });
    }
    Ok(result)
}

#[derive(Debug, Clone)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Result<Vec<Token>, CliError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' => {
                chars.next();
            }
            '0'..='9' | '.' => {
                let mut num_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == '.' {
                        num_str.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let n: f64 = num_str.parse().map_err(|_| CliError::ParseError {
                    message: format!("Invalid number: {num_str}"),
                })?;
                tokens.push(Token::Number(n));
            }
            '+' => {
                tokens.push(Token::Plus);
                chars.next();
            }
            '-' => {
                tokens.push(Token::Minus);
                chars.next();
            }
            '*' => {
                tokens.push(Token::Star);
                chars.next();
            }
            '/' => {
                tokens.push(Token::Slash);
                chars.next();
            }
            '^' => {
                tokens.push(Token::Caret);
                chars.next();
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            _ => {
                return Err(CliError::ParseError {
                    message: format!("Unexpected character: '{ch}'"),
                });
            }
        }
    }

    Ok(tokens)
}

// expr = term (('+' | '-') term)*
fn parse_expr(tokens: &[Token], pos: &mut usize) -> Result<f64, CliError> {
    let mut left = parse_term(tokens, pos)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Plus => {
                *pos += 1;
                left += parse_term(tokens, pos)?;
            }
            Token::Minus => {
                *pos += 1;
                left -= parse_term(tokens, pos)?;
            }
            _ => break,
        }
    }
    Ok(left)
}

// term = power (('*' | '/') power)*
fn parse_term(tokens: &[Token], pos: &mut usize) -> Result<f64, CliError> {
    let mut left = parse_power(tokens, pos)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Star => {
                *pos += 1;
                left *= parse_power(tokens, pos)?;
            }
            Token::Slash => {
                *pos += 1;
                let right = parse_power(tokens, pos)?;
                if right == 0.0 {
                    return Err(CliError::ParseError {
                        message: "Division by zero".to_string(),
                    });
                }
                left /= right;
            }
            _ => break,
        }
    }
    Ok(left)
}

// power = unary ('^' power)?   (right-associative)
fn parse_power(tokens: &[Token], pos: &mut usize) -> Result<f64, CliError> {
    let base = parse_unary(tokens, pos)?;
    if *pos < tokens.len()
        && let Token::Caret = &tokens[*pos]
    {
        *pos += 1;
        let exp = parse_power(tokens, pos)?;
        return Ok(base.powf(exp));
    }
    Ok(base)
}

// unary = '-' unary | primary
fn parse_unary(tokens: &[Token], pos: &mut usize) -> Result<f64, CliError> {
    if *pos < tokens.len()
        && let Token::Minus = &tokens[*pos]
    {
        *pos += 1;
        let val = parse_unary(tokens, pos)?;
        return Ok(-val);
    }
    parse_primary(tokens, pos)
}

// primary = NUMBER | '(' expr ')'
fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<f64, CliError> {
    if *pos >= tokens.len() {
        return Err(CliError::ParseError {
            message: "Unexpected end of expression".to_string(),
        });
    }

    match &tokens[*pos] {
        Token::Number(n) => {
            let val = *n;
            *pos += 1;
            Ok(val)
        }
        Token::LParen => {
            *pos += 1;
            let val = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() {
                return Err(CliError::ParseError {
                    message: "Missing closing parenthesis".to_string(),
                });
            }
            if let Token::RParen = &tokens[*pos] {
                *pos += 1;
                Ok(val)
            } else {
                Err(CliError::ParseError {
                    message: "Expected closing parenthesis".to_string(),
                })
            }
        }
        _ => Err(CliError::ParseError {
            message: format!("Unexpected token at position {pos}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_arithmetic() {
        assert!((evaluate("2 + 3").unwrap() - 5.0).abs() < 1e-10);
        assert!((evaluate("10 - 4").unwrap() - 6.0).abs() < 1e-10);
        assert!((evaluate("3 * 7").unwrap() - 21.0).abs() < 1e-10);
        assert!((evaluate("15 / 3").unwrap() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn operator_precedence() {
        assert!((evaluate("2 + 3 * 4").unwrap() - 14.0).abs() < 1e-10);
        assert!((evaluate("(2 + 3) * 4").unwrap() - 20.0).abs() < 1e-10);
    }

    #[test]
    fn exponentiation() {
        assert!((evaluate("2 ^ 3").unwrap() - 8.0).abs() < 1e-10);
        assert!((evaluate("2 ^ 3 ^ 2").unwrap() - 512.0).abs() < 1e-10); // right-assoc
    }

    #[test]
    fn unary_minus() {
        assert!((evaluate("-5").unwrap() - (-5.0)).abs() < 1e-10);
        assert!((evaluate("-(2 + 3)").unwrap() - (-5.0)).abs() < 1e-10);
    }

    #[test]
    fn division_by_zero() {
        assert!(evaluate("1 / 0").is_err());
    }
}
