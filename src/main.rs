mod client;
mod config;
mod daemon;
mod db;
mod server;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bh", about = "Boxhouse — agent platform", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the Boxhouse server
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
    /// Connect to a Boxhouse server
    Login {
        /// Server URL (e.g., http://localhost:8080)
        server_url: String,
        /// API key (if auth is enabled)
        #[arg(long)]
        key: Option<String>,
    },
    /// Disconnect from Boxhouse server
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
    /// Manage groups and API keys
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
    /// Delegate a task to a worker
    Delegate {
        worker: String,
        /// Task description (omit to read from stdin)
        task: Option<String>,
    },
    /// Wait for pending task results
    Wait,
    /// Reply to a worker's question
    Reply {
        thread_id: String,
        message: String,
    },
    /// Show connection status and pending tasks
    Status,
}

#[derive(Subcommand)]
enum WorkerCommand {
    /// Register a new worker
    Add {
        name: String,
        #[arg(long)]
        instructions: String,
        /// Node to run on (default: local)
        #[arg(long, default_value = "local")]
        node: String,
    },
    /// List all workers
    Ls,
    /// Show worker details
    Info { name: String },
    /// Update worker instructions
    Update {
        name: String,
        #[arg(long)]
        instructions: String,
    },
    /// Remove a worker
    Remove { name: String },
    /// Stop a worker (pause task processing)
    Stop { name: String },
    /// Start a stopped worker
    Start { name: String },
    /// Show recent task history for a worker
    Logs { name: String },
    /// Run a one-off task
    Temp {
        task: String,
        #[arg(long, default_value = "You are a helpful assistant. Complete the task. Be concise.")]
        instructions: String,
    },
}

#[derive(Subcommand)]
enum NodeCommand {
    /// Join as a worker node
    Join {
        /// Server URL
        server_url: String,
        /// Node name (default: hostname)
        #[arg(long)]
        name: Option<String>,
        /// API key
        #[arg(long)]
        key: Option<String>,
    },
    /// List all nodes
    Ls,
}

#[derive(Subcommand)]
enum GroupCommand {
    /// Create a new group (admin only)
    Create { name: String },
    /// List groups (admin only)
    Ls,
    /// Generate an API key for a group (admin only)
    Invite {
        /// Group name
        group: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    /// List API keys
    Keys,
    /// Revoke an API key (admin only)
    Revoke { key_prefix: String },
}

fn make_client(cfg: &config::CliConfig) -> client::BhClient {
    match &cfg.api_key {
        Some(key) => client::BhClient::with_api_key(&cfg.server_url(), key),
        None => client::BhClient::new(&cfg.server_url()),
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Server {
            config: config_path,
            host,
            port,
            db,
        } => {
            let mut cfg = config::ServerConfig::load(config_path.as_deref());
            if let Some(h) = host {
                cfg.host = h;
            }
            if let Some(p) = port {
                cfg.port = p;
            }
            if let Some(d) = db {
                cfg.db_path = d;
            }

            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cfg.log_level)),
                )
                .init();

            server::run(cfg).await;
        }

        Command::Login { server_url, key } => cmd_login(&server_url, key.as_deref()).await,
        Command::Logout => cmd_logout(),

        Command::Worker { command } => match command {
            WorkerCommand::Add {
                name,
                instructions,
                node,
            } => cmd_worker_add(&name, &instructions, &node).await,
            WorkerCommand::Ls => cmd_worker_ls().await,
            WorkerCommand::Info { name } => cmd_worker_info(&name).await,
            WorkerCommand::Update { name, instructions } => {
                cmd_worker_update(&name, &instructions).await
            }
            WorkerCommand::Remove { name } => cmd_worker_remove(&name).await,
            WorkerCommand::Stop { name } => cmd_worker_stop(&name).await,
            WorkerCommand::Start { name } => cmd_worker_start(&name).await,
            WorkerCommand::Logs { name } => cmd_worker_logs(&name).await,
            WorkerCommand::Temp { task, instructions } => {
                cmd_worker_temp(&task, &instructions).await
            }
        },

        Command::Node { command } => match command {
            NodeCommand::Join {
                server_url,
                name,
                key,
            } => cmd_node_join(&server_url, name.as_deref(), key.as_deref()).await,
            NodeCommand::Ls => cmd_node_ls().await,
        },

        Command::Group { command } => match command {
            GroupCommand::Create { name } => cmd_group_create(&name).await,
            GroupCommand::Ls => cmd_group_ls().await,
            GroupCommand::Invite { group, description } => {
                cmd_group_invite(&group, &description).await
            }
            GroupCommand::Keys => cmd_group_keys().await,
            GroupCommand::Revoke { key_prefix } => cmd_group_revoke(&key_prefix).await,
        },

        Command::Delegate { worker, task } => {
            let task_content = match task {
                Some(t) => t,
                None => {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin()
                        .read_to_string(&mut buf)
                        .expect("failed to read from stdin");
                    buf
                }
            };
            cmd_delegate(&worker, &task_content).await;
        }
        Command::Wait => cmd_wait().await,
        Command::Reply { thread_id, message } => cmd_reply(&thread_id, &message).await,
        Command::Status => cmd_status().await,
    }
}

