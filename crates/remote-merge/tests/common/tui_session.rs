use std::process::Command;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use expectrl::Expect;

pub struct DrainingSession {
    requests: Sender<Request>,
    responses: Receiver<Result<(), String>>,
    worker: Option<JoinHandle<Result<(), String>>>,
}

enum Request {
    Send(String),
    Expect(String),
    Exit,
    Stop,
}

impl Request {
    fn label(&self) -> String {
        match self {
            Self::Send(keys) => format!("send {keys:?}"),
            Self::Expect(text) => format!("expect {text:?}"),
            Self::Exit => "expect EOF".to_owned(),
            Self::Stop => "stop".to_owned(),
        }
    }
}

impl DrainingSession {
    pub fn spawn(command: Command) -> Self {
        let mut session = expectrl::Session::spawn(command).unwrap();
        session.get_process_mut().set_window_size(200, 50).unwrap();
        let (requests, incoming) = mpsc::channel();
        let (outgoing, responses) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut session = expectrl::session::log(session, std::io::stderr())
                .map_err(|error| format!("set up PTY logging: {error}"))?;
            session.set_expect_timeout(Some(Duration::from_secs(15)));
            let mut run = || -> Result<(), String> {
                let mut eof = false;
                loop {
                    match incoming.recv_timeout(Duration::from_millis(5)) {
                        Ok(Request::Stop) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
                        Ok(request) => {
                            let label = request.label();
                            eprintln!("PTY request: {label}");
                            let result = match request {
                                Request::Send(keys) => session.send(keys).map(|_| ()),
                                Request::Expect(text) => session.expect(text.as_str()).map(|_| ()),
                                Request::Exit => session.expect(expectrl::Eof).map(|_| ()),
                                Request::Stop => unreachable!(),
                            }
                            .map_err(|error| format!("{label}: {error}"));
                            let failed = result.is_err();
                            if outgoing.send(result.clone()).is_err() {
                                return result;
                            }
                            if failed {
                                return result;
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                    if !eof {
                        // Reading directly would consume bytes needed by later expect calls.
                        eof = session
                            .is_matched(expectrl::Eof)
                            .map_err(|error| format!("idle output read / EOF check: {error}"))?;
                    }
                }
            };
            let result = run();
            if let Err(error) = &result {
                let _ = outgoing.send(Err(error.clone()));
            }
            result
        });
        Self {
            requests,
            responses,
            worker: Some(worker),
        }
    }

    pub fn send(&mut self, keys: &str) {
        self.request(Request::Send(keys.to_owned()));
    }

    pub fn expect(&mut self, text: &str) {
        self.request(Request::Expect(text.to_owned()));
    }

    pub fn expect_exit(&mut self) {
        self.request(Request::Exit);
    }

    fn request(&self, request: Request) {
        let label = request.label();
        if self.requests.send(request).is_err() {
            panic!(
                "PTY worker stopped during {label}: {:?}",
                self.responses.try_recv()
            );
        }
        self.responses
            .recv_timeout(Duration::from_secs(16))
            .unwrap_or_else(|error| panic!("PTY worker did not respond during {label}: {error}"))
            .unwrap_or_else(|error| panic!("PTY operation failed during {label}: {error}"));
    }
}

pub fn wait_for_state(
    path: &std::path::Path,
    expected: &str,
    matches: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let diagnostic = match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(state) if matches(&state) => {
                    eprintln!("TUI state matched {expected}: {state}");
                    return state;
                }
                Ok(state) => format!("last state: {state}"),
                Err(error) => format!("last JSON error: {error}"),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                "state file not yet created".to_owned()
            }
            Err(error) => panic!("reading TUI state while expecting {expected}: {error}"),
        };
        assert!(
            std::time::Instant::now() < deadline,
            "timed out expecting TUI state {expected}; {diagnostic}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

impl Drop for DrainingSession {
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Stop);
        let result = self.worker.take().unwrap().join();
        if !thread::panicking() {
            result
                .expect("PTY worker panicked")
                .expect("PTY reader failed");
        }
    }
}
