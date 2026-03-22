use box0::{client, config, daemon, server};
use clap::{Parser, Subcommand};
use std::io::IsTerminal;

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
    /// Manage agents
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Manage machines
    Machine {
        #[command(subcommand)]
        command: MachineCommand,
    },
    /// Manage workspaces
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Manage agent skill integrations
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Schedule recurring tasks
    Cron {
        #[command(subcommand)]
        command: CronCommand,
    },
    /// Delegate a task to an agent
    Delegate {
        /// Workspace name
        #[arg(long)]
        workspace: Option<String>,
        /// Continue an existing conversation
        #[arg(long)]
        thread: Option<String>,
        /// Agent name
        agent: String,
        /// Task (omit to read from stdin)
        task: Option<String>,
    },
    /// Wait for pending task results
    Wait,
    /// Reply to an agent's question
    Reply {
        /// Workspace name
        #[arg(long)]
        workspace: Option<String>,
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
enum AgentCommand {
    Add {
        #[arg(long)]
        workspace: Option<String>,
        name: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        instructions: String,
        #[arg(long, default_value = "local")]
        machine: String,
        /// Runtime: auto (default), claude, or codex
        #[arg(long, default_value = "auto")]
        runtime: String,
    },
    Ls {
        #[arg(long)]
        workspace: Option<String>,
    },
    Info {
        #[arg(long)]
        workspace: Option<String>,
        name: String,
    },
    Update {
        #[arg(long)]
        workspace: Option<String>,
        name: String,
        #[arg(long)]
        instructions: String,
    },
    Remove {
        #[arg(long)]
        workspace: Option<String>,
        name: String,
    },
    Stop {
        #[arg(long)]
        workspace: Option<String>,
        name: String,
    },
    Start {
        #[arg(long)]
        workspace: Option<String>,
        name: String,
    },
    Logs {
        #[arg(long)]
        workspace: Option<String>,
        name: String,
    },
    Temp {
        #[arg(long)]
        workspace: Option<String>,
        task: String,
        #[arg(long, default_value = "You are a helpful assistant. Complete the task. Be concise.")]
        instructions: String,
    },
}

#[derive(Subcommand)]
enum MachineCommand {
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
enum WorkspaceCommand {
    Create { name: String },
    Ls,
    AddMember {
        workspace: Option<String>,
        user_id: String,
    },
}

#[derive(Subcommand)]
enum SkillCommand {
    Show,
    Install { agent: String },
    Uninstall { agent: String },
}

#[derive(Subcommand)]
enum CronCommand {
    /// Schedule a recurring task
    Add {
        #[arg(long)]
        workspace: Option<String>,
        /// Agent name
        agent: String,
        /// Schedule: 30s, 5m, 1h, 6h, 1d
        #[arg(long)]
        every: String,
        /// Task to run
        task: String,
    },
    /// List scheduled tasks
    Ls {
        #[arg(long)]
        workspace: Option<String>,
    },
    /// Remove a scheduled task
    Remove {
        #[arg(long)]
        workspace: Option<String>,
        /// Cron job ID
        id: String,
    },
    /// Enable a scheduled task
    Enable {
        #[arg(long)]
        workspace: Option<String>,
        id: String,
    },
    /// Disable a scheduled task
    Disable {
        #[arg(long)]
        workspace: Option<String>,
        id: String,
    },
}

fn make_client(cfg: &config::CliConfig) -> client::BhClient {
    match &cfg.api_key {
        Some(key) => client::BhClient::with_api_key(&cfg.server_url(), key),
        None => client::BhClient::new(&cfg.server_url()),
    }
}

/// Expand @/path/to/file references in a task string.
/// Replaces each @<path> with the file contents appended at the end.
fn expand_file_refs(task: &str) -> String {
    let re = regex::Regex::new(r"@(/[^\s]+)").unwrap();
    let mut files: Vec<(String, String)> = Vec::new();
    let cleaned = re.replace_all(task, |caps: &regex::Captures| {
        let path = &caps[1];
        let p = std::path::Path::new(path);
        if p.is_file() {
            match std::fs::read_to_string(p) {
                Ok(content) => {
                    files.push((path.to_string(), content));
                    path.to_string()
                }
                Err(e) => {
                    eprintln!("Warning: could not read {}: {}", path, e);
                    format!("@{}", path)
                }
            }
        } else if p.is_dir() {
            // List directory contents
            let mut listing = Vec::new();
            if let Ok(entries) = std::fs::read_dir(p) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let kind = if entry.path().is_dir() { "dir" } else { "file" };
                    listing.push(format!("  {} ({})", name, kind));
                }
            }
            if !listing.is_empty() {
                files.push((path.to_string(), listing.join("\n")));
            }
            path.to_string()
        } else {
            // Not a valid path, leave as-is
            format!("@{}", path)
        }
    });

    if files.is_empty() {
        return cleaned.to_string();
    }

    let mut result = cleaned.to_string();
    for (path, content) in &files {
        result.push_str(&format!("\n\n--- {} ---\n{}", path, content));
    }
    result
}

