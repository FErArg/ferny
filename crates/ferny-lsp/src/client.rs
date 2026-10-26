//! One running language server: spawn, `initialize` handshake, document
//! notifications, diagnostics, shutdown (H3.1a).
//!
//! The server is untrusted output-wise: it is read on its own thread with
//! bounded frames; its requests get empty answers; it is killed if it does
//! not answer `initialize` in time or does not exit after `shutdown`.

use std::ffi::OsString;
use std::io::{BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::{Diagnostic, parse_diagnostics, read_message, write_message};

/// Time a server has to answer `initialize`.
pub const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);
/// Time a server has to exit after `shutdown`.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// What a server reports, delivered from its reader thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// `initialize` answered; documents can be sent.
    Ready,
    /// Diagnostics of one document.
    Diagnostics(String, Vec<Diagnostic>),
    /// The server exited (or was killed); `true` if unexpectedly.
    Exited(bool),
    /// The answer to a request (H3.1b): its id and `result` (null on an
    /// error answer).
    Response(i64, Value),
}

/// A running server.
pub struct Server {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Child>>,
    ready: Arc<AtomicBool>,
    utf8: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    pending: Arc<Mutex<Vec<Vec<u8>>>>,
    next_id: AtomicI64,
}

fn send(stdin: &Mutex<Option<ChildStdin>>, body: &[u8]) {
    if let Ok(mut guard) = stdin.lock()
        && let Some(pipe) = guard.as_mut()
        && write_message(pipe, body).is_err()
    {
        *guard = None; // the server is gone
    }
}

fn message(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_default()
}

