//! Fuzz target: Ferny's budgeted Lua patterns against Lua 5.4's C ones
//! (phase 3 audit, F1). Same results and errors for find, match, gsub and
//! gmatch; inputs are kept small so the C matcher always finishes.
#![no_main]

use std::sync::{Arc, Mutex};

use libfuzzer_sys::fuzz_target;
use mlua::{Function, Lua, MultiValue, Value};

thread_local! {
    static STATES: (Lua, Lua) = {
        let c = Lua::new();
        let rust = Lua::new();
        ferny_plugin::pattern::install(&rust, Arc::new(Mutex::new(None))).expect("install");
        (c, rust)
    };
}

fn show(result: mlua::Result<MultiValue>) -> String {
    match result {
        Ok(values) => values
            .iter()
            .map(|v| match v {
                Value::String(s) => format!("s:{:?}", s.as_bytes().to_vec()),
                Value::Integer(n) => format!("i:{n}"),
                Value::Nil => "nil".to_owned(),
                Value::Boolean(b) => format!("b:{b}"),
                other => format!("{}", other.type_name()),
            })
            .collect::<Vec<_>>()
            .join("|"),
        // Errors: the message only (both are plain strings).
        Err(err) => format!("error:{}", err.to_string().lines().next().unwrap_or("")),
    }
}

fn run(lua: &Lua, op: u8, subject: &[u8], pattern: &[u8], init: i64) -> String {
    let string: mlua::Table = lua.globals().get("string").expect("string");
    let (s, p) = (
        lua.create_string(subject).expect("s"),
        lua.create_string(pattern).expect("p"),
    );
    match op % 4 {
        0 => show(string.get::<Function>("find").expect("f").call((s, p, init))),
        1 => show(string.get::<Function>("match").expect("f").call((s, p, init))),
        2 => show(string.get::<Function>("gsub").expect("f").call((s, p, "<%0>"))),
        _ => {
            let iter: mlua::Result<Function> = string.get::<Function>("gmatch").expect("f").call((s, p));
            match iter {
                Err(e) => show(Err(e)),
                Ok(iter) => {
                    let mut out = Vec::new();
                    for _ in 0..64 {
                        let r: mlua::Result<MultiValue> = iter.call(());
                        let done = matches!(&r, Ok(v) if v.is_empty()) || r.is_err();
                        out.push(show(r));
                        if done {
                            break;
                        }
                    }
                    out.join(";")
                }
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let [op, init, plen, rest @ ..] = data else {
        return;
    };
    let plen = usize::from(*plen % 13).min(rest.len());
    let (pattern, subject) = rest.split_at(plen);
    // Small inputs and few repetitions: the C matcher has no time limit.
    let repeats = pattern.iter().filter(|c| b"*+-?".contains(c)).count();
    if subject.len() > 48 || repeats > 3 {
        return;
    }
    let init = i64::from(*init as i8);
    STATES.with(|(c, rust)| {
        let expected = run(c, *op, subject, pattern, init);
        let actual = run(rust, *op, subject, pattern, init);
        assert_eq!(expected, actual, "op {op} pattern {pattern:?} subject {subject:?} init {init}");
    });
});
