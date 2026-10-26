//! Lua 5.4 patterns in Rust, within the call budget (phase 3 audit, F1).
//!
//! Lua's `string.find`, `match`, `gmatch` and `gsub` run in C, where the
//! instruction hook never fires: one call with a backtracking pattern could
//! freeze Ferny. This is a line-by-line port of the matcher in Lua 5.4's
//! `lstrlib.c` that checks the plugin's deadline every 1,024 steps and fails
//! with the same time-limit error as the hook. Results and error messages
//! follow Lua; `tests/patterns.rs` compares both implementations. Two known
//! differences: an error from a tail call (`return s:find("%")`) has no
//! position prefix, and an error raised inside a `gsub` replacement
//! function comes back as a Ferny error with the same text.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use mlua::{Lua, LuaString, MultiValue, Value};

use crate::TIME_LIMIT_MESSAGE;

const MAXCCALLS: usize = 200;
const MAXCAPTURES: usize = 32;
const CAP_UNFINISHED: isize = -1;
const CAP_POSITION: isize = -2;
const L_ESC: u8 = b'%';
const SPECIALS: &[u8] = b"^$*+?.([%-";

enum Fail {
    /// A Lua error: raised in Lua as `error(message, 2)`, as C does.
    Lua(String),
    Time,
    Mlua(mlua::Error),
}

impl From<mlua::Error> for Fail {
    fn from(err: mlua::Error) -> Self {
        Self::Mlua(err)
    }
}

/// `true, results…`, or `false, message` for the Lua wrapper to raise.
fn finish(lua: &Lua, result: R<MultiValue>) -> mlua::Result<MultiValue> {
    match result {
        Ok(mut values) => {
            values.push_front(Value::Boolean(true));
            Ok(values)
        }
        Err(Fail::Lua(message)) => Ok(MultiValue::from_iter([
            Value::Boolean(false),
            Value::String(lua.create_string(message)?),
        ])),
        Err(Fail::Time) => Err(mlua::Error::runtime(TIME_LIMIT_MESSAGE)),
        Err(Fail::Mlua(err)) => Err(err),
    }
}

type R<T> = Result<T, Fail>;

fn error<T>(message: impl Into<String>) -> R<T> {
    Err(Fail::Lua(message.into()))
}

struct MatchState<'a> {
    src: &'a [u8],
    pat: &'a [u8],
    level: usize,
    capture: [(usize, isize); MAXCAPTURES],
    matchdepth: usize,
    steps: u32,
    deadline: Option<Instant>,
}

impl<'a> MatchState<'a> {
    fn new(src: &'a [u8], pat: &'a [u8], deadline: Option<Instant>) -> Self {
        Self {
            src,
            pat,
            level: 0,
            capture: [(0, 0); MAXCAPTURES],
            matchdepth: MAXCCALLS,
            steps: 0,
            deadline,
        }
    }

    fn reprep(&mut self) {
        self.level = 0;
        self.matchdepth = MAXCCALLS;
    }

    /// One unit of work; every 1,024 of them the deadline is checked.
    fn tick(&mut self) -> R<()> {
        self.steps = self.steps.wrapping_add(1);
        if self.steps.is_multiple_of(1024) && self.deadline.is_some_and(|d| Instant::now() > d) {
            return Err(Fail::Time);
        }
        Ok(())
    }

    /// Pattern byte at `i`; 0 past the end, as C's terminating NUL.
    fn p(&self, i: usize) -> u8 {
        self.pat.get(i).copied().unwrap_or(0)
    }

    /// Subject byte at `i`; 0 past the end.
    fn s(&self, i: usize) -> u8 {
        self.src.get(i).copied().unwrap_or(0)
    }