/// Resolve the workspace: use explicit --workspace, or fall back to default_workspace in config.
fn resolve_workspace(explicit: Option<String>) -> String {
    if let Some(w) = explicit {
        return w;
    }
    let cfg = config::CliConfig::load();
    if let Some(w) = cfg.default_workspace {
        return w;
    }
    eprintln!("Error: --workspace is required. Set a default with: b0 workspace switch <name>");
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

        Command::Agent { command } => match command {
            AgentCommand::Add { workspace, name, description, instructions, machine, runtime } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.register_agent(&workspace, &name, &description, &instructions, &machine, &runtime).await {
                    Ok(a) => println!("Agent \"{}\" registered in workspace \"{}\" on machine \"{}\" (runtime: {}).", a.name, workspace, a.machine_id, a.runtime),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            AgentCommand::Ls { workspace } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.list_agents(&workspace).await {
                    Ok(agents) => {
                        if agents.is_empty() {
                            println!("No agents in workspace \"{}\".", workspace);
                        } else {
                            println!("{:<20} {:<30} {:<10} {:<10} {}", "NAME", "DESCRIPTION", "MACHINE", "STATUS", "CREATED");
                            for a in agents {
                                println!("{:<20} {:<30} {:<10} {:<10} {}", a.name, a.description, a.machine_id, a.status, a.created_at.format("%Y-%m-%d %H:%M:%S"));
                            }
                        }
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            AgentCommand::Info { workspace, name } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.get_agent(&workspace, &name).await {
                    Ok(a) => {
                        println!("Name:          {}", a.name);
                        println!("Workspace:     {}", workspace);
                        println!("Machine:       {}", a.machine_id);
                        println!("Status:        {}", a.status);
                        println!("Registered by: {}", if a.registered_by.is_empty() { "(unknown)" } else { &a.registered_by });
                        println!("Created:       {}", a.created_at.format("%Y-%m-%d %H:%M:%S"));
                        println!("Instructions:  {}", a.instructions);
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            AgentCommand::Update { workspace, name, instructions } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.update_agent(&workspace, &name, &instructions).await {
                    Ok(()) => println!("Agent \"{}\" updated.", name),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            AgentCommand::Remove { workspace, name } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.remove_agent(&workspace, &name).await {
                    Ok(()) => println!("Agent \"{}\" removed.", name),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            AgentCommand::Stop { workspace, name } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.stop_agent(&workspace, &name).await {
                    Ok(()) => println!("Agent \"{}\" stopped.", name),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            AgentCommand::Start { workspace, name } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.start_agent(&workspace, &name).await {
                    Ok(()) => println!("Agent \"{}\" started.", name),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            AgentCommand::Logs { workspace, name } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.agent_logs(&workspace, &name).await {
                    Ok(messages) => {
                        if messages.is_empty() {
                            println!("No task history for \"{}\".", name);
                        } else {
                            for msg in messages {
                                let content = msg.content.as_ref().and_then(|v| v.as_str()).unwrap_or("").chars().take(80).collect::<String>();
                                println!("{} {} {:<8} {} -> {} {}", msg.created_at.format("%H:%M:%S"), &msg.thread_id, msg.msg_type, msg.from_id, msg.to_id, content);
                            }
                        }
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            AgentCommand::Temp { workspace, task, instructions } => { let workspace = resolve_workspace(workspace);
                let task_content = if !std::io::stdin().is_terminal() {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf).expect("failed to read stdin");
                    if !buf.trim().is_empty() {
                        format!("{}\n\n{}", task, buf)
                    } else {
                        task
                    }
                } else {
                    task
                };
                let task_content = expand_file_refs(&task_content);
                cmd_agent_temp(&workspace, &task_content, &instructions).await;
            }
        },

        Command::Machine { command } => match command {
            MachineCommand::Join { server_url, name, key } => {
                cmd_machine_join(&server_url, name.as_deref(), key.as_deref()).await;
            }
            MachineCommand::Ls => {
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.list_machines().await {
                    Ok(machines) => {
                        if machines.is_empty() { println!("No machines."); }
                        else {
                            println!("{:<20} {:<15} {:<10} {}", "NAME", "OWNER", "STATUS", "LAST HEARTBEAT");
                            for m in machines {
                                let hb = m.last_heartbeat.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_else(|| "never".to_string());
                                println!("{:<20} {:<15} {:<10} {}", m.id, m.owner, m.status, hb);
                            }
                        }
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
        },

        Command::Workspace { command } => match command {
            WorkspaceCommand::Create { name } => {
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.create_workspace(&name).await {
                    Ok(w) => println!("Workspace \"{}\" created.", w.name),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            WorkspaceCommand::Ls => {
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.list_workspaces().await {
                    Ok(workspaces) => {
                        if workspaces.is_empty() { println!("No workspaces."); }
                        else {
                            println!("{:<20} {:<15} {}", "NAME", "CREATED BY", "CREATED");
                            for w in workspaces {
                                println!("{:<20} {:<15} {}", w.name, w.created_by, w.created_at.format("%Y-%m-%d %H:%M:%S"));
                            }
                        }
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            WorkspaceCommand::AddMember { workspace, user_id } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.add_workspace_member(&workspace, &user_id).await {
                    Ok(()) => println!("User {} added to workspace \"{}\".", user_id, workspace),
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

        Command::Cron { command } => match command {
            CronCommand::Add { workspace, agent, every, task } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.create_cron_job(&workspace, &agent, &every, &task).await {
                    Ok(job) => println!("Cron job \"{}\" created. Agent \"{}\" will run every {}.", job.id, agent, every),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            CronCommand::Ls { workspace } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.list_cron_jobs(&workspace).await {
                    Ok(jobs) => {
                        if jobs.is_empty() {
                            println!("No scheduled tasks in workspace \"{}\".", workspace);
                        } else {
                            println!("{:<16} {:<16} {:<10} {:<8} {:<20} {}", "ID", "AGENT", "SCHEDULE", "ENABLED", "LAST RUN", "TASK");
                            for j in jobs {
                                let last = j.last_run.map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "never".to_string());
                                let task_preview: String = j.task.chars().take(40).collect();
                                println!("{:<16} {:<16} {:<10} {:<8} {:<20} {}", j.id, j.agent, j.schedule, j.enabled, last, task_preview);
                            }
                        }
                    }
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            CronCommand::Remove { workspace, id } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.remove_cron_job(&workspace, &id).await {
                    Ok(()) => println!("Cron job \"{}\" removed.", id),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            CronCommand::Enable { workspace, id } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.set_cron_enabled(&workspace, &id, true).await {
                    Ok(()) => println!("Cron job \"{}\" enabled.", id),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
            CronCommand::Disable { workspace, id } => { let workspace = resolve_workspace(workspace);
                let cfg = config::CliConfig::load();
                let client = make_client(&cfg);
                match client.set_cron_enabled(&workspace, &id, false).await {
                    Ok(()) => println!("Cron job \"{}\" disabled.", id),
                    Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
                }
            }
        },

        Command::Delegate { workspace, thread, agent, task } => { let workspace = resolve_workspace(workspace);
            let task_content = match task {
                Some(t) => {
                    if !std::io::stdin().is_terminal() {
                        use std::io::Read;
                        let mut buf = String::new();
                        std::io::stdin().read_to_string(&mut buf).expect("failed to read stdin");
                        if !buf.trim().is_empty() {
                            format!("{}\n\n{}", t, buf)
                        } else {
                            t
                        }
                    } else {
                        t
                    }
                }
                None => {
                    if !std::io::stdin().is_terminal() {
                        use std::io::Read;
                        let mut buf = String::new();
                        std::io::stdin().read_to_string(&mut buf).expect("failed to read stdin");
                        buf
                    } else {
                        eprintln!("Error: no task provided. Pass a task argument or pipe content via stdin.");
                        std::process::exit(1);
                    }
                }
            };
            let task_content = expand_file_refs(&task_content);
            cmd_delegate(&workspace, &agent, &task_content, thread.as_deref()).await;
        }

        Command::Wait => cmd_wait().await,

        Command::Reply { workspace, thread_id, message } => { let workspace = resolve_workspace(workspace);
            cmd_reply(&workspace, &thread_id, &message).await;
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

    // Auto-set default_workspace from user's first workspace
    if cfg.default_workspace.is_none() {
        if let Ok(workspaces) = client.list_workspaces().await {
            if let Some(first) = workspaces.first() {
                cfg.default_workspace = Some(first.name.clone());
            }
        }
    }

    if let Err(e) = cfg.save() {
        eprintln!("Error saving config: {}", e);
        std::process::exit(1);
    }

    println!("Login complete. Server: {}", url);
    if let Some(ref w) = cfg.default_workspace {
        println!("Default workspace: {}", w);
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

    if let Ok(workspaces) = client.list_workspaces().await {
        println!("Workspaces: {}", workspaces.len());
        for w in &workspaces { println!("  {}", w.name); }
    }

    let pending = config::CliConfig::load_pending();
    if pending.threads.is_empty() {
        println!("Pending tasks: none");
    } else {
        println!("Pending tasks: {}", pending.threads.len());
        for (tid, info) in &pending.threads {
            println!("  {} -> {} ({})", tid, info.agent, info.workspace);
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

async fn cmd_agent_temp(workspace: &str, task: &str, instructions: &str) {
    let mut cfg = config::CliConfig::load();
    let lead_id = cfg.lead_id();
    let client = make_client(&cfg);

    let temp_name = format!("temp-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    if let Err(e) = client.register_agent(workspace, &temp_name, "", instructions, "local", "auto").await {
        eprintln!("Error: {}", e); std::process::exit(1);
    }

    let thread_id = format!("thread-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    match client.send_message(workspace, &temp_name, &thread_id, &lead_id, "request", Some(&serde_json::json!(task))).await {
        Ok(_) => {
            let mut pending = config::CliConfig::load_pending();
            pending.threads.insert(thread_id.clone(), config::PendingThread {
                agent: temp_name,
                workspace: workspace.to_string(),
                task: task.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                temp: true,
            });
            let _ = config::CliConfig::save_pending(&pending);
            println!("{}", thread_id);
        }
        Err(e) => {
            let _ = client.remove_agent(workspace, &temp_name).await;
            eprintln!("Error: {}", e); std::process::exit(1);
        }
    }
}

async fn cmd_delegate(workspace: &str, agent: &str, task: &str, continue_thread: Option<&str>) {
    let mut cfg = config::CliConfig::load();
    let lead_id = cfg.lead_id();
    let client = make_client(&cfg);

    if let Err(e) = client.get_agent(workspace, agent).await {
        eprintln!("Error: agent \"{}\" not found in workspace \"{}\". {}", agent, workspace, e);
        std::process::exit(1);
    }

    // Reuse thread for multi-turn, or create new
    let thread_id = match continue_thread {
        Some(tid) => tid.to_string(),
        None => format!("thread-{}", &uuid::Uuid::new_v4().to_string()[..8]),
    };

    // For continuing a conversation, send as "answer" so daemon uses --resume
    let msg_type = if continue_thread.is_some() { "answer" } else { "request" };

    match client.send_message(workspace, agent, &thread_id, &lead_id, msg_type, Some(&serde_json::json!(task))).await {
        Ok(_) => {
            let mut pending = config::CliConfig::load_pending();
            pending.threads.insert(thread_id.clone(), config::PendingThread {
                agent: agent.to_string(),
                workspace: workspace.to_string(),
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

    let total = pending.threads.len();
    println!("Waiting for {} task(s)...\n", total);

    // Track per-agent status: "queued" or "running"
    let mut status: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for info in pending.threads.values() {
        status.insert(info.agent.clone(), "queued");
    }

    let is_tty = std::io::IsTerminal::is_terminal(&std::io::stderr());
    let mut status_lines_printed: usize = 0;

    // Print initial status
    print_status(&status, &pending, is_tty, &mut status_lines_printed);

    loop {
        if pending.threads.is_empty() {
            println!("\nAll {} task(s) done.", total);
            break;
        }

        let workspaces: Vec<String> = pending.threads.values().map(|t| t.workspace.clone()).collect::<std::collections::HashSet<_>>().into_iter().collect();

        for workspace in &workspaces {
            let messages = match client.get_inbox(workspace, &lead_id, Some("unread"), Some(2.0)).await {
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
                    let agent_name = thread_info.agent.clone();
                    let thread_workspace = thread_info.workspace.clone();

                    match msg.msg_type.as_str() {
                        "started" => {
                            status.insert(agent_name.clone(), "running");
                            // Clear and reprint status
                            clear_status(is_tty, status_lines_printed);
                            status_lines_printed = 0;
                            print_status(&status, &pending, is_tty, &mut status_lines_printed);
                        }
                        "done" => {
                            status.remove(&agent_name);
                            clear_status(is_tty, status_lines_printed);
                            status_lines_printed = 0;
                            println!("{} done ({}): {}", agent_name, elapsed, content);
                            pending.threads.remove(&msg.thread_id);
                            if is_temp { let _ = client.remove_agent(&thread_workspace, &agent_name).await; }
                            if !pending.threads.is_empty() {
                                println!();
                                print_status(&status, &pending, is_tty, &mut status_lines_printed);
                            }
                        }
                        "failed" => {
                            status.remove(&agent_name);
                            clear_status(is_tty, status_lines_printed);
                            status_lines_printed = 0;
                            eprintln!("{} failed ({}): {}", agent_name, elapsed, content);
                            pending.threads.remove(&msg.thread_id);
                            if is_temp { let _ = client.remove_agent(&thread_workspace, &agent_name).await; }
                            if !pending.threads.is_empty() {
                                println!();
                                print_status(&status, &pending, is_tty, &mut status_lines_printed);
                            }
                        }
                        "question" => {
                            clear_status(is_tty, status_lines_printed);
                            status_lines_printed = 0;
                            println!("\n{} asks (thread {}): {}\n  -> Use: b0 reply --workspace {} {} \"<your answer>\"",
                                agent_name, msg.thread_id, content, thread_workspace, msg.thread_id);
                            print_status(&status, &pending, is_tty, &mut status_lines_printed);
                        }
                        _ => {}
                    }
                }
                let _ = client.ack_message(workspace, &msg.id).await;
            }
        }

        // Refresh elapsed times in status display
        if !pending.threads.is_empty() && is_tty {
            clear_status(is_tty, status_lines_printed);
            status_lines_printed = 0;
            print_status(&status, &pending, is_tty, &mut status_lines_printed);
        }

        let _ = config::CliConfig::save_pending(&pending);
    }
}

fn print_status(
    status: &std::collections::HashMap<String, &str>,
    pending: &config::PendingState,
    _is_tty: bool,
    lines_printed: &mut usize,
) {
    for info in pending.threads.values() {
        let state = status.get(info.agent.as_str()).copied().unwrap_or("queued");
        let elapsed = if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&info.created_at) {
            let secs = (chrono::Utc::now() - created.with_timezone(&chrono::Utc)).num_seconds();
            format!("{}s", secs)
        } else {
            "?s".to_string()
        };
        let indicator = match state {
            "running" => "...",
            _ => "   ",
        };
        eprintln!("  {:<16} {} ({}){}", info.agent, state, elapsed, indicator);
        *lines_printed += 1;
    }
}

fn clear_status(is_tty: bool, lines: usize) {
    if !is_tty || lines == 0 {
        return;
    }
    // Move cursor up and clear each line
    for _ in 0..lines {
        eprint!("\x1b[1A\x1b[2K");
    }
}

async fn cmd_reply(workspace: &str, thread_id: &str, message: &str) {
    let mut cfg = config::CliConfig::load();
    let lead_id = cfg.lead_id();
    let client = make_client(&cfg);

    // Try pending first, then fall back to requiring --agent
    let pending = config::CliConfig::load_pending();
    let agent = match pending.threads.get(thread_id) {
        Some(t) => t.agent.clone(),
        None => {
            // Look up agent from thread history
            eprintln!("Error: thread \"{}\" not found. Use: b0 delegate --thread {} <agent> \"<message>\"", thread_id, thread_id);
            std::process::exit(1);
        }
    };

    // Send as "answer" and re-add to pending for b0 wait
    match client.send_message(workspace, &agent, thread_id, &lead_id, "answer", Some(&serde_json::json!(message))).await {
        Ok(_) => {
            // Re-add to pending so b0 wait can collect the response
            let mut pending = config::CliConfig::load_pending();
            pending.threads.insert(thread_id.to_string(), config::PendingThread {
                agent: agent.clone(),
                workspace: workspace.to_string(),
                task: message.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                temp: false,
            });
            let _ = config::CliConfig::save_pending(&pending);
            println!("Reply sent to {} (thread {}). Run b0 wait to collect response.", agent, thread_id);
        }
        Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
    }
}

async fn cmd_machine_join(server_url: &str, name: Option<&str>, api_key: Option<&str>) {
    let machine_id = name.map(|s| s.to_string()).unwrap_or_else(|| {
        format!("machine-{}", &uuid::Uuid::new_v4().to_string()[..8])
    });

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    println!("Joining as machine \"{}\" -> {}", machine_id, server_url);
    daemon::run_remote(server_url, &machine_id, api_key).await;
}
