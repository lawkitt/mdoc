//! `mdoc-mcp`: MCP on stdio for AI apps, forwarded to the running mdoc.
//! One message per line each way; it ends when stdin does.
use mdoc_mcp::{forward::Forwarder, mcp::Server};
use std::io::Write;

fn main() {
    let mut server = Server::new(Forwarder::from_env());
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    loop {
        let line = match mdoc_mcp::ipc::read_message(&mut stdin) {
            Ok(Some(line)) => line,
            Ok(None) => return,
            Err(error) => {
                eprintln!("mdoc-mcp: {error}");
                return;
            }
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if let Some(answer) = server.handle(&line)
            && writeln!(stdout, "{answer}")
                .and_then(|()| stdout.flush())
                .is_err()
        {
            return;
        }
    }
}
