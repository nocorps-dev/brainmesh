use brainmesh_api::api_router;
use brainmesh_core::{
    AgentRegistrationRequest, AuthContext, ContributeKnowledgeRequest, RecallQuery,
    SubmitExperienceRequest, VerificationStatus, VerifyKnowledgeRequest,
};
use brainmesh_mcp::serve_stdio;
use brainmesh_storage::{connect_store, BrainMeshStore};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "brainmesh",
    version,
    about = "BrainMesh — Collective Intelligence Infrastructure for AI Agents"
)]
struct Cli {
    /// SQLite database URL or path (local-first default)
    #[arg(
        long,
        env = "BRAINMESH_DATABASE_URL",
        default_value = "sqlite://brainmesh.db?mode=rwc"
    )]
    db: String,

    /// Emit machine-readable JSON output
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, ValueEnum)]
enum McpTransport {
    Stdio,
    Http,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a local BrainMesh database and config
    Init,
    /// Manage the BrainMesh server
    Server {
        #[command(subcommand)]
        action: ServerAction,
    },
    /// Manage registered AI agents
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Generate MCP connection configuration for an agent framework
    Connect {
        /// Target framework (opencode, openclaw, hermes, claude-code)
        target: String,
    },
    /// Search the shared knowledge network
    Search { query: String },
    /// Recall relevant knowledge with provenance and verification details
    Recall {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Contribute structured knowledge from a JSON file
    Contribute { file: PathBuf },
    /// Submit an agent experience from a JSON file
    Learn {
        #[arg(long)]
        input: PathBuf,
    },
    /// Inspect, verify, or view history of a knowledge object
    Knowledge {
        #[command(subcommand)]
        action: KnowledgeAction,
    },
    /// Show BrainMesh network status
    Network {
        #[command(subcommand)]
        action: StatusAction,
    },
    /// Show federation status
    Federation {
        #[command(subcommand)]
        action: StatusAction,
    },
    /// Show effective configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Run environment and database health diagnostics
    Doctor,
    /// Start the BrainMesh MCP server
    Mcp {
        #[arg(long, value_enum, default_value_t = McpTransport::Stdio)]
        transport: McpTransport,
        #[arg(long, default_value = "127.0.0.1:8081")]
        addr: String,
    },
}

#[derive(Subcommand)]
enum ServerAction {
    Start {
        #[arg(long, default_value = "127.0.0.1:8080")]
        addr: String,
        /// Require Bearer API key authentication on REST & MCP HTTP endpoints
        #[arg(long, default_value_t = false)]
        auth: bool,
    },
    Status,
}

#[derive(Subcommand)]
enum AgentAction {
    Register {
        #[arg(long)]
        name: String,
        #[arg(long)]
        framework: String,
        #[arg(long)]
        model: Option<String>,
    },
    List,
}

#[derive(Subcommand)]
enum KnowledgeAction {
    Show { id: String },
    Verify {
        id: String,
        #[arg(long, default_value = "cli_verification")]
        method: String,
    },
    History { id: String },
}

#[derive(Subcommand)]
enum StatusAction {
    Status,
}

#[derive(Subcommand)]
enum ConfigAction {
    Show,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let cli = Cli::parse();
    let store: Arc<dyn BrainMeshStore> = connect_store(&cli.db).await?;
    let ctx = AuthContext::local_admin();