    fn class_end(&self, mut p: usize) -> R<usize> {
        let c = self.p(p);
        p += 1;
        if c == L_ESC {
            if p >= self.pat.len() {
                return error("malformed pattern (ends with '%')");
            }
            return Ok(p + 1);
        }
        if c == b'[' {
            if self.p(p) == b'^' {
                p += 1;
            }
            loop {
                if p >= self.pat.len() {
                    return error("malformed pattern (missing ']')");
                }
                let cc = self.p(p);
                p += 1;
                if cc == L_ESC && p < self.pat.len() {
                    p += 1;
                }
                if self.p(p) == b']' {
                    break;
                }
            }
            return Ok(p + 1);
        }
        Ok(p)
    }

    fn match_bracket_class(&self, c: u8, mut p: usize, ec: usize) -> bool {
        let mut sig = true;
        if self.p(p + 1) == b'^' {
            sig = false;
            p += 1;
        }
        loop {
            p += 1;
            if p >= ec {
                break;
            }
            if self.p(p) == L_ESC {
                p += 1;
                if match_class(c, self.p(p)) {
                    return sig;
                }
            } else if self.p(p + 1) == b'-' && p + 2 < ec {
                p += 2;
                if self.p(p - 2) <= c && c <= self.p(p) {
                    return sig;
                }
            } else if self.p(p) == c {
                return sig;
            }
        }
        !sig
    }

    fn single_match(&mut self, s: usize, p: usize, ep: usize) -> R<bool> {
        self.tick()?;
        if s >= self.src.len() {
            return Ok(false);
        }
        let c = self.src[s];
        Ok(match self.p(p) {
            b'.' => true,
            L_ESC => match_class(c, self.p(p + 1)),
            b'[' => self.match_bracket_class(c, p, ep - 1),
            other => other == c,
        })
    }

    fn match_balance(&mut self, s: usize, p: usize) -> R<Option<usize>> {
        if p + 1 >= self.pat.len() {
            return error("malformed pattern (missing arguments to '%b')");
        }
        if self.s(s) != self.p(p) || s >= self.src.len() {
            return Ok(None);
        }
        let (b, e) = (self.p(p), self.p(p + 1));
        let mut cont = 1;
        let mut s = s;
        loop {
            s += 1;
            if s >= self.src.len() {
                return Ok(None);
            }
            self.tick()?;
            if self.src[s] == e {
                cont -= 1;
                if cont == 0 {
                    return Ok(Some(s + 1));
                }
            } else if self.src[s] == b {
                cont += 1;
            }
        }
    }

    fn max_expand(&mut self, s: usize, p: usize, ep: usize) -> R<Option<usize>> {
        let mut i = 0;
        while self.single_match(s + i, p, ep)? {
            i += 1;
        }
        loop {
            if let Some(res) = self.do_match(s + i, ep + 1)? {
                return Ok(Some(res));
            }
            if i == 0 {
                return Ok(None);
            }
            i -= 1;
        }
    }

    fn min_expand(&mut self, mut s: usize, p: usize, ep: usize) -> R<Option<usize>> {
        loop {
            if let Some(res) = self.do_match(s, ep + 1)? {
                return Ok(Some(res));
            }
            if self.single_match(s, p, ep)? {
                s += 1;
            } else {
                return Ok(None);
            }
        }
    }

    fn start_capture(&mut self, s: usize, p: usize, what: isize) -> R<Option<usize>> {
        let level = self.level;
        if level >= MAXCAPTURES {
            return error("too many captures");
        }
        self.capture[level] = (s, what);
        self.level = level + 1;
        let res = self.do_match(s, p)?;
        if res.is_none() {
            self.level -= 1;
        }
        Ok(res)
    }

    fn end_capture(&mut self, s: usize, p: usize) -> R<Option<usize>> {
        let l = self.capture_to_close()?;
        self.capture[l].1 = isize::try_from(s - self.capture[l].0).unwrap_or(isize::MAX);
        let res = self.do_match(s, p)?;
        if res.is_none() {
            self.capture[l].1 = CAP_UNFINISHED;
        }
        Ok(res)
    }

