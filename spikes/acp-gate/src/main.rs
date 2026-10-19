//! Stack spike: proves the Rust ACP SDK can drive a real harness and gate its
//! actions before they happen. One file is "reserved" by another member; the
//! gate denies any proposed edit to it and allows the rest.
//!
//! Usage: acp-gate "<agent command>" <workspace> <reserved relative path> "<prompt>"

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    ClientCapabilities, ContentBlock, FileSystemCapabilities, InitializeRequest, NewSessionRequest,
    PermissionOptionKind, PromptRequest, ReadTextFileRequest, ReadTextFileResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, TextContent, ToolKind,
    WriteTextFileRequest, WriteTextFileResponse,
};
use agent_client_protocol::{AcpAgent, Agent, ConnectionTo};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

type Log = Arc<Mutex<Vec<String>>>;

fn touches_reserved(paths: &[PathBuf], reserved: &Path) -> bool {
    paths.iter().any(|p| {
        p == reserved
            || p.ends_with(reserved.file_name().unwrap_or_default())
                && p.parent() == reserved.parent()
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let [_, command, workspace, reserved_rel, prompt] = args.as_slice() else {
        eprintln!("usage: acp-gate \"<agent command>\" <workspace> <reserved path> \"<prompt>\"");
        std::process::exit(2);
    };
    let workspace = std::fs::canonicalize(workspace)?;
    std::env::set_current_dir(&workspace)?;
    let reserved = workspace.join(reserved_rel);
    let before = std::fs::read_to_string(&reserved).unwrap_or_default();

    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let t0 = Instant::now();
    let stamp = move || format!("{:>6}ms", t0.elapsed().as_millis());

    let agent = AcpAgent::from_str(command)?;
    let (log_n, log_p, log_w, log_r) = (log.clone(), log.clone(), log.clone(), log.clone());
    let (res_p, res_w) = (reserved.clone(), reserved.clone());

    agent_client_protocol::Client
        .builder()
        .on_receive_notification(
            async move |n: SessionNotification, _cx| {
                if let SessionUpdate::ToolCall(tc) = &n.update {
                    log_n.lock().unwrap().push(format!(
                        "{} tool_call   {:?} {}",
                        stamp(),
                        tc.kind,
                        tc.title
                    ));
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |req: RequestPermissionRequest, responder, _conn| {
                let f = &req.tool_call.fields;
                let mut paths: Vec<PathBuf> = f
                    .locations
                    .iter()
                    .flatten()
                    .map(|l| l.path.clone())
                    .collect();
                if let Some(raw) = &f.raw_input {
                    for key in ["file_path", "filePath", "path"] {
                        if let Some(p) = raw.get(key).and_then(|v| v.as_str()) {
                            paths.push(PathBuf::from(p));
                        }
                    }
                }
                let is_write = matches!(
                    f.kind,
                    Some(ToolKind::Edit | ToolKind::Delete | ToolKind::Move)
                );
                let deny = is_write && touches_reserved(&paths, &res_p);
                let want = if deny {
                    PermissionOptionKind::RejectOnce
                } else {
                    PermissionOptionKind::AllowOnce
                };
                let choice = req
                    .options
                    .iter()
                    .find(|o| o.kind == want)
                    .or(req.options.first());
                log_p.lock().unwrap().push(format!(
                    "{} permission  {:?} {:?} -> {}",
                    stamp(),
                    f.kind,
                    paths
                        .iter()
                        .map(|p| p
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string())
                        .collect::<Vec<_>>(),
                    if deny { "DENY (reserved)" } else { "allow" }
                ));
                let outcome = match choice {
                    Some(o) => RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                        o.option_id.clone(),
                    )),
                    None => RequestPermissionOutcome::Cancelled,
                };
                responder.respond(RequestPermissionResponse::new(outcome))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: WriteTextFileRequest, responder, _conn| {
                // Hard gate at the filesystem boundary, in case a write arrives without a permission ask.
                if req.path == res_w {
                    log_w.lock().unwrap().push(format!(
                        "{} fs_write    {} -> BLOCKED",
                        stamp(),
                        req.path.display()
                    ));
                    return responder.respond_with_internal_error("reserved by another member");
                }
                std::fs::write(&req.path, &req.content).ok();
                log_w.lock().unwrap().push(format!(
                    "{} fs_write    {} ({} bytes)",
                    stamp(),
                    req.path.display(),
                    req.content.len()
                ));
                responder.respond(WriteTextFileResponse::new())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: ReadTextFileRequest, responder, _conn| {
                log_r.lock().unwrap().push(format!(
                    "{} fs_read     {}",
                    stamp(),
                    req.path.display()
                ));
                let content = std::fs::read_to_string(&req.path).unwrap_or_default();
                responder.respond(ReadTextFileResponse::new(content))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, |conn: ConnectionTo<Agent>| {
            let workspace = workspace.clone();
            let prompt = prompt.clone();
            async move {
                let caps = ClientCapabilities::new().fs(FileSystemCapabilities::new()
                    .read_text_file(true)
                    .write_text_file(true));
                let init = conn
                    .send_request(
                        InitializeRequest::new(ProtocolVersion::V1).client_capabilities(caps),
                    )
                    .block_task()
                    .await?;
                eprintln!("agent: {:?}", init.agent_info.map(|i| i.name));
                let session = conn
                    .send_request(NewSessionRequest::new(workspace))
                    .block_task()
                    .await?;
                let done = conn
                    .send_request(PromptRequest::new(
                        session.session_id,
                        vec![ContentBlock::Text(TextContent::new(prompt))],
                    ))
                    .block_task()
                    .await?;
                eprintln!("stop: {:?}", done.stop_reason);
                Ok(())
            }
        })
        .await?;

    for line in log.lock().unwrap().iter() {
        println!("{line}");
    }
    let after = std::fs::read_to_string(&reserved).unwrap_or_default();
    println!("RESERVED {} untouched: {}", reserved_rel, before == after);
    Ok(())
}
