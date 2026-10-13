//! Bounded, deterministic formula prototype. Results never replace source caches.
use crate::spreadsheet_edit::{Content, Workbook, validate};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Number(f64),
    Text(String),
    Boolean(bool),
    Empty,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Failure {
    pub kind: String,
    pub code: String,
}
type Calc<T> = Result<T, Failure>;
fn failure(kind: &'static str, code: &'static str) -> Failure {
    Failure {
        kind: kind.into(),
        code: code.into(),
    }
}
fn unsupported() -> Failure {
    failure("unsupported", "UNSUPPORTED_FORMULA")
}
fn limit() -> Failure {
    failure("limit", "CALCULATION_LIMIT")
}
fn numeric(n: f64) -> Calc<Value> {
    if n.is_finite() {
        Ok(Value::Number(n))
    } else {
        Err(failure("error", "#NUM!"))
    }
}
fn number(value: Value) -> Calc<f64> {
    match value {
        Value::Number(n) => Ok(n),
        Value::Boolean(b) => Ok(u8::from(b) as f64),
        Value::Empty => Ok(0.),
        Value::Text(_) => Err(unsupported()),
    }
}
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    Word(String),
    Quoted(String),
    Text(String),
    Error(&'static str),
    Comparison(&'static str),
    Symbol(char),
}
fn tokens(input: &str) -> Calc<Vec<Token>> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '#' {
            let tail: String = chars[i..].iter().collect();
            let code = [
                "#DIV/0!", "#REF!", "#VALUE!", "#N/A", "#NAME?", "#NUM!", "#NULL!",
            ]
            .into_iter()
            .find(|code| tail.starts_with(code))
            .ok_or_else(unsupported)?;
            i += code.len();
            if code == "#REF!" {
                let start = i;
                while chars
                    .get(i)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '$')
                {
                    i += 1;
                }
                if i > start {
                    address(&chars[start..i].iter().collect::<String>(), None)?;
                }
            }
            out.push(Token::Error(code));
        } else if c == '\'' || c == '"' {
            i += 1;
            let mut text = String::new();
            let mut closed = false;
            while i < chars.len() {
                let next = chars[i];
                i += 1;
                if next == c {
                    if chars.get(i) == Some(&c) {
                        text.push(c);
                        i += 1;
                    } else {
                        closed = true;
                        break;
                    }
                } else {
                    text.push(next);
                }
            }
            if !closed {
                return Err(unsupported());
            }
            out.push(if c == '\'' {
                Token::Quoted(text)
            } else {
                Token::Text(text)
            });
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if matches!(chars.get(i), Some('e' | 'E')) {
                i += 1;
                if matches!(chars.get(i), Some('+' | '-')) {
                    i += 1;
                }
                while chars.get(i).is_some_and(char::is_ascii_digit) {
                    i += 1;
                }
            }
            let n: f64 = chars[start..i]
                .iter()
                .collect::<String>()
                .parse()
                .map_err(|_| unsupported())?;
            if !n.is_finite() {
                return Err(unsupported());
            }
            out.push(Token::Number(n));
        } else if c.is_alphabetic() || matches!(c, '_' | '$' | '\\') {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '$' | '.' | '\\'))
            {
                i += 1;
            }
            out.push(Token::Word(chars[start..i].iter().collect()));
        } else if "=<>".contains(c) {
            let next = chars.get(i + 1).copied();
            let op = match (c, next) {
                ('<', Some('=')) => "<=",
                ('>', Some('=')) => ">=",
                ('<', Some('>')) => "<>",
                ('<', _) => "<",
                ('>', _) => ">",
                _ => "=",
            };
            i += op.len();
            out.push(Token::Comparison(op));
        } else if "+-*/^%(),:!".contains(c) {
            out.push(Token::Symbol(c));
            i += 1;
        } else {
            return Err(unsupported());
        }
        if out.len() > 512 {
            return Err(limit());
        }
    }
    Ok(out)
}
#[derive(Clone, Debug)]
struct Reference {
    sheet: Option<String>,
    row: u32,
    column: u32,
}
fn address(word: &str, sheet: Option<String>) -> Calc<Reference> {
    let stripped = word.strip_prefix('$').unwrap_or(word);
    let letters_end = stripped
        .find(|c: char| !c.is_ascii_alphabetic())
        .ok_or_else(unsupported)?;
    let tail = &stripped[letters_end..];
    let digits = tail.strip_prefix('$').unwrap_or(tail);
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(unsupported());
    }
    let word = format!("{}{}", &stripped[..letters_end], digits);
    let split = word
        .find(|c: char| c.is_ascii_digit())
        .ok_or_else(unsupported)?;
    let (letters, digits) = word.split_at(split);
    if letters.is_empty()
        || !letters.chars().all(|c| c.is_ascii_alphabetic())
        || !digits.chars().all(|c| c.is_ascii_digit())
        || digits.starts_with('0')
    {
        return Err(unsupported());
    }
    let mut col = 0u32;
    for c in letters.bytes() {
        col = col
            .checked_mul(26)
            .and_then(|n| n.checked_add((c.to_ascii_uppercase() - b'A' + 1) as u32))
            .ok_or_else(unsupported)?;
    }
    let row: u32 = digits.parse().map_err(|_| unsupported())?;
    if col == 0 || col > 16384 || row == 0 || row > 1048576 {
        return Err(unsupported());
    }
    Ok(Reference {
        sheet,
        row: row - 1,
        column: col - 1,
    })
}
#[derive(Debug)]
enum Expr {
    Comparison(&'static str, Box<Expr>, Box<Expr>),
    Literal(Value),
    Error(&'static str),
    Reference(Reference),
    Range(Reference, Reference),
    Unary(char, Box<Expr>),
    Binary(char, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    nodes: usize,
}
impl Parser {
    fn symbol(&mut self, c: char) -> bool {
        if self.tokens.get(self.pos) == Some(&Token::Symbol(c)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn word(&mut self) -> Calc<String> {
        match self.tokens.get(self.pos).cloned() {
            Some(Token::Word(word)) => {
                self.pos += 1;
                Ok(word)
            }
            _ => Err(unsupported()),
        }
    }
    fn expression(&mut self, minimum: u8, depth: usize) -> Calc<Expr> {
        if depth > 64 {
            return Err(limit());
        }
        self.nodes += 1;
        if self.nodes > 256 {
            return Err(limit());
        }
        let token = self.tokens.get(self.pos).cloned().ok_or_else(unsupported)?;
        self.pos += 1;
        let quoted = matches!(&token, Token::Quoted(_));
        let mut left = match token {
            Token::Number(n) => Expr::Literal(Value::Number(n)),
            Token::Text(s) => Expr::Literal(Value::Text(s)),
            Token::Error(code) => Expr::Error(code),
            Token::Symbol(c @ ('+' | '-')) => {
                Expr::Unary(c, Box::new(self.expression(50, depth + 1)?))
            }
            Token::Symbol('(') => {
                let expression = self.expression(0, depth + 1)?;
                if !self.symbol(')') {
                    return Err(unsupported());
                }
                expression
            }
            Token::Word(word) | Token::Quoted(word) => {
                if self.symbol('!') {
                    let cell = self.word()?;
                    Expr::Reference(address(&cell, Some(word))?)
                } else if quoted {
                    return Err(unsupported());
                } else if self.symbol('(') {
                    if ![
                        "SUM", "AVERAGE", "MIN", "MAX", "COUNT", "ABS", "SQRT", "INT", "SIGN",
                        "IF", "IFERROR", "AND", "OR", "NOT",
                    ]
                    .contains(&word.to_ascii_uppercase().as_str())
                    {
                        return Err(unsupported());
                    }
                    let mut args = vec![];
                    if !self.symbol(')') {
                        loop {
                            args.push(self.expression(0, depth + 1)?);
                            if self.symbol(')') {
                                break;
                            }
                            if !self.symbol(',') {
                                return Err(unsupported());
                            }
                        }
                    }
                    Expr::Call(word.to_ascii_uppercase(), args)
                } else if word.eq_ignore_ascii_case("TRUE") {
                    Expr::Literal(Value::Boolean(true))
                } else if word.eq_ignore_ascii_case("FALSE") {
                    Expr::Literal(Value::Boolean(false))
                } else {
                    Expr::Reference(address(&word, None)?)
                }
            }
            _ => return Err(unsupported()),
        };
        if self.symbol(':') {
            let Expr::Reference(start) = left else {
                return Err(unsupported());
            };
            let end = address(&self.word()?, start.sheet.clone())?;
            left = Expr::Range(start, end);
        }
        loop {
            if self.tokens.get(self.pos) == Some(&Token::Symbol('%')) && minimum <= 40 {
                self.pos += 1;
                left = Expr::Unary('%', Box::new(left));
                self.nodes += 1;
            } else if let Some(Token::Symbol(op @ ('+' | '-' | '*' | '/' | '^'))) =
                self.tokens.get(self.pos).cloned()
            {
                let precedence = match op {
                    '+' | '-' => 10,
                    '*' | '/' => 20,
                    _ => 30,
                };
                if precedence < minimum {
                    break;
                }
                self.pos += 1;
                let right = self.expression(precedence + 1, depth + 1)?;
                left = Expr::Binary(op, Box::new(left), Box::new(right));
                self.nodes += 1;
            } else if let Some(Token::Comparison(op)) = self.tokens.get(self.pos).cloned() {
                if minimum > 1 {
                    break;
                }
                self.pos += 1;
                let right = self.expression(2, depth + 1)?;
                left = Expr::Comparison(op, Box::new(left), Box::new(right));
                self.nodes += 1;
            } else {
                break;
            }
            if self.nodes > 256 {
                return Err(limit());
            }
        }
        Ok(left)
    }
    fn parse(formula: &str) -> Calc<Expr> {
        let mut parser = Self {
            tokens: tokens(formula)?,
            pos: 0,
            nodes: 0,
        };
        let expression = parser.expression(0, 0)?;
        if parser.pos != parser.tokens.len() {
            return Err(unsupported());
        }
        Ok(expression)
    }
}
type Key = (usize, u32, u32);
struct Engine<'a> {
    book: &'a Workbook,
    cells: HashMap<Key, &'a Content>,
    cache: HashMap<Key, Calc<Value>>,
    visiting: HashSet<Key>,
    work: usize,
}
impl Engine<'_> {
    fn step(&mut self) -> Calc<()> {
        if self.work >= 1_000_000 {
            return Err(limit());
        }
        self.work += 1;
        Ok(())
    }
    fn key(&self, reference: &Reference, sheet: usize) -> Calc<Key> {
        if reference
            .sheet
            .as_ref()
            .is_some_and(|name| name.contains(['[', ']', ':']))
        {
            return Err(unsupported());
        }
        let index = match &reference.sheet {
            None => sheet,
            Some(name) => self
                .book
                .sheets
                .iter()
                .position(|sheet| sheet.name.to_lowercase() == name.to_lowercase())
                .ok_or_else(|| failure("error", "#REF!"))?,
        };
        Ok((index, reference.row, reference.column))
    }
    fn cell(&mut self, key: Key, depth: usize) -> Calc<Value> {
        self.step()?;
        if let Some(value) = self.cache.get(&key) {
            return value.clone();
        }
        if depth > 64 {
            return Err(limit());
        }
        if !self.visiting.insert(key) {
            return Err(failure("cycle", "CIRCULAR_REFERENCE"));
        }
        let result = match self.cells.get(&key).copied() {
            None | Some(Content::Empty) => Ok(Value::Empty),
            Some(Content::Number(n)) => Ok(Value::Number(*n)),
            Some(Content::Text(s)) => Ok(Value::Text(s.clone())),
            Some(Content::Boolean(b)) => Ok(Value::Boolean(*b)),
            Some(Content::Formula(formula)) => Parser::parse(formula)
                .and_then(|expression| self.eval(&expression, key.0, depth + 1))
                .map(|value| {
                    if value == Value::Empty {
                        Value::Number(0.)
                    } else {
                        value
                    }
                }),
        };
        self.visiting.remove(&key);
        self.cache.insert(key, result.clone());
        result
    }
    fn eval(&mut self, expression: &Expr, sheet: usize, depth: usize) -> Calc<Value> {
        self.step()?;
        if depth > 64 {
            return Err(limit());
        }
        match expression {
            Expr::Comparison(op, left, right) => {
                let a = self.eval(left, sheet, depth + 1)?;
                let b = self.eval(right, sheet, depth + 1)?;
                let a = if a == Value::Empty {
                    Value::Number(0.)
                } else {
                    a
                };
                let b = if b == Value::Empty {
                    Value::Number(0.)
                } else {
                    b
                };
                let ordering = match (a, b) {
                    (Value::Number(a), Value::Number(b)) => {
                        a.partial_cmp(&b).ok_or_else(unsupported)?
                    }
                    (Value::Boolean(a), Value::Boolean(b)) => a.cmp(&b),
                    _ => return Err(unsupported()),
                };
                use std::cmp::Ordering::*;
                Ok(Value::Boolean(match *op {
                    "=" => ordering == Equal,
                    "<>" => ordering != Equal,
                    "<" => ordering == Less,
                    ">" => ordering == Greater,
                    "<=" => ordering != Greater,
                    _ => ordering != Less,
                }))
            }
            Expr::Literal(value) => Ok(value.clone()),
            Expr::Reference(reference) => self.cell(self.key(reference, sheet)?, depth + 1),
            Expr::Error(code) => Err(failure("error", code)),
            Expr::Range(_, _) => Err(unsupported()),
            Expr::Unary(op, expression) => {
                let n = number(self.eval(expression, sheet, depth + 1)?)?;
                numeric(match op {
                    '-' => -n,
                    '%' => n / 100.,
                    _ => n,
                })
            }
            Expr::Binary(op, left, right) => {
                let a = number(self.eval(left, sheet, depth + 1)?)?;
                let b = number(self.eval(right, sheet, depth + 1)?)?;
                if *op == '/' && b == 0. {
                    return Err(failure("error", "#DIV/0!"));
                }
                if *op == '^' && a == 0. && b < 0. {
                    return Err(failure("error", "#DIV/0!"));
                }
                if *op == '^' && a == 0. && b == 0. {
                    return Err(failure("error", "#NUM!"));
                }
                numeric(match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' => a / b,
                    _ => a.powf(b),
                })
            }
            Expr::Call(name, args) => {
                if ![
                    "SUM", "AVERAGE", "MIN", "MAX", "COUNT", "ABS", "SQRT", "INT", "SIGN", "IF",
                    "IFERROR", "AND", "OR", "NOT",
                ]
                .contains(&name.as_str())
                {
                    return Err(unsupported());
                }
                if name == "IF" {
                    if !(2..=3).contains(&args.len()) {
                        return Err(unsupported());
                    }
                    let condition = number(self.eval(&args[0], sheet, depth + 1)?)? != 0.;
                    return match args.get(if condition { 1 } else { 2 }) {
                        Some(branch) => self.eval(branch, sheet, depth + 1),
                        None => Ok(Value::Boolean(false)),
                    };
                }
                if name == "IFERROR" {
                    if args.len() != 2 {
                        return Err(unsupported());
                    }
                    let result = match self.eval(&args[0], sheet, depth + 1) {
                        Err(error) if error.kind == "error" => {
                            self.eval(&args[1], sheet, depth + 1)
                        }
                        other => other,
                    }?;
                    return Ok(if result == Value::Empty {
                        Value::Text(String::new())
                    } else {
                        result
                    });
                }
                if name == "NOT" {
                    if args.len() != 1 {
                        return Err(unsupported());
                    }
                    return Ok(Value::Boolean(
                        number(self.eval(&args[0], sheet, depth + 1)?)? == 0.,
                    ));
                }
                if ["ABS", "SQRT", "INT", "SIGN"].contains(&name.as_str()) {
                    if args.len() != 1 {
                        return Err(unsupported());
                    }
                    let value = number(self.eval(&args[0], sheet, depth + 1)?)?;
                    return numeric(match name.as_str() {
                        "ABS" => value.abs(),
                        "SQRT" => {
                            if value < 0. {
                                return Err(failure("error", "#NUM!"));
                            }
                            value.sqrt()
                        }
                        "INT" => value.floor(),
                        _ => {
                            if value == 0. {
                                0.
                            } else if value > 0. {
                                1.
                            } else {
                                -1.
                            }
                        }
                    });
                }
                if args.is_empty() || args.len() > 255 {
                    return Err(unsupported());
                }
                let mut values = vec![];
                for argument in args {
                    if let Expr::Range(start, end) = argument {
                        let a = self.key(start, sheet)?;
                        let b = self.key(end, sheet)?;
                        let (top, bottom, left, right) =
                            (a.1.min(b.1), a.1.max(b.1), a.2.min(b.2), a.2.max(b.2));
                        if u64::from(bottom - top + 1) * u64::from(right - left + 1) > 100000 {
                            return Err(limit());
                        }
                        for row in top..=bottom {
                            for column in left..=right {
                                let value = self.cell((a.0, row, column), depth + 1);
                                Self::collect(&mut values, value, true, name)?;
                            }
                        }
                    } else {
                        let value = self.eval(argument, sheet, depth + 1);
                        Self::collect(
                            &mut values,
                            value,
                            matches!(argument, Expr::Reference(_)),
                            name,
                        )?;
                    }
                }
                if name == "AND" || name == "OR" {
                    if values.is_empty() {
                        return Err(failure("error", "#VALUE!"));
                    }
                    return Ok(Value::Boolean(if name == "AND" {
                        values.iter().all(|n| *n != 0.)
                    } else {
                        values.iter().any(|n| *n != 0.)
                    }));
                }
                let sum: f64 = values.iter().sum();
                numeric(match name.as_str() {
                    "COUNT" => values.len() as f64,
                    "SUM" => sum,
                    "AVERAGE" => {
                        if values.is_empty() {
                            return Err(failure("error", "#DIV/0!"));
                        }
                        sum / values.len() as f64
                    }
                    "MIN" => values.into_iter().reduce(f64::min).unwrap_or(0.),
                    _ => values.into_iter().reduce(f64::max).unwrap_or(0.),
                })
            }
        }
    }
    fn collect(
        values: &mut Vec<f64>,
        value: Calc<Value>,
        reference: bool,
        function: &str,
    ) -> Calc<()> {
        match value {
            Ok(Value::Number(n)) => values.push(n),
            Ok(Value::Boolean(b)) if !reference || function == "AND" || function == "OR" => {
                values.push(u8::from(b) as f64)
            }
            Ok(Value::Text(_)) if !reference && (function == "AND" || function == "OR") => {
                return Err(unsupported());
            }
            Ok(Value::Text(s)) if !reference => {
                let n = s
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or_else(unsupported)?;
                values.push(n);
            }
            Err(error) if reference && function == "COUNT" && error.kind == "error" => {}
            Err(error) => return Err(error),
            _ => {}
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct CellResult {
    pub sheet: usize,
    pub row: u32,
    pub column: u32,
    pub formula: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub engine: String,
    pub complete: bool,
    pub operations: usize,
    pub cells: Vec<CellResult>,
}
pub fn calculate(model: &Workbook) -> anyhow::Result<Report> {
    validate(model)?;
    let mut engine = Engine {
        book: model,
        cells: HashMap::new(),
        cache: HashMap::new(),
        visiting: HashSet::new(),
        work: 0,
    };
    for (sheet, data) in model.sheets.iter().enumerate() {
        for cell in &data.cells {
            engine
                .cells
                .insert((sheet, cell.row, cell.column), &cell.content);
        }
    }
    let mut results = vec![];
    for (sheet, data) in model.sheets.iter().enumerate() {
        for cell in &data.cells {
            if let Content::Formula(formula) = &cell.content {
                let result = engine.cell((sheet, cell.row, cell.column), 0);
                results.push(CellResult {
                    sheet,
                    row: cell.row,
                    column: cell.column,
                    formula: formula.clone(),
                    value: result.as_ref().ok().cloned(),
                    failure: result.err(),
                });
            }
        }
    }
    let complete = results.iter().all(|cell| {
        cell.failure
            .as_ref()
            .is_none_or(|failure| failure.kind == "error")
    });
    Ok(Report {
        engine: "bounded-numeric-v1".into(),
        complete,
        operations: engine.work,
        cells: results,
    })
}
