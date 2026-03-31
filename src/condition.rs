use crate::error::ForgeError;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Identifier(String),
    Literal(String),
    Eq,
    Ne,
    And,
    Or,
}

pub fn evaluate_condition(
    expression: &str,
    variables: &BTreeMap<String, String>,
) -> Result<bool, ForgeError> {
    let tokens = tokenize(expression)?;
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        variables,
    };
    let value = parser.parse_or()?;
    if parser.position != tokens.len() {
        return Err(ForgeError::message(
            "unexpected trailing tokens in condition",
        ));
    }
    Ok(value)
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    variables: &'a BTreeMap<String, String>,
}

impl<'a> Parser<'a> {
    fn parse_or(&mut self) -> Result<bool, ForgeError> {
        let mut value = self.parse_and()?;
        while self.match_token(Token::Or) {
            value = value || self.parse_and()?;
        }
        Ok(value)
    }

    fn parse_and(&mut self) -> Result<bool, ForgeError> {
        let mut value = self.parse_comparison()?;
        while self.match_token(Token::And) {
            if value {
                value = self.parse_comparison()?;
            } else {
                // Short-circuit: skip parsing the right-hand side as a
                // real evaluation (variables may not exist yet). Consume
                // the tokens without resolving identifiers.
                self.skip_comparison()?;
            }
        }
        Ok(value)
    }

    fn parse_comparison(&mut self) -> Result<bool, ForgeError> {
        let left = self.parse_operand()?;
        if self.match_token(Token::Eq) {
            let right = self.parse_operand()?;
            return Ok(left == right);
        }
        if self.match_token(Token::Ne) {
            let right = self.parse_operand()?;
            return Ok(left != right);
        }

        match left.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(ForgeError::message("bare values are not valid conditions")),
        }
    }

    fn skip_comparison(&mut self) -> Result<(), ForgeError> {
        // Consume one comparison expression without resolving variables.
        self.skip_operand()?;
        if self.match_token(Token::Eq) || self.match_token(Token::Ne) {
            self.skip_operand()?;
        }
        Ok(())
    }

    fn skip_operand(&mut self) -> Result<(), ForgeError> {
        let token = self
            .tokens
            .get(self.position)
            .cloned()
            .ok_or_else(|| ForgeError::message("unexpected end of condition"))?;
        self.position += 1;
        match token {
            Token::Identifier(_) | Token::Literal(_) => Ok(()),
            _ => Err(ForgeError::message(
                "expected identifier or literal in condition",
            )),
        }
    }

    fn parse_operand(&mut self) -> Result<String, ForgeError> {
        let token = self
            .tokens
            .get(self.position)
            .cloned()
            .ok_or_else(|| ForgeError::message("unexpected end of condition"))?;
        self.position += 1;
        match token {
            Token::Identifier(identifier) => self
                .variables
                .get(&identifier)
                .cloned()
                .ok_or_else(|| ForgeError::message(format!("missing variable `{identifier}`"))),
            Token::Literal(value) => Ok(value),
            _ => Err(ForgeError::message(
                "expected identifier or literal in condition",
            )),
        }
    }

    fn match_token(&mut self, expected: Token) -> bool {
        if self.tokens.get(self.position) == Some(&expected) {
            self.position += 1;
            return true;
        }
        false
    }
}

fn tokenize(input: &str) -> Result<Vec<Token>, ForgeError> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        match chars[index] {
            ' ' | '\t' | '\n' => {
                index += 1;
            }
            '=' => {
                if chars.get(index + 1) == Some(&'=') {
                    tokens.push(Token::Eq);
                    index += 2;
                } else {
                    return Err(ForgeError::message("expected `==` in condition"));
                }
            }
            '!' => {
                if chars.get(index + 1) == Some(&'=') {
                    tokens.push(Token::Ne);
                    index += 2;
                } else {
                    return Err(ForgeError::message("expected `!=` in condition"));
                }
            }
            '&' => {
                if chars.get(index + 1) == Some(&'&') {
                    tokens.push(Token::And);
                    index += 2;
                } else {
                    return Err(ForgeError::message("expected `&&` in condition"));
                }
            }
            '|' => {
                if chars.get(index + 1) == Some(&'|') {
                    tokens.push(Token::Or);
                    index += 2;
                } else {
                    return Err(ForgeError::message("expected `||` in condition"));
                }
            }
            '\'' => {
                index += 1;
                let start = index;
                while index < chars.len() && chars[index] != '\'' {
                    index += 1;
                }
                if index >= chars.len() {
                    return Err(ForgeError::message(
                        "unterminated string literal in condition",
                    ));
                }
                tokens.push(Token::Literal(chars[start..index].iter().collect()));
                index += 1;
            }
            _ => {
                let start = index;
                while index < chars.len()
                    && !matches!(
                        chars[index],
                        ' ' | '\t' | '\n' | '=' | '!' | '&' | '|' | '\''
                    )
                {
                    index += 1;
                }
                let text: String = chars[start..index].iter().collect();
                if text == "true" || text == "false" || text.chars().all(|ch| ch.is_ascii_digit()) {
                    tokens.push(Token::Literal(text));
                } else {
                    tokens.push(Token::Identifier(text));
                }
            }
        }
    }

    Ok(tokens)
}