    fn capture_to_close(&self) -> R<usize> {
        (0..self.level)
            .rev()
            .find(|&l| self.capture[l].1 == CAP_UNFINISHED)
            .map_or_else(|| error("invalid pattern capture"), Ok)
    }

    fn check_capture(&self, l: u8) -> R<usize> {
        let l = i32::from(l) - i32::from(b'1');
        match usize::try_from(l) {
            Ok(i) if i < self.level && self.capture[i].1 != CAP_UNFINISHED => Ok(i),
            _ => error(format!("invalid capture index %{}", l + 1)),
        }
    }

    fn match_capture(&mut self, s: usize, l: u8) -> R<Option<usize>> {
        let l = self.check_capture(l)?;
        let (init, len) = self.capture[l];
        let Ok(len) = usize::try_from(len) else {
            return Ok(None); // a position capture never matches text
        };
        Ok(
            (self.src.len() - s >= len && self.src[init..init + len] == self.src[s..s + len])
                .then_some(s + len),
        )
    }

    /// Lua's `match`: the end of the match of `pat[p..]` at `s`, if any.
    fn do_match(&mut self, mut s: usize, mut p: usize) -> R<Option<usize>> {
        if self.matchdepth == 0 {
            return error("pattern too complex");
        }
        self.matchdepth -= 1;
        let result = loop {
            self.tick()?;
            if p == self.pat.len() {
                break Some(s);
            }
            match self.p(p) {
                b'(' => {
                    break if self.p(p + 1) == b')' {
                        self.start_capture(s, p + 2, CAP_POSITION)?
                    } else {
                        self.start_capture(s, p + 1, CAP_UNFINISHED)?
                    };
                }
                b')' => break self.end_capture(s, p + 1)?,
                b'$' if p + 1 == self.pat.len() => {
                    break (s == self.src.len()).then_some(s);
                }
                L_ESC if self.p(p + 1) == b'b' => match self.match_balance(s, p + 2)? {
                    Some(next) => {
                        s = next;
                        p += 4;
                    }
                    None => break None,
                },
                L_ESC if self.p(p + 1) == b'f' => {
                    p += 2;
                    if self.p(p) != b'[' {
                        return error("missing '[' after '%f' in pattern");
                    }
                    let ep = self.class_end(p)?;
                    let previous = if s == 0 { 0 } else { self.s(s - 1) };
                    if !self.match_bracket_class(previous, p, ep - 1)
                        && self.match_bracket_class(self.s(s), p, ep - 1)
                    {
                        p = ep;
                    } else {
                        break None;
                    }
                }
                L_ESC if self.p(p + 1).is_ascii_digit() => {
                    match self.match_capture(s, self.p(p + 1))? {
                        Some(next) => {
                            s = next;
                            p += 2;
                        }
                        None => break None,
                    }
                }
                _ => {
                    let ep = self.class_end(p)?;
                    let suffix = self.p(ep);
                    if !self.single_match(s, p, ep)? {
                        if matches!(suffix, b'*' | b'?' | b'-') {
                            p = ep + 1;
                            continue;
                        }
                        break None;
                    }
                    match suffix {
                        b'?' => {
                            if let Some(res) = self.do_match(s + 1, ep + 1)? {
                                break Some(res);
                            }
                            p = ep + 1;
                        }
                        b'+' => break self.max_expand(s + 1, p, ep)?,
                        b'*' => break self.max_expand(s, p, ep)?,
                        b'-' => break self.min_expand(s, p, ep)?,
                        _ => {
                            s += 1;
                            p = ep;
                        }
                    }
                }
            }
        };
        self.matchdepth += 1;
        Ok(result)
    }

