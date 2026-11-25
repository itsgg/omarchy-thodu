//! Enough of Lua to edit the user's Hyprland bindings without losing a line
//! of them (REQ-INSTALL-01): a tokenizer that knows comments, every string
//! form and long brackets, the `o.bind` and `hl.env` calls in a file, and
//! where its top-level `return` is. Anything it cannot read is an error, so
//! install refuses instead of guessing.

use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Name(String),
    /// A string literal, with its escapes decoded.
    Str(String),
    Number,
    Punct(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    /// Byte offsets of the token in the source.
    pub span: Range<usize>,
}

const PUNCTS: [&str; 33] = [
    "...", "..", "==", "~=", "<=", ">=", "<<", ">>", "//", "::", "+", "-", "*", "/", "%", "^", "#",
    "&", "~", "|", "<", ">", "=", "(", ")", "{", "}", "[", "]", ";", ":", ",", ".",
];

fn line_of(src: &str, at: usize) -> usize {
    src[..at.min(src.len())].matches('\n').count() + 1
}

/// The level of a long bracket opening at `at` (`[[` is 0, `[==[` is 2).
fn long_bracket(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at) != Some(&b'[') {
        return None;
    }
    let mut level = 0;
    while bytes.get(at + 1 + level) == Some(&b'=') {
        level += 1;
    }
    (bytes.get(at + 1 + level) == Some(&b'[')).then_some(level)
}

/// The end of a long bracket body opened at `at` with `level`, past its
/// closing bracket, and the body's text.
fn long_body(src: &str, at: usize, level: usize) -> Result<(usize, String), String> {
    let open = at + level + 2;
    let close = format!("]{}]", "=".repeat(level));
    let Some(found) = src[open..].find(&close) else {
        return Err(format!(
            "line {}: a long string or comment is never closed",
            line_of(src, at)
        ));
    };
    let mut body = &src[open..open + found];
    // A newline straight after the opening bracket is not part of the text.
    if let Some(rest) = body
        .strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'))
    {
        body = rest;
    }
    Ok((open + found + close.len(), body.to_string()))
}

