use std::fs;
use std::io::BufReader;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{Connection, params};
use rustls::{ClientConfig, RootCertStore};
use serde_json::{Map, Value, json};
use taskforce::backend::TaskStatus;
use taskforce::config::{
    AppConfig, BackendKind, base_env_file_path, bootstrap_environment_from_args, env_file_path,
    resolve_environment_from_sources,
};
use tokio_postgres::types::ToSql;
use tokio_postgres_rustls::MakeRustlsConnect;

#[tokio::main]
async fn main() -> Result<()> {
    bootstrap_env()?;
    let options = SeedOptions::parse()?;
    let config = AppConfig::load()?;

    match config.backend.kind {
        BackendKind::Sqlite => seed_sqlite(&config, &options)?,
        BackendKind::Postgres => seed_postgres(&config, &options).await?,
    }

    print_examples(&options);
    Ok(())
}

fn bootstrap_env() -> Result<()> {
    if let Some(path) = base_env_file_path()
        && path.exists()
    {
        dotenvy::from_path(&path)?;
    }

    let cli_env = bootstrap_environment_from_args(std::env::args_os())?;
    if let Some(environment) = resolve_environment_from_sources(cli_env)? {
        unsafe {
            std::env::set_var("TASKFORCE_ENV", environment);
        }
    }

    if let Some(path) = env_file_path()
        && path.exists()
    {
        dotenvy::from_path_override(&path)?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct SeedOptions {
    env: Option<String>,
    seed: u64,
    start: u64,
    count: u64,
    project: String,
}

impl SeedOptions {
    fn parse() -> Result<Self> {
        let mut env = None;
        let mut seed = 42_u64;
        let mut start = 1_u64;
        let mut count = 24_u64;
        let mut project = None;

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--env" => env = Some(next_value(&mut args, "--env")?),
                "--seed" => seed = parse_u64(next_value(&mut args, "--seed")?, "--seed")?,
                "--start" => start = parse_u64(next_value(&mut args, "--start")?, "--start")?,
                "--count" => count = parse_u64(next_value(&mut args, "--count")?, "--count")?,
                "--project" => project = Some(next_value(&mut args, "--project")?),
                other => bail!("unknown argument: {other}"),
            }
        }

        if count == 0 {
            bail!("--count must be greater than zero");
        }

        Ok(Self {
            env,
            seed,
            start,
            count,
            project: project.unwrap_or_else(|| format!("seed-{seed}")),
        })
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .ok_or_else(|| anyhow!("{flag} requires a value"))
}

fn parse_u64(value: String, flag: &str) -> Result<u64> {
    value
        .parse::<u64>()
        .with_context(|| format!("{flag} must be an integer"))
}

#[derive(Debug, Clone)]
struct SeedTask {
    ordinal: u64,
    uuid: String,
    title: String,
    description: String,
    status: TaskStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    target_date: Option<NaiveDate>,
    deadline: Option<NaiveDate>,
    launch_date: Option<NaiveDate>,
    target_time_hint: Option<String>,
    deadline_time_hint: Option<String>,
    launch_time_hint: Option<String>,
    project: String,
    tags: Vec<String>,
    extra: Map<String, Value>,
}

fn build_seed_tasks(options: &SeedOptions) -> Vec<SeedTask> {
    let statuses = [
        TaskStatus::Unstarted,
        TaskStatus::Active,
        TaskStatus::Active,
        TaskStatus::Waiting,
        TaskStatus::Waiting,
        TaskStatus::Done,
        TaskStatus::Suspended,
        TaskStatus::Abandoned,
        TaskStatus::Mistaken,
        TaskStatus::Duplicated,
    ];
    let requesters = ["石井", "田中", "佐藤", "高橋", "鈴木"];
    let summaries = [
        "LP バナー差し替え",
        "特設ページ文言調整",
        "QA 依頼の整理",
        "本番反映チェック",
        "デザイン確認",
    ];
    let verbs = ["Refresh", "Audit", "Coordinate", "Prepare", "Review", "Polish"];
    let targets = [
        "summer LP",
        "pricing page",
        "recruit site",
        "campaign banner",
        "checkout flow",
        "release checklist",
    ];
    let deadlines = [
        None,
        Some("2026-06-03"),
        Some("2026-06-07"),
        Some("2026-06-10"),
        Some("2026-06-14"),
        Some("2026-06-20"),
    ];
    let launch_dates = [
        None,
        Some("2026-06-04"),
        Some("2026-06-09"),
        Some("2026-06-12"),
        Some("2026-06-18"),
    ];
    let time_hints = [None, Some("am"), Some("pm"), Some("eod")];
    let tag_groups = [
        &["ops", "release"][..],
        &["design", "qa"][..],
        &["backend", "bug"][..],
        &["frontend", "copy"][..],
        &["ops", "hotfix"][..],
        &["design", "handoff"][..],
    ];

    let base_time = Utc::now() - Duration::days(14);
    let mut tasks = Vec::with_capacity(options.count as usize);

    for offset in 0..options.count {
        let ordinal = options.start + offset;
        let index = options.seed + ordinal;
        let requester = pick(&requesters, index * 3);
        let summary = pick(&summaries, index * 5);
        let verb = pick(&verbs, index * 11);
        let target = pick(&targets, index * 13);
        let deadline = parse_optional_date(*pick(&deadlines, index * 17));
        let launch_date = parse_optional_date(*pick(&launch_dates, index * 19));
        let target_hint = pick(&time_hints, index * 23).map(ToString::to_string);
        let deadline_hint = pick(&time_hints, index * 29).map(ToString::to_string);
        let launch_hint = pick(&time_hints, index * 31).map(ToString::to_string);
        let tags = pick(&tag_groups, index * 37)
            .iter()
            .map(|tag| (*tag).to_string())
            .collect::<Vec<_>>();
        let created_at = base_time + Duration::minutes((ordinal as i64) * 17);
        let production_release = ordinal.is_multiple_of(2);
        let uuid = format!("seed-{}-{ordinal:04}", options.seed);
        let title = format!("{verb} {target} #{ordinal}");
        let description =
            format!("{summary} / requester={requester} / project={}", options.project);
        let status = *pick(&statuses, index * 7);

        let mut extra = Map::new();
        extra.insert(
            "chatwork".to_string(),
            json!({
                "requester": requester,
                "summary": summary,
                "production_release": production_release,
            }),
        );

        tasks.push(SeedTask {
            ordinal,
            uuid,
            title,
            description,
            status,
            created_at,
            updated_at: created_at,
            target_date: deadline.map(|date| date - Duration::days(2)),
            deadline,
            launch_date,
            target_time_hint: target_hint,
            deadline_time_hint: deadline_hint,
            launch_time_hint: launch_hint,
            project: options.project.clone(),
            tags: {
                let mut value = vec!["seeded".to_string()];
                value.extend(tags);
                value
            },
            extra,
        });
    }

    tasks
}

fn pick<T>(values: &[T], index: u64) -> &T {
    &values[(index as usize) % values.len()]
}

fn parse_optional_date(value: Option<&str>) -> Option<NaiveDate> {
    value.and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
}

fn seed_sqlite(config: &AppConfig, options: &SeedOptions) -> Result<()> {
    let path = config.resolve_sqlite_path()?;
    let mut connection = Connection::open(path)?;
    let tx = connection.transaction()?;
    ensure_sqlite_schema(&tx)?;
    let mut statement = tx.prepare(
        r#"
        INSERT INTO tasks (
          uuid,
          title,
          description,
          status_id,
          created_at,
          updated_at,
          target_date,
          deadline,
          launch_date,
          target_time_hint,
          deadline_time_hint,
          launch_time_hint,
          project,
          tags_json,
          extra_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        "#,
    )?;

    for task in build_seed_tasks(options) {
        statement.execute(params![
            task.uuid,
            task.title,
            task.description,
            task_status_id(task.status),
            task.created_at.to_rfc3339(),
            task.updated_at.to_rfc3339(),
            task.target_date.map(|value| value.to_string()),
            task.deadline.map(|value| value.to_string()),
            task.launch_date.map(|value| value.to_string()),
            task.target_time_hint,
            task.deadline_time_hint,
            task.launch_time_hint,
            task.project,
            serde_json::to_string(&task.tags)?,
            serde_json::to_string(&task.extra)?,
        ])?;
        println!(
            "seeded sqlite ordinal={} title={} status={}",
            task.ordinal, task.title, task.status
        );
    }

    drop(statement);
    tx.commit()?;
    Ok(())
}

async fn seed_postgres(config: &AppConfig, options: &SeedOptions) -> Result<()> {
    let mut client = connect_postgres(
        &config.resolve_postgres_url()?,
        config.resolve_postgres_ssl_root_cert()?.as_deref(),
    )
    .await?;

    ensure_postgres_schema(&client).await?;
    let transaction = client.transaction().await?;
    let statement = transaction
        .prepare(
            r#"
            INSERT INTO tasks (
              uuid,
              title,
              description,
              status_id,
              created_at,
              updated_at,
              target_date,
              deadline,
              launch_date,
              target_time_hint,
              deadline_time_hint,
              launch_time_hint,
              project,
              tags_json,
              extra_json
            ) VALUES (
              $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15
            )
            "#,
        )
        .await?;

    for task in build_seed_tasks(options) {
        let tags_json = serde_json::to_value(&task.tags)?;
        let extra_json = serde_json::to_value(&task.extra)?;
        let params: [&(dyn ToSql + Sync); 15] = [
            &task.uuid,
            &task.title,
            &task.description,
            &task_status_id(task.status),
            &task.created_at,
            &task.updated_at,
            &task.target_date,
            &task.deadline,
            &task.launch_date,
            &task.target_time_hint,
            &task.deadline_time_hint,
            &task.launch_time_hint,
            &task.project,
            &tags_json,
            &extra_json,
        ];
        transaction.execute(&statement, &params).await?;
        println!(
            "seeded postgres ordinal={} title={} status={}",
            task.ordinal, task.title, task.status
        );
    }

    transaction.commit().await?;
    Ok(())
}

fn ensure_sqlite_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS task_statuses (
          id INTEGER PRIMARY KEY,
          name TEXT NOT NULL UNIQUE
        );
        CREATE TABLE IF NOT EXISTS tasks (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          uuid TEXT NOT NULL UNIQUE,
          title TEXT NOT NULL,
          description TEXT,
          status_id INTEGER NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          target_date TEXT,
          deadline TEXT,
          launch_date TEXT,
          target_time_hint TEXT,
          deadline_time_hint TEXT,
          launch_time_hint TEXT,
          project TEXT,
          tags_json TEXT NOT NULL,
          extra_json TEXT NOT NULL,
          FOREIGN KEY(status_id) REFERENCES task_statuses(id)
        );
        "#,
    )?;
    seed_sqlite_statuses(connection)
}

