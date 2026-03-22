use box0::{client, config, daemon, server};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "b0", about = "Box0 agent platform", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the Box0 server
    Server {
        #[arg(long)]
        config: Option<String>,
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        db: Option<String>,
    },
    /// Connect to a Box0 server
    Login {
        server_url: String,
        #[arg(long)]
        key: Option<String>,
    },
    /// Disconnect
    Logout,
    /// Manage workers
    Worker {
        #[command(subcommand)]
        command: WorkerCommand,
    },
    /// Manage nodes
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    /// Manage groups
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
    /// Manage agent skill integrations
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Delegate a task to a worker
    Delegate {
        /// Group name
        #[arg(long)]
        group: Option<String>,
        /// Continue an existing conversation
        #[arg(long)]
        thread: Option<String>,
        /// Worker name
        worker: String,
        /// Task (omit to read from stdin)
        task: Option<String>,
    },
    /// Wait for pending task results
    Wait,
    /// Reply to a worker's question
    Reply {
        /// Group name
        #[arg(long)]
        group: Option<String>,
        thread_id: String,
        message: String,
    },
    /// Reset everything
    Reset,
    /// Show connection status
    Status,
    /// Invite a user (admin only)
    Invite {
        name: String,
    },
}

#[derive(Subcommand)]
enum WorkerCommand {
    Add {
        #[arg(long)]
        group: Option<String>,
        name: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        instructions: String,
        #[arg(long, default_value = "local")]
        node: String,
        /// Runtime: auto (default), claude, or codex
        #[arg(long, default_value = "auto")]
        runtime: String,
    },
    Ls {
        #[arg(long)]
        group: Option<String>,
    },
    Info {
        #[arg(long)]
        group: Option<String>,
        name: String,
    },
    Update {
        #[arg(long)]
        group: Option<String>,
        name: String,
        #[arg(long)]
        instructions: String,
    },
    Remove {
        #[arg(long)]
        group: Option<String>,
        name: String,
    },
    Stop {
        #[arg(long)]
        group: Option<String>,
        name: String,
    },
    Start {
        #[arg(long)]
        group: Option<String>,
        name: String,
    },
    Logs {
        #[arg(long)]
        group: Option<String>,
        name: String,
    },
    Temp {
        #[arg(long)]
        group: Option<String>,
        task: String,
        #[arg(long, default_value = "You are a helpful assistant. Complete the task. Be concise.")]
        instructions: String,
    },
}

#[derive(Subcommand)]
enum NodeCommand {
    Join {
        server_url: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        key: Option<String>,
    },
    Ls,
}

#[derive(Subcommand)]
enum GroupCommand {
    Create { name: String },
    Ls,
    AddMember {
        group: Option<String>,
        user_id: String,
    },
}

#[derive(Subcommand)]
enum SkillCommand {
    Show,
    Install { agent: String },
    Uninstall { agent: String },
}

fn make_client(cfg: &config::CliConfig) -> client::BhClient {
    match &cfg.api_key {
        Some(key) => client::BhClient::with_api_key(&cfg.server_url(), key),
        None => client::BhClient::new(&cfg.server_url()),
    }
}

