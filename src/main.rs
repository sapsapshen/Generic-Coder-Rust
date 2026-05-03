use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::Value;
use tokio::sync::{mpsc, RwLock};

use generic_coder::agent::GenericAgent;
use generic_coder::{config, web};

#[derive(Parser)]
#[command(
    name = "generic-coder",
    version,
    about = "Generic Coder autonomous development cockpit"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long)]
    task: Option<String>,

    #[arg(long)]
    reflect: Option<String>,

    #[arg(long)]
    input: Option<String>,

    #[arg(long, default_value = "0")]
    llm_no: usize,

    #[arg(long, default_value = "false")]
    verbose: bool,

    #[arg(long, default_value = "false")]
    bg: bool,
}

#[derive(Subcommand)]
enum Commands {
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        #[arg(long, default_value_t = 8765)]
        port: u16,
    },
}

fn script_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn project_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("GENERIC_CODER_PROJECT_DIR") {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            return path;
        }
    }

    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(dir);
    }

    if let Ok(dir) = std::env::current_dir() {
        if dir.join("Cargo.toml").is_file()
            || dir
                .join("assets")
                .join("generic_coder")
                .join("index.html")
                .is_file()
        {
            return dir;
        }
    }

    let exe_dir = script_dir();
    for candidate in exe_dir.ancestors() {
        if candidate.join("Cargo.toml").is_file()
            || candidate
                .join("assets")
                .join("generic_coder")
                .join("index.html")
                .is_file()
        {
            return candidate.to_path_buf();
        }
    }

    exe_dir
}

fn read_file(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

fn consume_file(dir: &Path, name: &str) -> Option<String> {
    let path = dir.join(name);
    let content = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(path);
    Some(content)
}

fn sanitize_identifier(label: &str, value: &str) -> Result<String> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(anyhow!(
            "{label} must contain only letters, numbers, '.', '_' or '-'"
        ));
    }
    Ok(value.to_string())
}