fn short_string(src: &str, at: usize) -> Result<(usize, String), String> {
    let bytes = src.as_bytes();
    let quote = bytes[at];
    let mut out: Vec<u8> = Vec::new();
    let mut i = at + 1;
    let unterminated = || format!("line {}: a string is never closed", line_of(src, at));
    loop {
        let Some(&c) = bytes.get(i) else {
            return Err(unterminated());
        };
        match c {
            b'\n' | b'\r' => return Err(unterminated()),
            c if c == quote => return Ok((i + 1, String::from_utf8_lossy(&out).into_owned())),
            b'\\' => {
                let Some(&e) = bytes.get(i + 1) else {
                    return Err(unterminated());
                };
                i += 2;
                match e {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'a' => out.push(7),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'v' => out.push(11),
                    b'\\' | b'"' | b'\'' => out.push(e),
                    b'\n' | b'\r' => {
                        out.push(b'\n');
                        let pair = if e == b'\n' { b'\r' } else { b'\n' };
                        if bytes.get(i) == Some(&pair) {
                            i += 1;
                        }
                    }
                    b'z' => {
                        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                            i += 1;
                        }
                    }
                    b'x' => {
                        let hex = src
                            .get(i..i + 2)
                            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
                        let Some(hex) = hex else {
                            return Err(format!(
                                "line {}: a \\x escape needs two hex digits",
                                line_of(src, i)
                            ));
                        };
                        out.push(u8::from_str_radix(hex, 16).expect("two hex digits"));
                        i += 2;
                    }
                    b'u' => {
                        let bad = || format!("line {}: a bad \\u escape", line_of(src, i));
                        if bytes.get(i) != Some(&b'{') {
                            return Err(bad());
                        }
                        let close = src[i..].find('}').ok_or_else(bad)?;
                        let digits = &src[i + 1..i + close];
                        let code = u32::from_str_radix(digits, 16).map_err(|_| bad())?;
                        let ch = char::from_u32(code).ok_or_else(bad)?;
                        let mut buf = [0; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        i += close + 1;
                    }
                    d if d.is_ascii_digit() => {
                        let mut value: u32 = u32::from(d - b'0');
                        let mut n = 1;
                        while n < 3 && bytes.get(i).is_some_and(u8::is_ascii_digit) {
                            value = value * 10 + u32::from(bytes[i] - b'0');
                            i += 1;
                            n += 1;
                        }
                        let byte = u8::try_from(value).map_err(|_| {
                            format!("line {}: a decimal escape above 255", line_of(src, i))
                        })?;
                        out.push(byte);
                    }
                    _ => {
                        return Err(format!(
                            "line {}: an unknown escape in a string",
                            line_of(src, i)
                        ));
                    }
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
}

/// Every token of `src`, comments left out.
pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if src[i..].starts_with("--") {
            if let Some(level) = long_bracket(bytes, i + 2) {
                i = long_body(src, i + 2, level)?.0;
            } else {
                i = src[i..].find('\n').map_or(bytes.len(), |n| i + n);
            }
            continue;
        }
        if c == b'[' && bytes.get(i + 1).is_some_and(|&b| b == b'[' || b == b'=') {
            let Some(level) = long_bracket(bytes, i) else {
                return Err(format!(
                    "line {}: an invalid long string delimiter",
                    line_of(src, i)
                ));
            };
            let (end, body) = long_body(src, i, level)?;
            tokens.push(Token {
                kind: Kind::Str(body),
                span: i..end,
            });
            i = end;
            continue;
        }
        if c == b'"' || c == b'\'' {
            let (end, text) = short_string(src, i)?;
            tokens.push(Token {
                kind: Kind::Str(text),
                span: i..end,
            });
            i = end;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while bytes
                .get(i)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
                i += 1;
            }
            tokens.push(Token {
                kind: Kind::Name(src[start..i].to_string()),
                span: start..i,
            });
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            while let Some(&b) = bytes.get(i) {
                let exponent = matches!(b, b'e' | b'E' | b'p' | b'P');
                if b.is_ascii_alphanumeric() || b == b'.' || b == b'_' {
                    i += 1;
                    if exponent && bytes.get(i).is_some_and(|s| *s == b'+' || *s == b'-') {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            tokens.push(Token {
                kind: Kind::Number,
                span: start..i,
            });
            continue;
        }
        let Some(punct) = PUNCTS.iter().find(|p| src[i..].starts_with(**p)) else {
            return Err(format!(
                "line {}: a character Lua does not allow outside a string or comment",
                line_of(src, i)
            ));
        };
        tokens.push(Token {
            kind: Kind::Punct(punct),
            span: i..i + punct.len(),
        });
        i += punct.len();
    }
    Ok(tokens)
}

/// A call `a.b(...)` found in the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The callee as written, such as `o.bind`.
    pub callee: String,
    /// From the callee's first byte to just past the closing parenthesis.
    pub span: Range<usize>,
    /// Each argument's tokens.
    pub args: Vec<Vec<Token>>,
}

impl Call {
    /// The source text of argument `n`.
    pub fn arg_source<'a>(&self, src: &'a str, n: usize) -> Option<&'a str> {
        let arg = self.args.get(n)?;
        Some(&src[arg.first()?.span.start..arg.last()?.span.end])
    }

    /// Argument `n` as a string, when it is one literal or literals joined
    /// with `..`; `None` when it needs evaluating (a variable, a call).
    pub fn arg_string(&self, n: usize) -> Option<String> {
        constant_string(self.args.get(n)?)
    }
}

fn constant_string(tokens: &[Token]) -> Option<String> {
    let mut out = String::new();
    let mut want_string = true;
    for token in tokens {
        match (&token.kind, want_string) {
            (Kind::Str(s), true) => out.push_str(s),
            (Kind::Punct(".."), false) => {}
            (Kind::Punct("("), _) | (Kind::Punct(")"), _) => continue,
            _ => return None,
        }
        want_string = !want_string;
    }
    (!want_string).then_some(out)
}

/// Every call of `object.method(...)` for a method in `methods`.
pub fn calls(tokens: &[Token], object: &str, methods: &[&str]) -> Result<Vec<Call>, String> {
    let mut found = Vec::new();
    let mut i = 0;
    while i + 3 < tokens.len() {
        let is_call = matches!(&tokens[i].kind, Kind::Name(n) if n == object)
            && tokens[i + 1].kind == Kind::Punct(".")
            && matches!(&tokens[i + 2].kind, Kind::Name(m) if methods.contains(&m.as_str()))
            && tokens[i + 3].kind == Kind::Punct("(")
            && (i == 0 || !matches!(tokens[i - 1].kind, Kind::Punct(".") | Kind::Punct(":")));
        if !is_call {
            i += 1;
            continue;
        }
        let Kind::Name(method) = &tokens[i + 2].kind else {
            unreachable!("matched a name above");
        };
        let mut depth = 0usize;
        let mut args: Vec<Vec<Token>> = vec![Vec::new()];
        let mut j = i + 3;
        let close = loop {
            let Some(token) = tokens.get(j) else {
                return Err(format!(
                    "the call {object}.{method} at byte {} has no closing parenthesis",
                    tokens[i].span.start
                ));
            };
            match token.kind {
                Kind::Punct("(") | Kind::Punct("[") | Kind::Punct("{") => {
                    depth += 1;
                    if depth > 1 {
                        args.last_mut().expect("an argument").push(token.clone());
                    }
                }
                Kind::Punct(")") | Kind::Punct("]") | Kind::Punct("}") => {
                    depth -= 1;
                    if depth == 0 {
                        break j;
                    }
                    args.last_mut().expect("an argument").push(token.clone());
                }
                Kind::Punct(",") if depth == 1 => args.push(Vec::new()),
                _ => args.last_mut().expect("an argument").push(token.clone()),
            }
            j += 1;
        };
        if args.len() == 1 && args[0].is_empty() {
            args.clear();
        }
        found.push(Call {
            callee: format!("{object}.{method}"),
            span: tokens[i].span.start..tokens[close].span.end,
            args,
        });
        i = close + 1;
    }
    Ok(found)
}

/// The byte offset of the file's top-level `return`, if it has one. Lua
/// allows `return` only as a block's last statement, so code added to the
/// file goes before it.
pub fn top_level_return(tokens: &[Token]) -> Option<usize> {
    let mut depth = 0i64;
    for token in tokens {
        match &token.kind {
            Kind::Name(n) if matches!(n.as_str(), "function" | "do" | "if" | "repeat") => {
                depth += 1;
            }
            Kind::Name(n) if matches!(n.as_str(), "end" | "until") => depth -= 1,
            Kind::Name(n) if n == "return" && depth == 0 => return Some(token.span.start),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_of_every_form_decode() {
        let tokens = tokenize(
            r#"a = 'x\'y' .. "q\"\65" .. [[raw]] .. [==[
long]]x]==]"#,
        )
        .expect("tokens");
        let strings: Vec<_> = tokens
            .iter()
            .filter_map(|t| match &t.kind {
                Kind::Str(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(strings, ["x'y", "q\"A", "raw", "long]]x"]);
    }

    #[test]
    fn comments_hide_calls_and_unclosed_input_is_an_error() {
        let src = "--[[ o.bind(\"A\", \"Thodu hints\", \"x\") ]]\n-- o.bind(\"B\")\n";
        let tokens = tokenize(src).expect("tokens");
        assert!(tokens.is_empty(), "{tokens:?}");
        assert!(tokenize("x = \"open").is_err());
        assert!(tokenize("--[[ open").is_err());
        assert!(tokenize("x = [=[ open ]]").is_err());
    }

    #[test]
    fn calls_split_arguments_across_lines_and_nesting() {
        let src = "o.bind(\n  'SUPER + ' .. \"F12\",\n  \"Thodu hints\",\n  f(1, {2, 3})\n)\n";
        let tokens = tokenize(src).expect("tokens");
        let found = calls(&tokens, "o", &["bind"]).expect("calls");
        assert_eq!(found.len(), 1);
        let call = &found[0];
        assert_eq!(call.args.len(), 3);
        assert_eq!(call.arg_string(0).as_deref(), Some("SUPER + F12"));
        assert_eq!(call.arg_string(1).as_deref(), Some("Thodu hints"));
        assert_eq!(call.arg_string(2), None);
        assert_eq!(call.arg_source(src, 2), Some("f(1, {2, 3})"));
        assert_eq!(&src[call.span.clone()], src.trim_end());
        assert!(calls(&tokenize("o.bind(1, 2").expect("tokens"), "o", &["bind"]).is_err());
    }

    #[test]
    fn a_return_inside_a_function_is_not_the_files() {
        let src =
            "local function f()\n  if x then return 1 end\n  return 2\nend\nreturn { f = f }\n";
        let tokens = tokenize(src).expect("tokens");
        assert_eq!(top_level_return(&tokens), src.find("return {"));
        assert_eq!(top_level_return(&tokenize("o.bind(1)").expect("t")), None);
    }
}