/// Resolve the group: use explicit --group, or fall back to default_group in config.
fn resolve_group(explicit: Option<String>) -> String {
    if let Some(g) = explicit {
        return g;
    }
    let cfg = config::CliConfig::load();
    if let Some(g) = cfg.default_group {
        return g;
    }
    eprintln!("Error: --group is required. Set a default with: b0 group switch <name>");
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Server { config: config_path, host, port, db } => {
            let mut cfg = config::ServerConfig::load(config_path.as_deref());
            if let Some(h) = host { cfg.host = h; }
            if let Some(p) = port { cfg.port = p; }
            if let Some(d) = db { cfg.db_path = d; }

            let default_level = if cfg.log_level == "info" { "warn" } else { &cfg.log_level };
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level)),
                )
                .init();

            server::run(cfg).await;
        }

        Command::Login { server_url, key } => cmd_login(&server_url, key.as_deref()).await,
        Command::Logout => cmd_logout(),
        Command::Reset => cmd_reset(),
        Command::Status => cmd_status().await,
        Command::Invite { name } => cmd_invite(&name).await,

        Command::Worker { command } => match command {
            WorkerCommand::Add { group, name, description, instructions, node, runtime } => { let group = resolve_group(group);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.register_worker(&group, &name, &description, &instructions, &node, &runtime).await {
                    Ok(w) => println!("Worker \"{}\" registered in group \"{}\" on node \"{}\" (runtime: {}).", w.name, group, w.node_id, w.runtime),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            WorkerCommand::Ls { group } => { let group = resolve_group(group);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.list_workers(&group).await {
                    Ok(workers) => {
                        if workers.is_empty() {
                            println!("No workers in group \"{}\".", group);
                        } else {
                            println!("{:<20} {:<30} {:<10} {:<10} {}", "NAME", "DESCRIPTION", "NODE", "STATUS", "CREATED");
                            for w in workers {
                                println!("{:<20} {:<30} {:<10} {:<10} {}", w.name, w.description, w.node_id, w.status, w.created_at.format("%Y-%m-%d %H:%M:%S"));
                            }
                        }
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            WorkerCommand::Info { group, name } => { let group = resolve_group(group);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.get_worker(&group, &name).await {
                    Ok(w) => {
                        println!("Name:          {}", w.name);
                        println!("Group:         {}", group);
                        println!("Node:          {}", w.node_id);
                        println!("Status:        {}", w.status);
                        println!("Registered by: {}", if w.registered_by.is_empty() { "(unknown)" } else { &w.registered_by });
                        println!("Created:       {}", w.created_at.format("%Y-%m-%d %H:%M:%S"));
                        println!("Instructions:  {}", w.instructions);
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            WorkerCommand::Update { group, name, instructions } => { let group = resolve_group(group);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.update_worker(&group, &name, &instructions).await {
                    Ok(()) => println!("Worker \"{}\" updated.", name),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            WorkerCommand::Remove { group, name } => { let group = resolve_group(group);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.remove_worker(&group, &name).await {
                    Ok(()) => println!("Worker \"{}\" removed.", name),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            WorkerCommand::Stop { group, name } => { let group = resolve_group(group);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.stop_worker(&group, &name).await {
                    Ok(()) => println!("Worker \"{}\" stopped.", name),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            WorkerCommand::Start { group, name } => { let group = resolve_group(group);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.start_worker(&group, &name).await {
                    Ok(()) => println!("Worker \"{}\" started.", name),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            WorkerCommand::Logs { group, name } => { let group = resolve_group(group);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.worker_logs(&group, &name).await {
                    Ok(messages) => {
                        if messages.is_empty() {
                            println!("No task history for \"{}\".", name);
                        } else {
                            for msg in messages {
                                let content = msg.content.as_ref().and_then(|v| v.as_str()).unwrap_or("").chars().take(80).collect::<String>();
                                println!("{} {} {:<8} {} -> {} {}", msg.created_at.format("%H:%M:%S"), &msg.thread_id, msg.msg_type, msg.from_agent, msg.to_agent, content);
                            }
                        }
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            WorkerCommand::Temp { group, task, instructions } => { let group = resolve_group(group);
                cmd_worker_temp(&group, &task, &instructions).await;
            }
        },

        Command::Node { command } => match command {
            NodeCommand::Join { server_url, name, key } => {
                cmd_node_join(&server_url, name.as_deref(), key.as_deref()).await;
            }
            NodeCommand::Ls => {
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.list_nodes().await {
                    Ok(nodes) => {
                        if nodes.is_empty() { println!("No nodes."); }
                        else {
                            println!("{:<20} {:<15} {:<10} {}", "NAME", "OWNER", "STATUS", "LAST HEARTBEAT");
                            for n in nodes {
                                let hb = n.last_heartbeat.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_else(|| "never".to_string());
                                println!("{:<20} {:<15} {:<10} {}", n.id, n.owner, n.status, hb);
                            }
                        }
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
        },

        Command::Group { command } => match command {
            GroupCommand::Create { name } => {
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.create_group(&name).await {
                    Ok(g) => println!("Group \"{}\" created.", g.name),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            GroupCommand::Ls => {
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.list_groups().await {
                    Ok(groups) => {
                        if groups.is_empty() { println!("No groups."); }
                        else {
                            println!("{:<20} {:<15} {}", "NAME", "CREATED BY", "CREATED");
                            for g in groups {
                                println!("{:<20} {:<15} {}", g.name, g.created_by, g.created_at.format("%Y-%m-%d %H:%M:%S"));
                            }
                        }
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            GroupCommand::AddMember { group, user_id } => { let group = resolve_group(group);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.add_group_member(&group, &user_id).await {
                    Ok(()) => println!("User {} added to group \"{}\".", user_id, group),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
        },

        Command::Skill { command } => match command {
            SkillCommand::Show => {
                let cfg = config::CliConfig::load();
                print!("{}", config::CliConfig::skill_content(&cfg.server_url()));
            }
            SkillCommand::Install { agent } => {
                let cfg = config::CliConfig::load();
                let url = cfg.server_url();
                match agent.as_str() {
                    "claude-code" => match config::CliConfig::install_skill_claude_code(&url) {
                        Ok(()) => println!("Skill installed for Claude Code."),
                        Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                    },
                    "codex" => match config::CliConfig::install_skill_codex(&url) {
                        Ok(()) => println!("Skill installed for Codex."),
                        Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                    },
                    _ => { eprintln!("Unknown agent: {}. Supported: claude-code, codex", agent); std::process::exit(1); }
                }
            }
            SkillCommand::Uninstall { agent } => {
                match agent.as_str() {
                    "claude-code" => { let _ = config::CliConfig::uninstall_skill_claude_code(); println!("Skill uninstalled for Claude Code."); }
                    "codex" => { let _ = config::CliConfig::uninstall_skill_codex(); println!("Skill uninstalled for Codex."); }
                    _ => { eprintln!("Unknown agent: {}. Supported: claude-code, codex", agent); std::process::exit(1); }
                }
            }
        },

        Command::Delegate { group, thread, worker, task } => { let group = resolve_group(group);
            let task_content = match task {
                Some(t) => t,
                None => {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf).expect("failed to read stdin");
                    buf
                }
            };
            cmd_delegate(&group, &worker, &task_content, thread.as_deref()).await;
        }

        Command::Wait => cmd_wait().await,

        Command::Reply { group, thread_id, message } => { let group = resolve_group(group);
            cmd_reply(&group, &thread_id, &message).await;
        }
    }
}

// --- Command implementations ---

async fn cmd_login(server_url: &str, api_key: Option<&str>) {
    let url = server_url.trim_end_matches('/');
    let client = match api_key {
        Some(key) => client::BhClient::with_api_key(url, key),
        None => client::BhClient::new(url),
    };

    match client.health().await {
        Ok(version) => println!("Connected to Box0 server v{}", version),
        Err(e) => { eprintln!("Error: could not connect to {}. {}", url, e); std::process::exit(1); }
    }

    let mut cfg = config::CliConfig::load();
    cfg.server_url = url.to_string();
    cfg.api_key = api_key.map(|s| s.to_string());
    let _ = cfg.lead_id();

    // Auto-set default_group from user's first group
    if cfg.default_group.is_none() {
        if let Ok(groups) = client.list_groups().await {
            if let Some(first) = groups.first() {
                cfg.default_group = Some(first.name.clone());
            }
        }
    }

    if let Err(e) = cfg.save() {
        eprintln!("Error saving config: {}", e);
        std::process::exit(1);
    }

    println!("Login complete. Server: {}", url);
    if let Some(ref g) = cfg.default_group {
        println!("Default group: {}", g);
    }
    println!("To install agent skill: b0 skill install claude-code  (or: codex)");
}

fn cmd_logout() {
    let _ = config::CliConfig::uninstall_skill_claude_code();
    let _ = config::CliConfig::uninstall_skill_codex();
    let cfg = config::CliConfig::load();
    let _ = cfg.clear();
    println!("Logged out.");
}

fn cmd_reset() {
    let b0_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".b0");
    for name in ["b0.db", "b0.db-wal", "b0.db-shm"] {
        let path = b0_dir.join(name);
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }
    let _ = config::CliConfig::uninstall_skill_claude_code();
    let _ = config::CliConfig::uninstall_skill_codex();
    let cfg = config::CliConfig::load();
    let _ = cfg.clear();
    println!("Reset complete.");
}

async fn cmd_status() {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    println!("Server: {}", cfg.server_url());
    match client.health().await {
        Ok(version) => println!("Status: connected (v{})", version),
        Err(_) => { println!("Status: disconnected"); return; }
    }

    if let Ok(groups) = client.list_groups().await {
        println!("Groups: {}", groups.len());
        for g in &groups { println!("  {}", g.name); }
    }

    let pending = config::CliConfig::load_pending();
    if pending.threads.is_empty() {
        println!("Pending tasks: none");
    } else {
        println!("Pending tasks: {}", pending.threads.len());
        for (tid, info) in &pending.threads {
            println!("  {} -> {} ({})", tid, info.worker, info.group);
        }
    }
}

async fn cmd_invite(name: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);
    match client.invite_user(name).await {
        Ok(resp) => {
            println!("User \"{}\" created (ID: {})", resp.name, resp.user_id);
            println!("  Key: {}", resp.key);
            println!("\nSave this key. It won't be shown again.");
        }
        Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
    }
}

async fn cmd_worker_temp(group: &str, task: &str, instructions: &str) {
    let mut cfg = config::CliConfig::load();
    let lead_id = cfg.lead_id();
    let client = make_client(&cfg);

    let temp_name = format!("temp-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    if let Err(e) = client.register_worker(group, &temp_name, "", instructions, "local", "auto").await {
        eprintln!("Error: {}", e); std::process::exit(1);
    }

    let _ = client.register_agent(group, &lead_id).await;

    let thread_id = format!("thread-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    match client.send_message(group, &temp_name, &thread_id, &lead_id, "request", Some(&serde_json::json!(task))).await {
        Ok(_) => {
            let mut pending = config::CliConfig::load_pending();
            pending.threads.insert(thread_id.clone(), config::PendingThread {
                worker: temp_name,
                group: group.to_string(),
                task: task.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                temp: true,
            });
            let _ = config::CliConfig::save_pending(&pending);
            println!("{}", thread_id);
        }
        Err(e) => {
            let _ = client.remove_worker(group, &temp_name).await;
            eprintln!("Error: {}", e); std::process::exit(1);
        }
    }
}

async fn cmd_delegate(group: &str, worker: &str, task: &str, continue_thread: Option<&str>) {
    let mut cfg = config::CliConfig::load();
    let lead_id = cfg.lead_id();
    let client = make_client(&cfg);

    if let Err(e) = client.get_worker(group, worker).await {
        eprintln!("Error: worker \"{}\" not found in group \"{}\". {}", worker, group, e);
        std::process::exit(1);
    }

    if let Err(e) = client.register_agent(group, &lead_id).await {
        eprintln!("Error registering lead agent: {}", e); std::process::exit(1);
    }

    // Reuse thread for multi-turn, or create new
    let thread_id = match continue_thread {
        Some(tid) => tid.to_string(),
        None => format!("thread-{}", &uuid::Uuid::new_v4().to_string()[..8]),
    };

    // For continuing a conversation, send as "answer" so daemon uses --resume
    let msg_type = if continue_thread.is_some() { "answer" } else { "request" };

    match client.send_message(group, worker, &thread_id, &lead_id, msg_type, Some(&serde_json::json!(task))).await {
        Ok(_) => {
            let mut pending = config::CliConfig::load_pending();
            pending.threads.insert(thread_id.clone(), config::PendingThread {
                worker: worker.to_string(),
                group: group.to_string(),
                task: task.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                temp: false,
            });
            let _ = config::CliConfig::save_pending(&pending);
            println!("{}", thread_id);
        }
        Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
    }
}

async fn cmd_wait() {
    let mut cfg = config::CliConfig::load();
    let lead_id = cfg.lead_id();
    let client = make_client(&cfg);

    let mut pending = config::CliConfig::load_pending();
    if pending.threads.is_empty() {
        println!("No pending tasks.");
        return;
    }

    println!("Waiting for {} task(s)...", pending.threads.len());

    loop {
        if pending.threads.is_empty() {
            println!("All done.");
            break;
        }

        // Poll each group's inbox
        let groups: Vec<String> = pending.threads.values().map(|t| t.group.clone()).collect::<std::collections::HashSet<_>>().into_iter().collect();

        for group in &groups {
            let messages = match client.get_inbox(group, &lead_id, Some("unread"), Some(5.0)).await {
                Ok(m) => m,
                Err(_) => continue,
            };

            for msg in messages {
                if let Some(thread_info) = pending.threads.get(&msg.thread_id) {
                    let elapsed = if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&thread_info.created_at) {
                        format!("{}s", (chrono::Utc::now() - created.with_timezone(&chrono::Utc)).num_seconds())
                    } else { "?s".to_string() };

                    let content = msg.content.as_ref().and_then(|v| v.as_str()).unwrap_or("(no content)");
                    let is_temp = thread_info.temp;
                    let worker_name = thread_info.worker.clone();
                    let thread_group = thread_info.group.clone();

                    match msg.msg_type.as_str() {
                        "done" => {
                            println!("{} done ({}): {}", worker_name, elapsed, content);
                            pending.threads.remove(&msg.thread_id);
                            if is_temp { let _ = client.remove_worker(&thread_group, &worker_name).await; }
                        }
                        "failed" => {
                            eprintln!("{} failed ({}): {}", worker_name, elapsed, content);
                            pending.threads.remove(&msg.thread_id);
                            if is_temp { let _ = client.remove_worker(&thread_group, &worker_name).await; }
                        }
                        "question" => {
                            println!("\n{} asks (thread {}): {}\n  -> Use: b0 reply --group {} {} \"<your answer>\"",
                                worker_name, msg.thread_id, content, thread_group, msg.thread_id);
                        }
                        _ => {}
                    }
                }
                let _ = client.ack_message(group, &msg.id).await;
            }
        }

        let _ = config::CliConfig::save_pending(&pending);
    }
}

async fn cmd_reply(group: &str, thread_id: &str, message: &str) {
    let mut cfg = config::CliConfig::load();
    let lead_id = cfg.lead_id();
    let client = make_client(&cfg);

    // Try pending first, then fall back to requiring --worker
    let pending = config::CliConfig::load_pending();
    let worker = match pending.threads.get(thread_id) {
        Some(t) => t.worker.clone(),
        None => {
            // Look up worker from thread history
            eprintln!("Error: thread \"{}\" not found. Use: b0 delegate --thread {} <worker> \"<message>\"", thread_id, thread_id);
            std::process::exit(1);
        }
    };

    // Send as "answer" and re-add to pending for bh wait
    match client.send_message(group, &worker, thread_id, &lead_id, "answer", Some(&serde_json::json!(message))).await {
        Ok(_) => {
            // Re-add to pending so b0 wait can collect the response
            let mut pending = config::CliConfig::load_pending();
            pending.threads.insert(thread_id.to_string(), config::PendingThread {
                worker: worker.clone(),
                group: group.to_string(),
                task: message.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                temp: false,
            });
            let _ = config::CliConfig::save_pending(&pending);
            println!("Reply sent to {} (thread {}). Run b0 wait to collect response.", worker, thread_id);
        }
        Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
    }
}

async fn cmd_node_join(server_url: &str, name: Option<&str>, api_key: Option<&str>) {
    let node_id = name.map(|s| s.to_string()).unwrap_or_else(|| {
        format!("node-{}", &uuid::Uuid::new_v4().to_string()[..8])
    });

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    println!("Joining as node \"{}\" -> {}", node_id, server_url);
    daemon::run_remote(server_url, &node_id, api_key).await;
}