// --- Login / Logout ---

async fn cmd_login(server_url: &str, api_key: Option<&str>) {
    let url = server_url.trim_end_matches('/');

    let client = match api_key {
        Some(key) => client::BhClient::with_api_key(url, key),
        None => client::BhClient::new(url),
    };

    match client.health().await {
        Ok(version) => println!("Connected to Boxhouse server v{}", version),
        Err(e) => {
            eprintln!("Error: could not connect to {}. {}", url, e);
            std::process::exit(1);
        }
    }

    let mut cfg = config::CliConfig::load();
    cfg.server_url = url.to_string();
    cfg.api_key = api_key.map(|s| s.to_string());
    let _ = cfg.lead_id();
    if let Err(e) = cfg.save() {
        eprintln!("Error saving config: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = config::CliConfig::install_skill(url) {
        eprintln!("Warning: failed to install skill: {}", e);
    } else {
        println!("Claude Code skill installed.");
    }

    println!("Login complete. Server: {}", url);
}

fn cmd_logout() {
    let cfg = config::CliConfig::load();
    let _ = config::CliConfig::uninstall_skill();
    let _ = cfg.clear();
    println!("Logged out.");
}

// --- Worker commands ---

async fn cmd_worker_add(name: &str, instructions: &str, node: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.register_worker(name, instructions, node).await {
        Ok(worker) => println!(
            "Worker \"{}\" registered on node \"{}\".",
            worker.name, worker.node_id
        ),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_worker_ls() {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.list_workers().await {
        Ok(workers) => {
            if workers.is_empty() {
                println!("No workers registered.");
            } else {
                println!("{:<20} {:<10} {:<10} {}", "NAME", "NODE", "STATUS", "CREATED");
                for w in workers {
                    println!(
                        "{:<20} {:<10} {:<10} {}",
                        w.name,
                        w.node_id,
                        w.status,
                        w.created_at.format("%Y-%m-%d %H:%M:%S")
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_worker_info(name: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.get_worker(name).await {
        Ok(w) => {
            println!("Name:          {}", w.name);
            println!("Node:          {}", w.node_id);
            println!("Status:        {}", w.status);
            println!("Registered by: {}", if w.registered_by.is_empty() { "(no auth)" } else { &w.registered_by });
            println!("Created:       {}", w.created_at.format("%Y-%m-%d %H:%M:%S"));
            println!("Instructions:  {}", w.instructions);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_worker_update(name: &str, instructions: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.update_worker(name, instructions).await {
        Ok(()) => println!("Worker \"{}\" updated.", name),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_worker_remove(name: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.remove_worker(name).await {
        Ok(()) => println!("Worker \"{}\" removed.", name),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_worker_stop(name: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.stop_worker(name).await {
        Ok(()) => println!("Worker \"{}\" stopped.", name),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_worker_start(name: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.start_worker(name).await {
        Ok(()) => println!("Worker \"{}\" started.", name),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_worker_logs(name: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.worker_logs(name).await {
        Ok(messages) => {
            if messages.is_empty() {
                println!("No task history for \"{}\".", name);
            } else {
                for msg in messages {
                    let content = msg
                        .content
                        .as_ref()
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .chars()
                        .take(80)
                        .collect::<String>();
                    println!(
                        "{} {} {:<8} {} → {} {}",
                        msg.created_at.format("%H:%M:%S"),
                        &msg.thread_id,
                        msg.msg_type,
                        msg.from_agent,
                        msg.to_agent,
                        content,
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_worker_temp(task: &str, instructions: &str) {
    let mut cfg = config::CliConfig::load();
    let lead_id = cfg.lead_id();
    let client = make_client(&cfg);

    // Create temp worker
    let temp_name = format!("temp-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    if let Err(e) = client
        .register_worker(&temp_name, instructions, "local")
        .await
    {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    // Ensure lead agent exists
    let _ = client.register_agent(&lead_id).await;

    // Delegate (non-blocking, same as cmd_delegate)
    let thread_id = format!("thread-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    match client
        .send_message(
            &temp_name,
            &thread_id,
            &lead_id,
            "request",
            Some(&serde_json::json!(task)),
        )
        .await
    {
        Ok(_) => {
            // Store in pending with temp flag
            let mut pending = config::CliConfig::load_pending();
            pending.threads.insert(
                thread_id.clone(),
                config::PendingThread {
                    worker: temp_name,
                    task: task.to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    temp: true,
                },
            );
            let _ = config::CliConfig::save_pending(&pending);
            println!("{}", thread_id);
        }
        Err(e) => {
            let _ = client.remove_worker(&temp_name).await;
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

// --- Node commands ---

async fn cmd_node_join(server_url: &str, name: Option<&str>, api_key: Option<&str>) {
    let node_id = name.map(|s| s.to_string()).unwrap_or_else(|| {
        format!("node-{}", &uuid::Uuid::new_v4().to_string()[..8])
    });

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    println!("Joining as node \"{}\" → {}", node_id, server_url);
    daemon::run_remote(server_url, &node_id, api_key).await;
}

async fn cmd_node_ls() {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.list_nodes().await {
        Ok(nodes) => {
            if nodes.is_empty() {
                println!("No nodes registered.");
            } else {
                println!("{:<20} {:<10} {}", "NAME", "STATUS", "LAST HEARTBEAT");
                for n in nodes {
                    let hb = n
                        .last_heartbeat
                        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| "never".to_string());
                    println!("{:<20} {:<10} {}", n.id, n.status, hb);
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

// --- Team commands ---

async fn cmd_group_create(name: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.create_group(name).await {
        Ok(group) => println!("Group \"{}\" created.", group.name),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_group_ls() {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.list_groups().await {
        Ok(groups) => {
            if groups.is_empty() {
                println!("No groups.");
            } else {
                println!("{:<20} {}", "NAME", "CREATED");
                for g in groups {
                    println!(
                        "{:<20} {}",
                        g.name,
                        g.created_at.format("%Y-%m-%d %H:%M:%S")
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_group_invite(group: &str, description: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.group_invite(group, description).await {
        Ok(resp) => {
            println!("API key created for group \"{}\":", resp.key_prefix);
            println!("  Key: {}", resp.key);
            println!("\nSave this key — it won't be shown again.");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_group_keys() {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.list_keys().await {
        Ok(keys) => {
            if keys.is_empty() {
                println!("No API keys.");
            } else {
                println!(
                    "{:<15} {:<10} {:<15} {:<20} {}",
                    "PREFIX", "ROLE", "GROUP", "DESCRIPTION", "CREATED"
                );
                for k in keys {
                    println!(
                        "{:<15} {:<10} {:<15} {:<20} {}",
                        k.key_prefix,
                        k.role,
                        k.group_name.as_deref().unwrap_or("-"),
                        k.description,
                        k.created_at.format("%Y-%m-%d %H:%M:%S")
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_group_revoke(key_prefix: &str) {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    match client.revoke_key(key_prefix).await {
        Ok(()) => println!("Key revoked."),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

// --- Delegate / Wait / Reply / Status ---

async fn cmd_delegate(worker: &str, task: &str) {
    let mut cfg = config::CliConfig::load();
    let lead_id = cfg.lead_id();
    let client = make_client(&cfg);

    if let Err(e) = client.get_worker(worker).await {
        eprintln!("Error: worker \"{}\" not found. {}", worker, e);
        eprintln!("Run 'bh worker ls' to see available workers.");
        std::process::exit(1);
    }

    if let Err(e) = client.register_agent(&lead_id).await {
        eprintln!("Error registering lead agent: {}", e);
        std::process::exit(1);
    }

    let thread_id = format!("thread-{}", &uuid::Uuid::new_v4().to_string()[..8]);

    match client
        .send_message(
            worker,
            &thread_id,
            &lead_id,
            "request",
            Some(&serde_json::json!(task)),
        )
        .await
    {
        Ok(_) => {
            let mut pending = config::CliConfig::load_pending();
            pending.threads.insert(
                thread_id.clone(),
                config::PendingThread {
                    worker: worker.to_string(),
                    task: task.to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    temp: false,
                },
            );
            let _ = config::CliConfig::save_pending(&pending);
            println!("{}", thread_id);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
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

        let messages = match client
            .get_inbox(&lead_id, Some("unread"), Some(10.0))
            .await
        {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Error: could not connect to server. ({})", e);
                std::process::exit(1);
            }
        };

        for msg in messages {
            if let Some(thread_info) = pending.threads.get(&msg.thread_id) {
                let elapsed = if let Ok(created) =
                    chrono::DateTime::parse_from_rfc3339(&thread_info.created_at)
                {
                    format!(
                        "{}s",
                        (chrono::Utc::now() - created.with_timezone(&chrono::Utc)).num_seconds()
                    )
                } else {
                    "?s".to_string()
                };

                let content = msg
                    .content
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no content)");

                let is_temp = thread_info.temp;
                let worker_name = thread_info.worker.clone();

                match msg.msg_type.as_str() {
                    "done" => {
                        println!("{} done ({}): {}", worker_name, elapsed, content);
                        pending.threads.remove(&msg.thread_id);
                        if is_temp {
                            let _ = client.remove_worker(&worker_name).await;
                        }
                    }
                    "failed" => {
                        eprintln!("{} failed ({}): {}", worker_name, elapsed, content);
                        pending.threads.remove(&msg.thread_id);
                        if is_temp {
                            let _ = client.remove_worker(&worker_name).await;
                        }
                    }
                    "question" => {
                        println!(
                            "\n{} asks (thread {}): {}\n  → Use: bh reply {} \"<your answer>\"",
                            worker_name, msg.thread_id, content, msg.thread_id
                        );
                    }
                    _ => {}
                }
            }

            let _ = client.ack_message(&msg.id).await;
        }

        let _ = config::CliConfig::save_pending(&pending);
    }
}

async fn cmd_reply(thread_id: &str, message: &str) {
    let mut cfg = config::CliConfig::load();
    let lead_id = cfg.lead_id();
    let client = make_client(&cfg);

    let pending = config::CliConfig::load_pending();
    let worker = match pending.threads.get(thread_id) {
        Some(t) => t.worker.clone(),
        None => {
            eprintln!("Error: thread \"{}\" not found in pending tasks.", thread_id);
            std::process::exit(1);
        }
    };

    match client
        .send_message(
            &worker,
            thread_id,
            &lead_id,
            "answer",
            Some(&serde_json::json!(message)),
        )
        .await
    {
        Ok(_) => println!("Reply sent to {} (thread {}).", worker, thread_id),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_status() {
    let cfg = config::CliConfig::load();
    let client = make_client(&cfg);

    println!("Server: {}", cfg.server_url());

    match client.health().await {
        Ok(version) => println!("Status: connected (v{})", version),
        Err(_) => {
            println!("Status: disconnected");
            return;
        }
    }

    if let Ok(nodes) = client.list_nodes().await {
        println!("Nodes: {}", nodes.len());
        for n in &nodes {
            println!("  {} ({})", n.id, n.status);
        }
    }

    if let Ok(workers) = client.list_workers().await {
        println!("Workers: {}", workers.len());
        for w in &workers {
            println!("  {} on {} ({})", w.name, w.node_id, w.status);
        }
    }

    let pending = config::CliConfig::load_pending();
    if pending.threads.is_empty() {
        println!("Pending tasks: none");
    } else {
        println!("Pending tasks: {}", pending.threads.len());
        for (tid, info) in &pending.threads {
            println!("  {} → {}", tid, info.worker);
        }
    }
}
