use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use dialoguer::{Confirm, Input, Password, Select, theme::ColorfulTheme};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(
    name = "paladen",
    about = "Account/credential manager with SSH launch"
)]
struct Cli {
    /// Path to SQLite DB (default: ~/.config/paladen/paladen.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// List all accounts
    List,
    /// Show one account's details
    Show { id: i64 },
    /// Add a new account (interactive)
    Add,
    /// Edit an existing account (interactive)
    Edit { id: i64 },
    /// Delete an account
    Delete { id: i64 },
    /// Pick an SSH account and connect
    Ssh {
        /// Account id (if omitted, opens picker)
        id: Option<i64>,
        /// Arguments to pass to ssh (e.g. command to run)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Use SCP to transfer files
    Scp {
        /// Account id
        id: i64,
        /// Scp arguments (e.g. src dest)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Import accounts from a plain-text file like account.txt
    Import { file: PathBuf },
    /// Interactive menu (default when no subcommand given)
    Menu,
}

#[derive(Debug, Clone)]
struct Account {
    id: i64,
    label: String,
    kind: String,
    subdomain: String,
    host: String,
    port: i64,
    username: String,
    password: String,
    path: String,
    url: String,
    notes: String,
}

impl Account {
    fn empty() -> Self {
        Self {
            id: 0,
            label: String::new(),
            kind: "SSH".into(),
            subdomain: String::new(),
            host: String::new(),
            port: 22,
            username: String::new(),
            password: String::new(),
            path: String::new(),
            url: String::new(),
            notes: String::new(),
        }
    }
}

fn db_path(cli_db: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = cli_db {
        return Ok(p.clone());
    }
    let mut p = dirs::config_dir().ok_or_else(|| anyhow!("no config dir"))?;
    p.push("paladen");
    std::fs::create_dir_all(&p)?;
    p.push("paladen.db");
    Ok(p)
}

fn open_db(path: &PathBuf) -> Result<Connection> {
    let conn = Connection::open(path)?;
    init_db(&conn)?;
    Ok(conn)
}

fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS accounts (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            label     TEXT NOT NULL,
            kind      TEXT NOT NULL DEFAULT 'SSH',
            subdomain TEXT NOT NULL DEFAULT '',
            host      TEXT NOT NULL DEFAULT '',
            port      INTEGER NOT NULL DEFAULT 22,
            username  TEXT NOT NULL DEFAULT '',
            password  TEXT NOT NULL DEFAULT '',
            path      TEXT NOT NULL DEFAULT '',
            url       TEXT NOT NULL DEFAULT '',
            notes     TEXT NOT NULL DEFAULT ''
        );",
    )?;
    Ok(())
}

fn import_section_kind(section: &str) -> Option<&'static str> {
    match section.trim().to_ascii_lowercase().as_str() {
        "ssh" | "akun ssh" => Some("SSH"),
        "cyberpanel" => Some("Cyberpanel"),
        "panel" => Some("Panel"),
        _ => None,
    }
}