fn seed_sqlite_statuses(connection: &Connection) -> Result<()> {
    for (id, name) in task_status_pairs() {
        connection.execute(
            "INSERT INTO task_statuses (id, name) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name",
            params![id, name],
        )?;
    }
    Ok(())
}

async fn ensure_postgres_schema(client: &tokio_postgres::Client) -> Result<()> {
    client
        .batch_execute(
            r#"
            CREATE TABLE IF NOT EXISTS task_statuses (
              id BIGINT PRIMARY KEY,
              name TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS tasks (
              id BIGSERIAL PRIMARY KEY,
              uuid TEXT NOT NULL UNIQUE,
              title TEXT NOT NULL,
              description TEXT,
              status_id BIGINT NOT NULL REFERENCES task_statuses(id),
              created_at TIMESTAMPTZ NOT NULL,
              updated_at TIMESTAMPTZ NOT NULL,
              target_date DATE,
              deadline DATE,
              launch_date DATE,
              target_time_hint TEXT,
              deadline_time_hint TEXT,
              launch_time_hint TEXT,
              project TEXT,
              tags_json JSONB NOT NULL DEFAULT '[]'::jsonb,
              extra_json JSONB NOT NULL DEFAULT '{}'::jsonb
            );
            "#,
        )
        .await?;
    seed_postgres_statuses(client).await
}

async fn seed_postgres_statuses(client: &tokio_postgres::Client) -> Result<()> {
    for (id, name) in task_status_pairs() {
        client
            .execute(
                "INSERT INTO task_statuses (id, name) VALUES ($1, $2)
                 ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name",
                &[&id, &name],
            )
            .await?;
    }
    Ok(())
}

fn task_status_pairs() -> [(i64, &'static str); 8] {
    [
        (1, "unstarted"),
        (2, "active"),
        (3, "suspended"),
        (4, "done"),
        (5, "abandoned"),
        (6, "mistaken"),
        (7, "duplicated"),
        (8, "waiting"),
    ]
}

fn task_status_id(status: TaskStatus) -> i64 {
    match status {
        TaskStatus::Unstarted => 1,
        TaskStatus::Active => 2,
        TaskStatus::Suspended => 3,
        TaskStatus::Done => 4,
        TaskStatus::Abandoned => 5,
        TaskStatus::Mistaken => 6,
        TaskStatus::Duplicated => 7,
        TaskStatus::Waiting => 8,
    }
}

async fn connect_postgres(
    connection_url: &str,
    ssl_root_cert: Option<&Path>,
) -> Result<tokio_postgres::Client> {
    let tls = MakeRustlsConnect::new(build_client_config(ssl_root_cert)?);
    let (client, connection) = tokio_postgres::connect(connection_url, tls).await?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });
    Ok(client)
}

fn build_client_config(ssl_root_cert: Option<&Path>) -> Result<ClientConfig> {
    let mut roots = RootCertStore::empty();

    if let Some(path) = ssl_root_cert {
        let file = fs::File::open(path)
            .with_context(|| format!("failed to open Postgres root certificate {}", path.display()))?;
        let mut reader = BufReader::new(file);
        let certs = rustls_pemfile::certs(&mut reader).collect::<std::io::Result<Vec<_>>>()?;
        let (_added, ignored) = roots.add_parsable_certificates(certs);
        if ignored > 0 || roots.is_empty() {
            bail!("failed to load any CA certificates from {}", path.display());
        }
    } else {
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }

    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(config)
}

fn print_examples(options: &SeedOptions) {
    let env = options.env.as_deref().unwrap_or("development");
    println!();
    println!("Done.");
    println!();
    println!("Example searches:");
    println!("  target/debug/taskforce --env={env} search --where \"project = '{}'\"", options.project);
    println!(
        "  target/debug/taskforce --env={env} search --where \"project = '{}' and status in ('active', 'waiting')\"",
        options.project
    );
    println!(
        "  target/debug/taskforce --env={env} search --where \"project = '{}' and chatwork.requester = '石井'\"",
        options.project
    );
}