    /// Capture `i` of the match `s..e` (Lua's `get_onecapture`).
    fn capture_value(&self, lua: &Lua, i: usize, s: usize, e: usize) -> R<Value> {
        if i >= self.level {
            if i != 0 {
                return error(format!("invalid capture index %{}", i + 1));
            }
            return Ok(Value::String(lua.create_string(&self.src[s..e])?));
        }
        let (init, len) = self.capture[i];
        match len {
            CAP_UNFINISHED => error("unfinished capture"),
            CAP_POSITION => Ok(Value::Integer(i64::try_from(init + 1).unwrap_or(i64::MAX))),
            len => {
                let len = usize::try_from(len).unwrap_or(0);
                Ok(Value::String(
                    lua.create_string(&self.src[init..init + len])?,
                ))
            }
        }
    }

    /// All captures, or the whole match when there are none and `s` is
    /// given (Lua's `push_captures`).
    fn captures(&self, lua: &Lua, s: Option<usize>, e: usize) -> R<Vec<Value>> {
        let n = if self.level == 0 && s.is_some() {
            1
        } else {
            self.level
        };
        (0..n)
            .map(|i| self.capture_value(lua, i, s.unwrap_or(0), e))
            .collect()
    }
}

fn match_class(c: u8, cl: u8) -> bool {
    let res = match cl.to_ascii_lowercase() {
        b'a' => c.is_ascii_alphabetic(),
        b'c' => c.is_ascii_control(),
        b'd' => c.is_ascii_digit(),
        b'g' => c.is_ascii_graphic(),
        b'l' => c.is_ascii_lowercase(),
        b'p' => c.is_ascii_punctuation(),
        b's' => matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'),
        b'u' => c.is_ascii_uppercase(),
        b'w' => c.is_ascii_alphanumeric(),
        b'x' => c.is_ascii_hexdigit(),
        b'z' => c == 0, // deprecated in Lua 5.2, still accepted by 5.4
        _ => return cl == c,
    };
    if cl.is_ascii_uppercase() { !res } else { res }
}

/// Lua's `posrelatI`: a 1-based start from a possibly negative position.
fn posrelat(pos: i64, len: usize) -> usize {
    let len = i64::try_from(len).unwrap_or(i64::MAX);
    let pos = if pos > 0 {
        pos
    } else if pos == 0 || pos < -len {
        1
    } else {
        len + pos + 1
    };
    usize::try_from(pos).unwrap_or(usize::MAX)
}