fn list_accounts(conn: &Connection) -> Result<Vec<Account>> {
    let mut stmt = conn.prepare(
        "SELECT id,label,kind,subdomain,host,port,username,password,path,url,notes
         FROM accounts ORDER BY kind, label, id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Account {
            id: r.get(0)?,
            label: r.get(1)?,
            kind: r.get(2)?,
            subdomain: r.get(3)?,
            host: r.get(4)?,
            port: r.get(5)?,
            username: r.get(6)?,
            password: r.get(7)?,
            path: r.get(8)?,
            url: r.get(9)?,
            notes: r.get(10)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn display_label(a: &Account) -> String {
    let generic_ssh_label = a.kind == "SSH"
        && (a.label.is_empty()
            || a.label.eq_ignore_ascii_case("ssh")
            || a.label.eq_ignore_ascii_case("akun ssh")
            || (!a.subdomain.is_empty()
                && (a
                    .label
                    .eq_ignore_ascii_case(&format!("{} (SSH)", a.subdomain))
                    || a.label
                        .eq_ignore_ascii_case(&format!("{} — SSH", a.subdomain))
                    || a.label
                        .eq_ignore_ascii_case(&format!("{} — Akun SSH", a.subdomain)))));

    if generic_ssh_label && !a.username.is_empty() {
        if a.subdomain.is_empty() {
            a.username.clone()
        } else {
            format!("{} — {}", a.subdomain, a.username)
        }
    } else if a.label.is_empty() {
        if a.subdomain.is_empty() {
            a.kind.clone()
        } else {
            format!("{} ({})", a.subdomain, a.kind)
        }
    } else {
        a.label.clone()
    }
}

fn get_account(conn: &Connection, id: i64) -> Result<Account> {
    conn.query_row(
        "SELECT id,label,kind,subdomain,host,port,username,password,path,url,notes
         FROM accounts WHERE id=?1",
        params![id],
        |r| {
            Ok(Account {
                id: r.get(0)?,
                label: r.get(1)?,
                kind: r.get(2)?,
                subdomain: r.get(3)?,
                host: r.get(4)?,
                port: r.get(5)?,
                username: r.get(6)?,
                password: r.get(7)?,
                path: r.get(8)?,
                url: r.get(9)?,
                notes: r.get(10)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow!("account {} not found", id))
}

fn insert_account(conn: &Connection, a: &Account) -> Result<i64> {
    conn.execute(
        "INSERT INTO accounts(label,kind,subdomain,host,port,username,password,path,url,notes)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            a.label,
            a.kind,
            a.subdomain,
            a.host,
            a.port,
            a.username,
            a.password,
            a.path,
            a.url,
            a.notes
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn update_account(conn: &Connection, a: &Account) -> Result<()> {
    conn.execute(
        "UPDATE accounts SET label=?1,kind=?2,subdomain=?3,host=?4,port=?5,username=?6,
         password=?7,path=?8,url=?9,notes=?10 WHERE id=?11",
        params![
            a.label,
            a.kind,
            a.subdomain,
            a.host,
            a.port,
            a.username,
            a.password,
            a.path,
            a.url,
            a.notes,
            a.id
        ],
    )?;
    Ok(())
}

fn delete_account(conn: &Connection, id: i64) -> Result<()> {
    let n = conn.execute("DELETE FROM accounts WHERE id=?1", params![id])?;
    if n == 0 {
        return Err(anyhow!("no account with id {}", id));
    }
    Ok(())
}

fn print_table(accounts: &[Account]) {
    if accounts.is_empty() {
        println!("(no accounts)");
        return;
    }
    println!(
        "{:>4}  {:<10}  {:<28}  {:<32}  {:<18}  {}",
        "ID", "KIND", "LABEL", "HOST/URL", "USER", "SUBDOMAIN"
    );
    println!("{}", "-".repeat(120));
    for a in accounts {
        let target = if !a.host.is_empty() {
            format!("{}:{}", a.host, a.port)
        } else {
            a.url.clone()
        };
        println!(
            "{:>4}  {:<10}  {:<28}  {:<32}  {:<18}  {}",
            a.id,
            trunc(&a.kind, 10),
            trunc(&display_label(a), 28),
            trunc(&target, 32),
            trunc(&a.username, 18),
            a.subdomain
        );
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn show_account(a: &Account) {
    println!("ID        : {}", a.id);
    println!("Label     : {}", a.label);
    println!("Kind      : {}", a.kind);
    println!("Subdomain : {}", a.subdomain);
    println!("Host      : {}", a.host);
    println!("Port      : {}", a.port);
    println!("Username  : {}", a.username);
    println!("Password  : {}", a.password);
    println!("Path      : {}", a.path);
    println!("URL       : {}", a.url);
    if !a.notes.is_empty() {
        println!("Notes     : {}", a.notes);
    }
}

fn prompt_account(initial: &Account) -> Result<Account> {
    let theme = ColorfulTheme::default();
    let kinds = ["SSH", "Cyberpanel", "Panel", "Other"];
    let kind_idx = kinds.iter().position(|k| *k == initial.kind).unwrap_or(0);
    let kind = kinds[Select::with_theme(&theme)
        .with_prompt("Kind")
        .items(&kinds)
        .default(kind_idx)
        .interact()?];

    let label: String = Input::with_theme(&theme)
        .with_prompt("Label")
        .with_initial_text(&initial.label)
        .interact_text()?;
    let subdomain: String = Input::with_theme(&theme)
        .with_prompt("Subdomain")
        .with_initial_text(&initial.subdomain)
        .allow_empty(true)
        .interact_text()?;
    let host: String = Input::with_theme(&theme)
        .with_prompt("Host/IP")
        .with_initial_text(&initial.host)
        .allow_empty(true)
        .interact_text()?;
    let port: i64 = Input::with_theme(&theme)
        .with_prompt("Port")
        .with_initial_text(initial.port.to_string())
        .interact_text()?;
    let username: String = Input::with_theme(&theme)
        .with_prompt("Username")
        .with_initial_text(&initial.username)
        .allow_empty(true)
        .interact_text()?;
    let password: String = if Confirm::with_theme(&theme)
        .with_prompt("Set/change password?")
        .default(initial.password.is_empty())
        .interact()?
    {
        Password::with_theme(&theme)
            .with_prompt("Password")
            .allow_empty_password(true)
            .interact()?
    } else {
        initial.password.clone()
    };
    let path: String = Input::with_theme(&theme)
        .with_prompt("Path")
        .with_initial_text(&initial.path)
        .allow_empty(true)
        .interact_text()?;
    let url: String = Input::with_theme(&theme)
        .with_prompt("URL")
        .with_initial_text(&initial.url)
        .allow_empty(true)
        .interact_text()?;
    let notes: String = Input::with_theme(&theme)
        .with_prompt("Notes")
        .with_initial_text(&initial.notes)
        .allow_empty(true)
        .interact_text()?;

    Ok(Account {
        id: initial.id,
        label,
        kind: kind.to_string(),
        subdomain,
        host,
        port,
        username,
        password,
        path,
        url,
        notes,
    })
}

fn ssh_connect(a: &Account, args: &[String]) -> Result<()> {
    if a.host.is_empty() || a.username.is_empty() {
        return Err(anyhow!("account has no host/username"));
    }
    let target = format!("{}@{}", a.username, a.host);
    let port = a.port.to_string();

    let use_sshpass = !a.password.is_empty() && which("sshpass");
    if !a.password.is_empty() && !use_sshpass {
        eprintln!("(sshpass not installed — you'll be prompted for the password)");
        eprintln!("Password: {}", a.password);
    }

    if args.is_empty() {
        eprintln!("Connecting to {} (port {})...", target, port);
    } else {
        eprintln!("Running on {} (port {}): {}", target, port, args.join(" "));
    }

    let mut common_opts = vec![
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "PubkeyAuthentication=yes".to_string(),
    ];

    if !a.path.is_empty() {
        common_opts.push("-i".to_string());
        common_opts.push(a.path.clone());
    }

    if !a.password.is_empty() {
        common_opts.push("-o".to_string());
        common_opts.push("PreferredAuthentications=publickey,password,keyboard-interactive".to_string());
    }

    let status = if use_sshpass {
        let mut cmd = Command::new("sshpass");
        cmd.args(["-p", &a.password, "ssh", "-p", &port]);
        cmd.args(&common_opts);
        cmd.arg(&target);
        cmd.args(args);
        cmd.status()?
    } else {
        let mut cmd = Command::new("ssh");
        cmd.args(["-p", &port]);
        cmd.args(&common_opts);
        cmd.arg(&target);
        cmd.args(args);
        cmd.status()?
    };

    if !status.success() {
        return Err(anyhow!("ssh exited with status {}", status));
    }
    Ok(())
}

fn scp_connect(a: &Account, args: &[String]) -> Result<()> {
    if a.host.is_empty() || a.username.is_empty() {
        return Err(anyhow!("account has no host/username"));
    }
    let target = format!("{}@{}", a.username, a.host);
    let port = a.port.to_string();

    let use_sshpass = !a.password.is_empty() && which("sshpass");

    eprintln!("SCP with {} (port {})...", target, port);

    let mut common_opts = vec![
        "-P".to_string(),
        port,
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "PubkeyAuthentication=yes".to_string(),
    ];

    if !a.path.is_empty() {
        common_opts.push("-i".to_string());
        common_opts.push(a.path.clone());
    }

    let mut final_args = Vec::new();
    for arg in args {
        if arg.contains(':') {
            // Replace leading ':' with target@host: or if it contains ':' but not at start, 
            // maybe it's already a full path or we just leave it.
            // Simple heuristic: if it starts with ':', it's remote.
            if arg.starts_with(':') {
                final_args.push(format!("{}{}", target, arg));
            } else {
                final_args.push(arg.clone());
            }
        } else {
            final_args.push(arg.clone());
        }
    }

    let status = if use_sshpass {
        let mut cmd = Command::new("sshpass");
        cmd.args(["-p", &a.password, "scp"]);
        cmd.args(&common_opts);
        cmd.args(&final_args);
        cmd.status()?
    } else {
        let mut cmd = Command::new("scp");
        cmd.args(&common_opts);
        cmd.args(&final_args);
        cmd.status()?
    };

    if !status.success() {
        return Err(anyhow!("scp exited with status {}", status));
    }
    Ok(())
}

fn which(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", cmd))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn pick_ssh_account(conn: &Connection) -> Result<Account> {
    let all = list_accounts(conn)?;
    let ssh: Vec<_> = all
        .into_iter()
        .filter(|a| !a.host.is_empty() && !a.username.is_empty())
        .collect();
    if ssh.is_empty() {
        return Err(anyhow!("no SSH-capable accounts"));
    }
    let labels: Vec<String> = ssh
        .iter()
        .map(|a| {
            format!(
                "[{}] {} — {}@{}:{}",
                a.id,
                display_label(a),
                a.username,
                a.host,
                a.port
            )
        })
        .collect();
    let idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Pick account to SSH into")
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(ssh[idx].clone())
}

fn import_file(conn: &Connection, path: &PathBuf) -> Result<usize> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut count = 0;
    let mut current_subdomain = String::new();
    let mut blocks: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for line in content.lines() {
        let l = line.trim();
        if l.is_empty() {
            if !cur.is_empty() {
                blocks.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(l.to_string());
        }
    }
    if !cur.is_empty() {
        blocks.push(cur);
    }

    for block in blocks {
        // Parse "key : value" lines. Detect header (first non-kv line) and "Sub Domain" lines.
        let mut header = String::new();
        let mut sub_in_block = String::new();
        let mut inline_label = String::new();
        let mut explicit_kind = None;
        let mut kv: Vec<(String, String)> = Vec::new();
        for line in &block {
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim().to_lowercase();
                let val = v.trim().to_string();
                if key == "sub domain" || key == "subdomain" {
                    sub_in_block = val;
                } else if let Some(kind) = import_section_kind(&key) {
                    explicit_kind = Some(kind);
                    if !val.is_empty() {
                        inline_label = val;
                    }
                    if header.is_empty() {
                        header = kind.to_string();
                    }
                } else {
                    kv.push((key, val));
                }
            } else {
                if explicit_kind.is_none() {
                    explicit_kind = import_section_kind(line);
                }
                if header.is_empty() {
                    header = line.clone();
                }
            }
        }
        if !sub_in_block.is_empty() {
            current_subdomain = sub_in_block.clone();
        }
        if kv.is_empty() {
            continue;
        }

        let kind = if let Some(kind) = explicit_kind {
            kind
        } else if header.to_lowercase().contains("cyberpanel")
            || kv.iter().any(|(k, _)| k == "url") && !kv.iter().any(|(k, _)| k == "ip")
        {
            "Cyberpanel"
        } else if header.to_lowercase().contains("panel") {
            "Panel"
        } else {
            "SSH"
        };

        let mut a = Account::empty();
        a.kind = kind.into();
        a.subdomain = if !sub_in_block.is_empty() {
            sub_in_block
        } else {
            current_subdomain.clone()
        };

        for (k, v) in &kv {
            match k.as_str() {
                "ip" => a.host = v.clone(),
                "user" | "username" => a.username = v.clone(),
                "password" => a.password = v.clone(),
                "path" => a.path = v.clone(),
                "url" | "link" => a.url = v.clone(),
                "port" => {
                    if let Ok(p) = v.parse() {
                        a.port = p;
                    }
                }
                _ => {}
            }
        }

        let label_source = if !inline_label.is_empty() {
            inline_label.as_str()
        } else {
            header.as_str()
        };
        let generic_label = label_source.eq_ignore_ascii_case(kind)
            || (kind == "SSH" && label_source.eq_ignore_ascii_case("akun ssh"));
        a.label = if !label_source.is_empty() && !generic_label {
            if a.subdomain.is_empty() {
                label_source.to_string()
            } else {
                format!("{} — {}", a.subdomain, label_source)
            }
        } else if kind == "SSH" && !a.username.is_empty() {
            if a.subdomain.is_empty() {
                a.username.clone()
            } else {
                format!("{} — {}", a.subdomain, a.username)
            }
        } else if !a.subdomain.is_empty() {
            format!("{} ({})", a.subdomain, kind)
        } else {
            kind.to_string()
        };

        if a.host.is_empty() && a.url.is_empty() && a.username.is_empty() {
            continue;
        }
        insert_account(conn, &a)?;
        count += 1;
    }
    Ok(count)
}

fn run_menu(conn: &Connection) -> Result<()> {
    let theme = ColorfulTheme::default();
    loop {
        let items = [
            "List accounts",
            "Show account",
            "SSH into account",
            "Add account",
            "Edit account",
            "Delete account",
            "Import from file",
            "Quit",
        ];
        let choice = Select::with_theme(&theme)
            .with_prompt("Account manager")
            .items(&items)
            .default(0)
            .interact()?;
        match choice {
            0 => print_table(&list_accounts(conn)?),
            1 => {
                let id: i64 = Input::with_theme(&theme)
                    .with_prompt("ID")
                    .interact_text()?;
                show_account(&get_account(conn, id)?);
            }
            2 => {
                let a = pick_ssh_account(conn)?;
                if let Err(e) = ssh_connect(&a, &[]) {
                    eprintln!("ssh failed: {}", e);
                }
            }
            3 => {
                let a = prompt_account(&Account::empty())?;
                let id = insert_account(conn, &a)?;
                println!("Created #{}", id);
            }
            4 => {
                let id: i64 = Input::with_theme(&theme)
                    .with_prompt("ID to edit")
                    .interact_text()?;
                let existing = get_account(conn, id)?;
                let updated = prompt_account(&existing)?;
                update_account(conn, &updated)?;
                println!("Updated #{}", id);
            }
            5 => {
                let id: i64 = Input::with_theme(&theme)
                    .with_prompt("ID to delete")
                    .interact_text()?;
                if Confirm::with_theme(&theme)
                    .with_prompt(format!("Really delete #{}?", id))
                    .default(false)
                    .interact()?
                {
                    delete_account(conn, id)?;
                    println!("Deleted.");
                }
            }
            6 => {
                let p: String = Input::with_theme(&theme)
                    .with_prompt("Path to file")
                    .interact_text()?;
                let n = import_file(conn, &PathBuf::from(p))?;
                println!("Imported {} accounts", n);
            }
            _ => return Ok(()),
        }
        println!();
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let path = db_path(&cli.db)?;
    let conn = open_db(&path)?;

    match cli.cmd.unwrap_or(Cmd::Menu) {
        Cmd::List => print_table(&list_accounts(&conn)?),
        Cmd::Show { id } => show_account(&get_account(&conn, id)?),
        Cmd::Add => {
            let a = prompt_account(&Account::empty())?;
            let id = insert_account(&conn, &a)?;
            println!("Created #{}", id);
        }
        Cmd::Edit { id } => {
            let existing = get_account(&conn, id)?;
            let updated = prompt_account(&existing)?;
            update_account(&conn, &updated)?;
            println!("Updated #{}", id);
        }
        Cmd::Delete { id } => {
            if Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Really delete #{}?", id))
                .default(false)
                .interact()?
            {
                delete_account(&conn, id)?;
                println!("Deleted.");
            }
        }
        Cmd::Ssh { id, args } => {
            let a = match id {
                Some(i) => get_account(&conn, i)?,
                None => pick_ssh_account(&conn)?,
            };
            ssh_connect(&a, &args)?;
        }
        Cmd::Scp { id, args } => {
            let a = get_account(&conn, id)?;
            scp_connect(&a, &args)?;
        }
        Cmd::Import { file } => {
            let n = import_file(&conn, &file)?;
            println!("Imported {} accounts", n);
        }
        Cmd::Menu => run_menu(&conn)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_temp_import_file(contents: &str) -> Result<PathBuf> {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("accounts-import-{suffix}.txt"));
        fs::write(&path, contents)?;
        Ok(path)
    }

    #[test]
    fn import_uses_inline_section_label_for_ssh_accounts() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        init_db(&conn)?;
        let path = write_temp_import_file(
            "Sub Domain : app-staging.example.com\n\nSSH : app_dev\nIP : 139.180.186.215\nuser : appde4404\npassword : secret\n",
        )?;

        let imported = import_file(&conn, &path)?;
        fs::remove_file(&path)?;

        assert_eq!(imported, 1);

        let accounts = list_accounts(&conn)?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].kind, "SSH");
        assert_eq!(accounts[0].label, "app-staging.example.com — app_dev");

        Ok(())
    }

    #[test]
    fn import_uses_ssh_username_when_header_is_generic() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        init_db(&conn)?;
        let path = write_temp_import_file(
            "Sub Domain : api-dev.example.com\n\nAkun SSH\nIP : 139.180.186.215\nuser : apide3047\npassword : secret\n",
        )?;

        let imported = import_file(&conn, &path)?;
        fs::remove_file(&path)?;

        assert_eq!(imported, 1);

        let accounts = list_accounts(&conn)?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].kind, "SSH");
        assert_eq!(accounts[0].label, "api-dev.example.com — apide3047");

        Ok(())
    }

    #[test]
    fn display_label_uses_username_for_legacy_generic_ssh_labels() {
        let mut account = Account::empty();
        account.kind = "SSH".into();
        account.subdomain = "app-staging.example.com".into();
        account.username = "deploy".into();
        account.label = "app-staging.example.com (SSH)".into();

        assert_eq!(display_label(&account), "app-staging.example.com — deploy");
    }
}
