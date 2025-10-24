use anyhow::Result;
use chrono::{DateTime, Local};
use ssh2::Session;
use std::{
    io::Read,
    net::TcpStream,
};

#[derive(Clone, Debug)]
pub struct UserStats {
    pub username: String,
    pub cpu_percent: f64,
    pub ram_mb: f64,
    pub last_updated: DateTime<Local>,
}

pub fn get_user_stats(
    host: &str,
    user: &str,
    password: Option<&str>,
    ssh_key_path: Option<&str>,
) -> Result<(Vec<UserStats>, f64)> {
    let tcp = TcpStream::connect(format!("{}:22", host))?;
    let mut sess = Session::new()?;
    sess.set_tcp_stream(tcp);
    sess.handshake()?;

    // Authenticate using either password or SSH key
    if let Some(key_path) = ssh_key_path {
        sess.userauth_pubkey_file(user, None, std::path::Path::new(key_path), None)?;
    } else if let Some(pwd) = password {
        sess.userauth_password(user, pwd)?;
    } else {
        return Err(anyhow::anyhow!("No authentication method provided"));
    }

    // Get per-user CPU and memory usage
    let mut channel = sess.channel_session()?;
    
    // This command gets CPU and memory usage per user
    // Uses ps to get processes with user, CPU%, and memory
    let cmd = r#"ps aux | awk 'NR>1 {cpu[$1]+=$3; mem[$1]+=$4; rss[$1]+=$6} END {for(user in cpu) printf "%s %.2f %.2f\n", user, cpu[user], rss[user]/1024}'"#;
    
    channel.exec(cmd)?;
    let mut output = String::new();
    channel.read_to_string(&mut output)?;
    channel.wait_close()?;

    let now = Local::now();
    let mut users = Vec::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            users.push(UserStats {
                username: parts[0].to_string(),
                cpu_percent: parts[1].parse().unwrap_or(0.0),
                ram_mb: parts[2].parse().unwrap_or(0.0),
                last_updated: now,
            });
        }
    }

    // Sort by CPU usage (descending)
    users.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap());

    // Get total RAM
    let mut channel = sess.channel_session()?;
    channel.exec("free -m | awk 'NR==2 {print $2}'")?;
    let mut ram_output = String::new();
    channel.read_to_string(&mut ram_output)?;
    channel.wait_close()?;
    
    let total_ram_mb: f64 = ram_output.trim().parse().unwrap_or(0.0);

    Ok((users, total_ram_mb))
}