impl Server {
    /// Starts `program` with `args` in `root` (the workspace) and the given
    /// environment only, and sends `initialize`. `on_event` runs on the
    /// reader thread.
    pub fn spawn(
        program: &Path,
        args: &[String],
        root: &Path,
        env: Vec<(OsString, OsString)>,
        on_event: impl Fn(Event) + Send + 'static,
    ) -> std::io::Result<Self> {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(root)
            .env_clear()
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Server logs may quote file contents: never kept (T10).
            .stderr(Stdio::null());
        // CREATE_NO_WINDOW (winbase.h): a console server gets no window of its own.
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000);
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("no stdout"))?;
        let stdin = Arc::new(Mutex::new(child.stdin.take()));
        let server = Self {
            stdin,
            child: Arc::new(Mutex::new(child)),
            ready: Arc::new(AtomicBool::new(false)),
            utf8: Arc::new(AtomicBool::new(false)),
            stopping: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(Mutex::new(Vec::new())),
            next_id: AtomicI64::new(2),
        };
        send(
            &server.stdin,
            &message(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "processId": std::process::id(),
                    "rootUri": crate::path_to_uri(root),
                    "capabilities": {
                        "general": { "positionEncodings": ["utf-8", "utf-16"] },
                        "textDocument": {
                            "synchronization": { "didSave": true },
                            "publishDiagnostics": {},
                            // Plain text only: no snippet syntax to expand.
                            "completion": { "completionItem": { "snippetSupport": false } },
                            "hover": { "contentFormat": ["plaintext", "markdown"] },
                            "signatureHelp": {
                                "signatureInformation": {
                                    "parameterInformation": { "labelOffsetSupport": true }
                                }
                            },
                            "definition": {},
                            "references": {}
                        }
                    },
                    "clientInfo": { "name": "Ferny" }
                }
            })),
        );

        // Reader thread.
        let (stdin, ready, utf8, pending, stopping) = (
            Arc::clone(&server.stdin),
            Arc::clone(&server.ready),
            Arc::clone(&server.utf8),
            Arc::clone(&server.pending),
            Arc::clone(&server.stopping),
        );
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(body) = read_message(&mut reader) {
                let Ok(value) = serde_json::from_slice::<Value>(&body) else {
                    continue; // not JSON: ignored
                };
                let method = value.get("method").and_then(Value::as_str);
                match (value.get("id"), method) {
                    // The answer to initialize.
                    (Some(id), None) if id.as_i64() == Some(1) && !ready.load(Ordering::SeqCst) => {
                        let encoding = value
                            .pointer("/result/capabilities/positionEncoding")
                            .and_then(Value::as_str);
                        utf8.store(encoding == Some("utf-8"), Ordering::SeqCst);
                        send(
                            &stdin,
                            &message(&json!({
                                "jsonrpc": "2.0", "method": "initialized", "params": {}
                            })),
                        );
                        ready.store(true, Ordering::SeqCst);
                        let queued = pending.lock().map(|mut p| std::mem::take(&mut *p));
                        for body in queued.unwrap_or_default() {
                            send(&stdin, &body);
                        }
                        on_event(Event::Ready);
                    }
                    // A request from the server: an empty answer, so it
                    // does not wait (configuration gets one null per item).
                    (Some(id), Some(method)) => {
                        let result = if method == "workspace/configuration" {
                            let n = value
                                .pointer("/params/items")
                                .and_then(Value::as_array)
                                .map_or(0, Vec::len)
                                .min(64);
                            Value::Array(vec![Value::Null; n])
                        } else {
                            Value::Null
                        };
                        send(
                            &stdin,
                            &message(&json!({
                                "jsonrpc": "2.0", "id": id, "result": result
                            })),
                        );
                    }
                    // The answer to one of Ferny's requests.
                    (Some(id), None) => {
                        if let Some(id) = id.as_i64() {
                            let result = value.get("result").cloned().unwrap_or(Value::Null);
                            on_event(Event::Response(id, result));
                        }
                    }
                    (None, Some("textDocument/publishDiagnostics")) => {
                        if let Some((uri, items)) = value.get("params").and_then(parse_diagnostics)
                        {
                            on_event(Event::Diagnostics(uri, items));
                        }
                    }
                    _ => {}
                }
            }
            on_event(Event::Exited(!stopping.load(Ordering::SeqCst)));
        });

        // A server that never answers initialize is killed.
        let (ready, child) = (Arc::clone(&server.ready), Arc::clone(&server.child));
        std::thread::spawn(move || {
            std::thread::sleep(INITIALIZE_TIMEOUT);
            if !ready.load(Ordering::SeqCst)
                && let Ok(mut child) = child.lock()
            {
                let _ = child.kill();
            }
        });
        Ok(server)
    }

    /// Whether positions are in UTF-8 bytes (negotiated), else UTF-16.
    pub fn utf8_positions(&self) -> bool {
        self.utf8.load(Ordering::SeqCst)
    }

    /// Whether `initialize` was answered.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// Sends a notification (queued until the server is ready).
    pub fn notify(&self, method: &str, params: Value) {
        let body = message(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
        if self.ready.load(Ordering::SeqCst) {
            send(&self.stdin, &body);
        } else if let Ok(mut pending) = self.pending.lock() {
            pending.push(body);
        }
    }

    /// Sends a request; returns its id (the answer arrives as
    /// `Event::Response`). Queued until the server is ready.
    pub fn request(&self, method: &str, params: Value) -> i64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let body =
            message(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        if self.ready.load(Ordering::SeqCst) {
            send(&self.stdin, &body);
        } else if let Ok(mut pending) = self.pending.lock() {
            pending.push(body);
        }
        id
    }

    /// `textDocument/didOpen`.
    pub fn did_open(&self, uri: &str, language: &str, version: i64, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": {
                "uri": uri, "languageId": language, "version": version, "text": text
            }}),
        );
    }

    /// `textDocument/didChange` with the whole text.
    pub fn did_change(&self, uri: &str, version: i64, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }),
        );
    }

    /// `textDocument/didSave`.
    pub fn did_save(&self, uri: &str) {
        self.notify(
            "textDocument/didSave",
            json!({ "textDocument": { "uri": uri } }),
        );
    }

    /// `textDocument/didClose`.
    pub fn did_close(&self, uri: &str) {
        self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        );
    }

    /// `shutdown`, `exit`, and a kill if it is still running after
    /// `SHUTDOWN_TIMEOUT`.
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        send(
            &self.stdin,
            &message(&json!({ "jsonrpc": "2.0", "id": id, "method": "shutdown" })),
        );
        send(
            &self.stdin,
            &message(&json!({ "jsonrpc": "2.0", "method": "exit" })),
        );
        if let Ok(mut stdin) = self.stdin.lock() {
            if let Some(pipe) = stdin.as_mut() {
                let _ = pipe.flush();
            }
            *stdin = None; // closes the pipe
        }
        let child = Arc::clone(&self.child);
        std::thread::spawn(move || {
            std::thread::sleep(SHUTDOWN_TIMEOUT);
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
        });
    }
}