    match cli.command {
        Commands::Init => {
            store.health_check().await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "initialized", "database": cli.db })
                );
            } else {
                println!("Initialized BrainMesh local store at {}", cli.db);
            }
        }
        Commands::Server { action } => match action {
            ServerAction::Start { addr, auth } => {
                let app = api_router(store, auth);
                let listener = tokio::net::TcpListener::bind(&addr).await?;
                eprintln!("BrainMesh server listening on http://{addr} (auth={auth})");
                axum::serve(listener, app).await?;
            }
            ServerAction::Status => {
                let status = store.network_status().await?;
                println!("{}", serde_json::to_string_pretty(&status)?);
            }
        },
        Commands::Agent { action } => match action {
            AgentAction::Register {
                name,
                framework,
                model,
            } => {
                let resp = store
                    .register_agent(AgentRegistrationRequest {
                        name,
                        framework,
                        model,
                        organization_id: None,
                        workspace_id: None,
                        capabilities: Vec::new(),
                    })
                    .await?;
                println!("{}", serde_json::to_string_pretty(&resp)?);
            }
            AgentAction::List => {
                let agents = store.list_agents(&ctx).await?;
                println!("{}", serde_json::to_string_pretty(&agents)?);
            }
        },
        Commands::Connect { target } => {
            let cfg = serde_json::json!({
                "target": target,
                "mcpServers": {
                    "brainmesh": {
                        "command": "brainmesh",
                        "args": ["--db", cli.db, "mcp", "--transport", "stdio"]
                    }
                }
            });
            println!("{}", serde_json::to_string_pretty(&cfg)?);
        }
        Commands::Search { query } => {
            let res = store
                .recall(
                    &ctx,
                    RecallQuery {
                        query,
                        domain: None,
                        framework: None,
                        tags: Vec::new(),
                        verification_status: None,
                        limit: 10,
                    },
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&res)?);
        }
        Commands::Recall { query, limit } => {
            let res = store
                .recall(
                    &ctx,
                    RecallQuery {
                        query,
                        domain: None,
                        framework: None,
                        tags: Vec::new(),
                        verification_status: None,
                        limit,
                    },
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&res)?);
        }
        Commands::Contribute { file } => {
            let raw = std::fs::read_to_string(&file)?;
            let req: ContributeKnowledgeRequest = serde_json::from_str(&raw)?;
            let created = store.contribute_knowledge(&ctx, req).await?;
            println!("{}", serde_json::to_string_pretty(&created)?);
        }
        Commands::Learn { input } => {
            let raw = std::fs::read_to_string(&input)?;
            let req: SubmitExperienceRequest = serde_json::from_str(&raw)?;
            let exp = store.submit_experience(&ctx, req).await?;
            println!("{}", serde_json::to_string_pretty(&exp)?);
        }
        Commands::Knowledge { action } => match action {
            KnowledgeAction::Show { id } => {
                let k = store.get_knowledge(&ctx, &id).await?;
                println!("{}", serde_json::to_string_pretty(&k)?);
            }
            KnowledgeAction::Verify { id, method } => {
                let v = store
                    .verify_knowledge(
                        &ctx,
                        &id,
                        VerifyKnowledgeRequest {
                            status: VerificationStatus::IndependentlyConfirmed,
                            method,
                            evidence: Vec::new(),
                            conditions: None,
                        },
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
            KnowledgeAction::History { id } => {
                let h = store.get_verification_history(&ctx, &id).await?;
                println!("{}", serde_json::to_string_pretty(&h)?);
            }
        },
        Commands::Network { action: _ } => {
            let status = store.network_status().await?;
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        Commands::Federation { action: _ } => {
            let nodes = store.list_federation_nodes(&ctx).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "ready",
                    "nodes": nodes
                }))?
            );
        }
        Commands::Config { action: _ } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "database_url": cli.db,
                    "mode": "local_sqlite"
                }))?
            );
        }
        Commands::Doctor => {
            store.health_check().await?;
            let status = store.network_status().await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "database": "ok",
                    "network": status
                }))?
            );
        }
        Commands::Mcp { transport, addr } => match transport {
            McpTransport::Stdio => {
                serve_stdio(store, ctx).await?;
            }
            McpTransport::Http => {
                let app = brainmesh_mcp::mcp_router(store, false);
                let listener = tokio::net::TcpListener::bind(&addr).await?;
                eprintln!("BrainMesh MCP HTTP listening on http://{addr}/mcp");
                axum::serve(listener, app).await?;
            }
        },
    }

    Ok(())
}