fn find_plain(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn deadline_of(deadline: &Arc<Mutex<Option<Instant>>>) -> Option<Instant> {
    deadline.lock().ok().and_then(|d| *d)
}

fn int(n: usize) -> Value {
    Value::Integer(i64::try_from(n).unwrap_or(i64::MAX))
}

fn find_aux(
    lua: &Lua,
    deadline: Option<Instant>,
    args: (LuaString, LuaString, Option<i64>, Option<Value>),
    find: bool,
) -> R<MultiValue> {
    let (s, p, init, plain) = args;
    let (s, p) = (s.as_bytes(), p.as_bytes());
    let (s, p): (&[u8], &[u8]) = (&s, &p);
    let init = posrelat(init.unwrap_or(1), s.len()) - 1;
    if init > s.len() {
        return Ok(MultiValue::from_iter([Value::Nil]));
    }
    let plain = plain.is_some_and(|v| !matches!(v, Value::Nil | Value::Boolean(false)));
    if find && (plain || !p.iter().any(|c| SPECIALS.contains(c))) {
        if let Some(at) = find_plain(&s[init..], p) {
            let start = init + at;
            return Ok(MultiValue::from_iter([
                int(start + 1),
                int(start + p.len()),
            ]));
        }
        return Ok(MultiValue::from_iter([Value::Nil]));
    }
    let anchor = p.first() == Some(&b'^');
    let p = if anchor { &p[1..] } else { p };
    let mut ms = MatchState::new(s, p, deadline);
    let mut s1 = init;
    loop {
        ms.reprep();
        if let Some(e) = ms.do_match(s1, 0)? {
            if find {
                let mut out = vec![int(s1 + 1), int(e)];
                out.extend(ms.captures(lua, None, 0)?);
                return Ok(MultiValue::from_iter(out));
            }
            return Ok(MultiValue::from_iter(ms.captures(lua, Some(s1), e)?));
        }
        s1 += 1;
        if s1 > s.len() || anchor {
            break;
        }
    }
    Ok(MultiValue::from_iter([Value::Nil]))
}

fn add_string(
    lua: &Lua,
    ms: &MatchState<'_>,
    out: &mut Vec<u8>,
    s: usize,
    e: usize,
    news: &[u8],
) -> R<()> {
    let mut i = 0;
    while i < news.len() {
        let c = news[i];
        i += 1;
        if c != L_ESC {
            out.push(c);
            continue;
        }
        let d = news.get(i).copied().unwrap_or(0);
        i += 1;
        if d == L_ESC {
            out.push(L_ESC);
        } else if d == b'0' {
            out.extend_from_slice(&ms.src[s..e]);
        } else if d.is_ascii_digit() {
            match ms.capture_value(lua, usize::from(d - b'1'), s, e)? {
                Value::String(cap) => out.extend_from_slice(&cap.as_bytes()),
                Value::Integer(n) => out.extend_from_slice(n.to_string().as_bytes()),
                _ => {}
            }
        } else {
            return error("invalid use of '%' in replacement string");
        }
    }
    Ok(())
}

fn gsub(
    lua: &Lua,
    deadline: Option<Instant>,
    (src, p, repl, max): (LuaString, LuaString, Value, Option<i64>),
) -> R<MultiValue> {
    let original = src.clone();
    let (src, p) = (src.as_bytes(), p.as_bytes());
    let (src, p): (&[u8], &[u8]) = (&src, &p);
    if !matches!(
        repl,
        Value::Integer(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::Function(_)
            | Value::Table(_)
    ) {
        return error(format!(
            "bad argument #3 to 'gsub' (string/function/table expected, got {})",
            repl.type_name()
        ));
    }
    let max = max.unwrap_or_else(|| i64::try_from(src.len() + 1).unwrap_or(i64::MAX));
    let anchor = p.first() == Some(&b'^');
    let p = if anchor { &p[1..] } else { p };
    let repl_text = match &repl {
        Value::Integer(_) | Value::Number(_) | Value::String(_) => lua
            .coerce_string(repl.clone())?
            .map(|s| s.as_bytes().to_vec()),
        _ => None,
    };
    let mut ms = MatchState::new(src, p, deadline);
    let mut out = Vec::new();
    let mut s = 0;
    let mut lastmatch: Option<usize> = None;
    let mut n = 0;
    let mut changed = false;
    while n < max {
        ms.reprep();
        let e = ms.do_match(s, 0)?;
        if let Some(e) = e.filter(|e| Some(*e) != lastmatch) {
            n += 1;
            let value = match &repl {
                Value::Function(f) => {
                    Some(f.call::<Value>(MultiValue::from_iter(ms.captures(lua, Some(s), e)?))?)
                }
                Value::Table(t) => Some(t.get::<Value>(ms.capture_value(lua, 0, s, e)?)?),
                _ => {
                    add_string(
                        lua,
                        &ms,
                        &mut out,
                        s,
                        e,
                        repl_text.as_deref().unwrap_or(&[]),
                    )?;
                    changed = true;
                    None
                }
            };
            if let Some(value) = value {
                match value {
                    Value::Nil | Value::Boolean(false) => out.extend_from_slice(&src[s..e]),
                    v => match lua.coerce_string(v.clone())? {
                        Some(text) => {
                            out.extend_from_slice(&text.as_bytes());
                            changed = true;
                        }
                        None => {
                            return error(format!(
                                "invalid replacement value (a {})",
                                v.type_name()
                            ));
                        }
                    },
                }
            }
            s = e;
            lastmatch = Some(e);
        } else if s < src.len() {
            out.push(src[s]);
            s += 1;
        } else {
            break;
        }
        if anchor {
            break;
        }
    }
    let result = if changed {
        out.extend_from_slice(&src[s..]);
        Value::String(lua.create_string(out)?)
    } else {
        Value::String(original)
    };
    Ok(MultiValue::from_iter([result, Value::Integer(n)]))
}

struct GMatch {
    src: Vec<u8>,
    pat: Vec<u8>,
    pos: usize,
    lastmatch: Option<usize>,
}

/// Raises the `false, message` results of the raw functions as Lua errors
/// at the caller's level, like `luaL_error` does.
const WRAPPER: &str = r#"
local find, match, gsub, gmatch = ...
local pack, unpack, error = table.pack, table.unpack, error
local function wrap(f)
    return function(...)
        local r = pack(f(...))
        if r[1] then return unpack(r, 2, r.n) end
        error(r[2], 2)
    end
end
string.find, string.match, string.gsub = wrap(find), wrap(match), wrap(gsub)
string.gmatch = function(...)
    local r = pack(gmatch(...))
    if not r[1] then error(r[2], 2) end
    return wrap(r[2])
end
"#;

/// Replaces `string.find`, `match`, `gmatch` and `gsub` in `lua` with the
/// budgeted versions; `deadline` is the plugin's call deadline.
pub fn install(lua: &Lua, deadline: Arc<Mutex<Option<Instant>>>) -> mlua::Result<()> {
    let d = Arc::clone(&deadline);
    let find = lua.create_function(move |lua, args| {
        finish(lua, find_aux(lua, deadline_of(&d), args, true))
    })?;
    let d = Arc::clone(&deadline);
    let matcher = lua.create_function(
        move |lua, (s, p, init): (LuaString, LuaString, Option<i64>)| {
            finish(
                lua,
                find_aux(lua, deadline_of(&d), (s, p, init, None), false),
            )
        },
    )?;
    let d = Arc::clone(&deadline);
    let gsub =
        lua.create_function(move |lua, args| finish(lua, gsub(lua, deadline_of(&d), args)))?;
    let d = deadline;
    let gmatch = lua.create_function(
        move |lua, (s, p, init): (LuaString, LuaString, Option<i64>)| {
            let src = s.as_bytes().to_vec();
            let mut pos = posrelat(init.unwrap_or(1), src.len()) - 1;
            if pos > src.len() {
                pos = src.len() + 1;
            }
            let state = Rc::new(RefCell::new(GMatch {
                src,
                pat: p.as_bytes().to_vec(),
                pos,
                lastmatch: None,
            }));
            let d = Arc::clone(&d);
            let iterator = lua.create_function(move |lua, ()| {
                let mut g = state.borrow_mut();
                let GMatch {
                    src,
                    pat,
                    pos,
                    lastmatch,
                } = &mut *g;
                let mut ms = MatchState::new(src, pat, deadline_of(&d));
                let mut start = *pos;
                let found = loop {
                    if start > src.len() {
                        break None;
                    }
                    ms.reprep();
                    match ms.do_match(start, 0) {
                        Ok(Some(e)) if Some(e) != *lastmatch => break Some(Ok((start, e))),
                        Ok(_) => start += 1,
                        Err(fail) => break Some(Err(fail)),
                    }
                };
                *pos = start;
                finish(
                    lua,
                    match found {
                        None => Ok(MultiValue::new()),
                        Some(Err(fail)) => Err(fail),
                        Some(Ok((start, e))) => {
                            *pos = e;
                            *lastmatch = Some(e);
                            ms.captures(lua, Some(start), e).map(MultiValue::from_iter)
                        }
                    },
                )
            })?;
            Ok(MultiValue::from_iter([
                Value::Boolean(true),
                Value::Function(iterator),
            ]))
        },
    )?;
    lua.load(WRAPPER)
        .set_name("=ferny patterns")
        .call::<()>((find, matcher, gsub, gmatch))
}
