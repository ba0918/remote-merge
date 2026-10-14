use remote_merge_core::scan::outcome::{ScanRequest, ScanScope, Start};
use remote_merge_protocol::{
    deserialize_response, framing, serialize_request, AgentRequest as R, AgentResponse,
};
use std::io::{BufRead, BufReader, Read, Write};

fn reqs() -> Vec<R> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    vec![
        R::Ping,
        R::ListTree { request: ScanRequest { scope: ScanScope::Whole, max_entries: 1000 }, exclude: vec![] },
        R::ReadFiles { paths: s(&["a.txt", "sub/b.txt", "missing.txt", "../etc/passwd", "link_to_a"]), chunk_size_limit: 1 << 20, size_limit: None },
        R::ReadFiles { paths: s(&["big.bin"]), chunk_size_limit: 1 << 20, size_limit: None },
        R::HashFiles { paths: s(&["a.txt", "link_to_a", "big.bin", "missing.txt"]) },
        R::HashContents { paths: s(&["a.txt", "link_to_a", "big.bin"]) },
        R::InspectPath { path: "link_to_a".into() },
        R::InspectPaths { paths: s(&["a.txt", "sub", "nope/x", "a.txt/x", "../x"]) },
        R::WriteFile { path: "sub/new.txt".into(), content: b"new content\n".to_vec(), is_binary: false, more_to_follow: false },
        R::ReadFiles { paths: s(&["sub/new.txt"]), chunk_size_limit: 1 << 20, size_limit: None },
        R::Symlink { path: "newlink".into(), target: "sub/new.txt".into() },
        R::ReadEntryLink { path: "newlink".into() },
        R::Chmod { path: "sub/new.txt".into(), mode: 0o600 },
        R::ListTree { request: ScanRequest { scope: ScanScope::Children("sub".into()), max_entries: 1000 }, exclude: vec![] },
        R::Remove { path: "newlink".into() },
        R::Remove { path: "sub/new.txt".into() },
        R::HashFiles { paths: s(&["sub/new.txt"]) },
        R::Ping,
        R::Shutdown,
    ]
}


fn reqs2() -> Vec<R> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let lt = |scope: ScanScope, max: usize, ex: &[&str]| R::ListTree { request: ScanRequest { scope, max_entries: max }, exclude: s(ex) };
    let st = |p: &str| ScanScope::Starts(vec![Start::value(p)]);
    let rf = |p: &[&str], lim: Option<u64>| R::ReadFiles { paths: s(p), chunk_size_limit: 1 << 20, size_limit: lim };
    let wf = |p: &str, c: &[u8], more: bool| R::WriteFile { path: p.into(), content: c.to_vec(), is_binary: false, more_to_follow: more };
    vec![
        lt(st("many"), 100000, &[]),
        lt(ScanScope::Whole, 100000, &["many", "cyc"]),
        lt(st("cyc"), 100000, &[]),
        lt(st("noperm"), 100000, &[]),
        lt(ScanScope::Children("linkdir".into()), 100000, &[]),
        lt(ScanScope::Whole, 10, &[]),
        wf("new/a/b/c.txt", b"part1-", true),
        wf("new/a/b/c.txt", b"part2\n", false),
        wf("existing.txt", b"short\n", false),
        rf(&["existing.txt", "new/a/b/c.txt", "noperm/s.txt", "fifo"], Some(1000)),
        rf(&["existing.txt"], Some(3)),
        R::HashFiles { paths: s(&["noperm/s.txt", "fifo", "linkdir"]) },
        R::HashContents { paths: s(&["linkdir/in.txt", "fifo"]) },
        R::InspectPath { path: "fifo".into() },
        R::InspectPaths { paths: s(&["linkdir", "linkdir/in.txt", "noperm/s.txt", "cyc/up/cyc"]) },
        R::Chmod { path: "linkdir".into(), mode: 0o700 },
        R::Chmod { path: "existing.txt".into(), mode: 0o640 },
        R::Symlink { path: "existing.txt".into(), target: "x".into() },
        R::Symlink { path: "new/l".into(), target: "../existing.txt".into() },
        R::ReadEntryLink { path: "new/l".into() },
        R::ReadEntryLink { path: "existing.txt".into() },
        R::Remove { path: "realdir/in.txt".into() },
        R::Remove { path: "many".into() },
        R::Remove { path: "new/l".into() },
        lt(ScanScope::Children("new".into()), 100000, &[]),
        lt(ScanScope::Whole, 100000, &["many", "cyc"]),
        R::Shutdown,
    ]
}

fn print_lines(response: &anyhow::Result<AgentResponse>) {
    match response {
        Ok(AgentResponse::TreeChunk {
            entries,
            is_last,
            unreadable_names,
            truncated,
            cycle,
            unresolvable_names,
            counted,
        }) => {
            for entry in entries {
                println!("ENTRY {entry:?}");
            }
            if *is_last {
                println!("TREE END unreadable={unreadable_names:?} truncated={truncated} cycle={cycle:?} unresolvable={unresolvable_names:?} counted={counted}");
            }
        }
        other => println!("RESP {other:?}"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args[1].as_str() {
        "gen" => {
            let mut out = std::io::stdout().lock();
            for r in reqs() {
                framing::write_frame(&mut out, &serialize_request(&r).unwrap()).unwrap();
            }
        }
        "gen2" => {
            let mut out = std::io::stdout().lock();
            for r in reqs2() {
                framing::write_frame(&mut out, &serialize_request(&r).unwrap()).unwrap();
            }
        }
        "decode" => {
            let mut rd = BufReader::new(std::fs::File::open(&args[2]).unwrap());
            let mut line = String::new();
            rd.read_line(&mut line).unwrap();
            println!("HANDSHAKE {:?}", line);
            let mut n = 0;
            loop {
                match framing::read_frame(&mut rd) {
                    Ok(f) => {
                        n += 1;
                        let response = deserialize_response(&f);
                        // LINES を付けると、走査の一覧を一項目ずつの行にする（枠の切れ目は更新時刻の
                        // 長さで変わるので、比べるときに切れ目を見ない）
                        if std::env::var_os("LINES").is_some() {
                            print_lines(&response);
                            continue;
                        }
                        let mut t = format!("{:?}", response);
                        if t.len() > 1500 && std::env::var_os("FULL").is_none() { t.truncate(1500); t.push_str("...<truncated>"); }
                        println!("RESP#{n} len={} {t}", f.len());
                    }
                    Err(e) => { println!("END {:?}", e.kind()); break; }
                }
            }
            let mut rest = Vec::new();
            rd.read_to_end(&mut rest).unwrap();
            println!("TRAILING_BYTES {}", rest.len());
            std::io::stdout().flush().unwrap();
        }
        _ => panic!("gen|decode"),
    }
}
