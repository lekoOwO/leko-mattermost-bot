use sqlx::{Connection, Executor, SqliteConnection};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Determine repository root by searching for Cargo.toml upwards from current dir
    let repo_root = find_repo_root()?;

    println!("Working directory: {}", repo_root.display());

    // Create a random temp db file in the system temp directory
    let db_file = make_temp_db_path(&repo_root)?;
    println!("Preparing sqlite DB file at: {}", db_file.display());

    // Ensure the file does not exist
    let _ = fs::remove_file(&db_file);

    // Create the empty file
    fs::File::create(&db_file)?;

    // Keep track to remove the file on exit
    let _db_cleanup = FileRemover::new(db_file.clone());

    // Schema source (must exist in repo)
    let schema_src = repo_root.join("src").join("schema.sql");
    if !schema_src.exists() {
        return Err(format!("schema file not found: {}", schema_src.display()).into());
    }
    println!("Using schema from {}", schema_src.display());
    let schema_contents = fs::read_to_string(&schema_src)?;

    // Apply schema using sqlx (in-process) via a small tokio runtime
    println!("Applying schema via sqlx");
    let database_url = format!("sqlite:{}", db_file.display());
    let rt = Runtime::new()?;
    rt.block_on(async {
        let mut conn = SqliteConnection::connect(&database_url).await?;
        conn.execute(schema_contents.as_str()).await?;
        Ok::<(), sqlx::Error>(())
    })?;

    // Set DATABASE_URL and run cargo sqlx prepare -- --bin leko-mattermost-bot
    let database_url = format!("sqlite:{}", db_file.display());
    println!("DATABASE_URL={}", database_url);

    // Check for cargo sqlx availability (cargo sqlx prepare --version)
    let sqlx_available = Command::new("cargo")
        .arg("sqlx")
        .arg("prepare")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !sqlx_available {
        eprintln!(
            "Please install sqlx-cli if you haven't: cargo install -f sqlx-cli --no-default-features --features sqlite"
        );
    }

    // Allow an env var to request preparing queries for all targets (useful
    // when running tests). If SQLX_PREPARE_ALL=1 then run `cargo sqlx prepare`
    // without the `-- --bin` filter so the query cache covers tests and other
    // targets as well.
    let prepare_all = env::var("SQLX_PREPARE_ALL").unwrap_or_default() == "1";

    if prepare_all {
        println!("Running: cargo sqlx prepare (all targets)");
    } else {
        println!("Running: cargo sqlx prepare -- --bin leko-mattermost-bot");
    }

    let mut prepare = Command::new("cargo");
    prepare.arg("sqlx").arg("prepare");
    if !prepare_all {
        prepare.arg("--").arg("--bin").arg("leko-mattermost-bot");
    }
    let mut prepare = prepare.env("DATABASE_URL", &database_url).spawn()?;

    let prepare_status = prepare.wait()?;

    if !prepare_status.success() {
        eprintln!(
            "cargo sqlx prepare failed with exit code: {}",
            prepare_status
        );
        // Exit with the same code if possible
        if let Some(code) = prepare_status.code() {
            std::process::exit(code);
        } else {
            return Err("cargo sqlx prepare terminated by signal".into());
        }
    }

    println!("cargo sqlx prepare completed successfully.");

    // FileRemover will delete the DB file on drop here
    Ok(())
}

// Note: schema is read from `src/schema.sql` at runtime. If you want to run this
// file with `cargo-script`, you'll need to have `cargo-script` installed and
// provide compatible dependency flags. Running with `cargo run --bin sqlx_prepare`
// is recommended inside this workspace.

fn unique_stamp() -> Result<String, io::Error> {
    let pid = std::process::id();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    Ok(format!("{}_{}", pid, now.as_nanos()))
}

fn make_temp_db_path(_repo_root: &Path) -> Result<PathBuf, io::Error> {
    let fname = format!("leko_sqlx_prepare_{}.db", unique_stamp()?);
    let mut p = env::temp_dir();
    p.push(fname);
    Ok(p)
}

fn find_repo_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Start from current directory and walk upwards looking for Cargo.toml
    let mut dir = env::current_dir()?;
    loop {
        if dir.join("Cargo.toml").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    Err("Could not find Cargo.toml in current or parent directories".into())
}

struct FileRemover {
    path: PathBuf,
}

impl FileRemover {
    fn new(path: PathBuf) -> Self {
        FileRemover { path }
    }
}

impl Drop for FileRemover {
    fn drop(&mut self) {
        if self.path.exists() {
            match fs::remove_file(&self.path) {
                Ok(_) => eprintln!("Removed temp DB file: {}", self.path.display()),
                Err(err) => eprintln!(
                    "Failed to remove temp DB file {}: {}",
                    self.path.display(),
                    err
                ),
            }
        }
    }
}