async fn initialize_runtime(
    project_dir: &Path,
    llm_no: usize,
    verbose: bool,
) -> Result<(
    Arc<RwLock<GenericAgent>>,
    mpsc::Sender<(String, String, mpsc::Sender<Value>)>,
)> {
    let cfg = config::load_config(project_dir);
    let system_prompt = config::get_system_prompt(project_dir);
    let tools_schema = config::load_tool_schema(project_dir, None);

    let mut agent = GenericAgent::new();
    agent.load_llm_sessions(&cfg.llm_configs, &cfg.mixin_configs)?;
    if !agent.llm_clients.is_empty() {
        agent.next_llm(llm_no as isize)?;
    } else {
        log::warn!(
            "No LLM clients configured. Save one in the Rust web UI settings or create mykey.json."
        );
    }
    agent.verbose = verbose;

    let agent = Arc::new(RwLock::new(agent));
    let (task_tx, mut task_rx) = mpsc::channel::<(String, String, mpsc::Sender<Value>)>(16);
    let background_agent = agent.clone();

    tokio::spawn(async move {
        while let Some((query, source, display_tx)) = task_rx.recv().await {
            let mut runtime = background_agent.write().await;
            runtime
                .run_task(
                    query,
                    source,
                    display_tx,
                    system_prompt.clone(),
                    tools_schema.clone(),
                )
                .await;
        }
    });

    Ok((agent, task_tx))
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();
    let project_dir = project_dir();

    if cli.bg {
        let current_exe = std::env::current_exe()?;
        let args: Vec<String> = std::env::args()
            .skip(1)
            .filter(|arg| arg != "--bg")
            .collect();
        let safe_task_name = cli
            .task
            .as_deref()
            .map(|task_name| sanitize_identifier("Task name", task_name))
            .transpose()?;
        let task_dir = safe_task_name
            .as_ref()
            .map(|task_name| project_dir.join("temp").join(task_name));
        if let Some(dir) = &task_dir {
            std::fs::create_dir_all(dir).ok();
        }

        let mut command = std::process::Command::new(current_exe);
        command.args(&args).current_dir(&project_dir);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        if let Some(dir) = &task_dir {
            command.stdout(std::fs::File::create(dir.join("stdout.log"))?);
            command.stderr(std::fs::File::create(dir.join("stderr.log"))?);
        }

        let child = command
            .spawn()
            .context("failed to spawn background process")?;
        println!("{}", child.id());
        return Ok(());
    }

    let (agent, task_tx) = initialize_runtime(&project_dir, cli.llm_no, cli.verbose).await?;

    if let Some(Commands::Serve { host, port }) = cli.command {
        let allow_remote = std::env::var("GENERIC_CODER_ALLOW_REMOTE")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if !allow_remote && host != "127.0.0.1" && host != "::1" && host != "localhost" {
            anyhow::bail!(
                "Refusing to bind to non-loopback host {host}. Set GENERIC_CODER_ALLOW_REMOTE=1 to override."
            );
        }
        return web::serve(web::ServeConfig {
            host,
            port,
            project_dir,
            agent,
            task_tx,
        })
        .await;
    }

    if let Some(task_name) = cli.task {
        let task_name = sanitize_identifier("Task name", &task_name)?;
        let task_dir = project_dir.join("temp").join(task_name);
        std::fs::create_dir_all(&task_dir)?;
        let input_file = task_dir.join("input.txt");

        if let Some(input) = cli.input {
            if let Ok(entries) = std::fs::read_dir(&task_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("output") && name.ends_with(".txt") {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
            write_file(&input_file, &input)?;
        }

        let mut raw = read_file(&input_file);
        let mut round = 0usize;

        loop {
            let (display_tx, mut display_rx) = mpsc::channel::<Value>(256);
            if task_tx
                .send((raw.clone(), "task".into(), display_tx))
                .await
                .is_err()
            {
                break;
            }

            let mut final_output = String::new();
            loop {
                let timeout = tokio::time::timeout(Duration::from_secs(120), display_rx.recv());
                match timeout.await {
                    Ok(Some(item)) => {
                        if let Some(next) = item.get("next").and_then(|value| value.as_str()) {
                            final_output = next.to_string();
                            let label = if round == 0 {
                                String::new()
                            } else {
                                round.to_string()
                            };
                            let _ = write_file(&task_dir.join(format!("output{label}.txt")), next);
                        }
                        if let Some(done) = item.get("done").and_then(|value| value.as_str()) {
                            final_output = done.to_string();
                            break;
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }

            let label = if round == 0 {
                String::new()
            } else {
                round.to_string()
            };
            write_file(
                &task_dir.join(format!("output{label}.txt")),
                &format!("{final_output}\n\n[ROUND END]\n"),
            )?;
            consume_file(&task_dir, "_stop");

            let mut reply = None;
            for _ in 0..300 {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if let Some(value) = consume_file(&task_dir, "reply.txt") {
                    reply = Some(value);
                    break;
                }
            }

            match reply {
                Some(value) => {
                    raw = value;
                    round += 1;
                }
                None => break,
            }
        }

        return Ok(());
    }

    if let Some(script_name) = cli.reflect {
        let script_name = sanitize_identifier("Reflect name", &script_name)?;
        let log_dir = project_dir.join("temp").join("reflect_logs");
        std::fs::create_dir_all(&log_dir)?;
        let trigger_file = project_dir
            .join("temp")
            .join(format!("_reflect_{}_trigger", script_name));
        println!("[Reflect] watching {}", trigger_file.display());

        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if !trigger_file.exists() {
                continue;
            }

            let task = match std::fs::read_to_string(&trigger_file) {
                Ok(content) if !content.trim().is_empty() => content.trim().to_string(),
                _ => continue,
            };
            let _ = std::fs::remove_file(&trigger_file);

            let (display_tx, mut display_rx) = mpsc::channel::<Value>(256);
            let _ = task_tx.send((task, "reflect".into(), display_tx)).await;

            let mut result = String::new();
            while let Some(item) = display_rx.recv().await {
                if let Some(done) = item.get("done").and_then(|value| value.as_str()) {
                    result = done.to_string();
                    break;
                }
            }

            println!("{result}");

            let today = chrono::Local::now().format("%Y-%m-%d");
            let log_file = log_dir.join(format!("{}_{}.log", script_name, today));
            let timestamp = chrono::Local::now().format("%m-%d %H:%M");
            let existing = read_file(&log_file);
            write_file(&log_file, &format!("{existing}[{timestamp}]\n{result}\n\n"))?;
        }
    }

    println!("Generic Coder v{}", env!("CARGO_PKG_VERSION"));
    println!("Type a query and press Enter. Ctrl+C to interrupt, Ctrl+D or 'exit' to quit.\n");
    print_prompt(&agent).await;

    loop {
        let Some(line) = read_line_or_interrupt(&agent).await else {
            break;
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            print_prompt(&agent).await;
            continue;
        }
        if line == "exit" || line == "quit" {
            break;
        }

        let (display_tx, mut display_rx) = mpsc::channel::<Value>(256);
        if task_tx
            .send((line, "user".into(), display_tx))
            .await
            .is_err()
        {
            eprintln!("Agent task queue closed.");
            break;
        }

        loop {
            tokio::select! {
                item = display_rx.recv() => {
                    match item {
                        Some(value) => {
                            if let Some(next) = value.get("next").and_then(|v| v.as_str()) {
                                print!("{next}");
                            }
                            if value.get("done").is_some() {
                                println!();
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    let runtime = agent.read().await;
                    runtime.abort();
                    eprintln!("\n[Interrupted]\n");
                    break;
                }
            }
        }

        print_prompt(&agent).await;
    }

    Ok(())
}

async fn print_prompt(agent: &Arc<RwLock<GenericAgent>>) {
    let model = agent.read().await.get_llm_name(true);
    if model.is_empty() {
        print!("gc> ");
    } else {
        print!("gc[{model}]> ");
    }
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
}

async fn read_line_or_interrupt(agent: &Arc<RwLock<GenericAgent>>) -> Option<String> {
    tokio::task::spawn_blocking(|| {
        let mut input = String::new();
        match std::io::stdin().read_line(&mut input) {
            Ok(0) => None,
            Ok(_) => Some(input),
            Err(_) => None,
        }
    })
    .await
    .ok()
    .flatten()
    .or_else(|| {
        futures::executor::block_on(async {
            agent.read().await.abort();
            None
        })
    })
}
