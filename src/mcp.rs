use std::io::{self, BufRead, Write};

use serde::Serialize;

use crate::monitor;

const PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorObj>,
}

#[derive(Serialize)]
struct ErrorObj {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

fn ok(id: Option<u64>, result: serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(result),
        error: None,
    }
}

fn err(id: Option<u64>, code: i32, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(ErrorObj {
            code,
            message: message.into(),
            data: None,
        }),
    }
}

pub fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let resp = err(None, -32700, format!("Parse error: {}", e));
                let json = serde_json::to_string(&resp)?;
                writeln!(stdout, "{}", json)?;
                stdout.flush()?;
                continue;
            }
        };

        let method = req["method"].as_str().unwrap_or("");
        let id = req["id"].as_u64();

        if method == "notifications/initialized" {
            continue;
        }

        let response = match method {
            "initialize" => handle_initialize(&req, id),
            "tools/list" => handle_tools_list(id),
            "tools/call" => handle_tools_call(&req["params"], id),
            _ => err(id, -32601, "Method not found"),
        };

        let json = serde_json::to_string(&response)?;
        writeln!(stdout, "{}", json)?;
        stdout.flush()?;
    }

    Ok(())
}

fn handle_initialize(_req: &serde_json::Value, id: Option<u64>) -> JsonRpcResponse {
    ok(id, serde_json::json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "ctrl-vitals",
            "version": env!("CARGO_PKG_VERSION")
        }
    }))
}

fn handle_tools_list(id: Option<u64>) -> JsonRpcResponse {
    ok(id, serde_json::json!({
        "tools": [
            {
                "name": "get_cpu_usage",
                "description": "Get CPU usage per-core, aggregate percent, and temperature",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "get_gpu_stats",
                "description": "Get GPU core utilization, VRAM used/total, and temperature (NVIDIA only)",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "get_ram_stats",
                "description": "Get RAM used/total in GB and percentage",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "get_disk_stats",
                "description": "Get disk usage for a mount point",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "mount": {
                            "type": "string",
                            "description": "Mount point (default: /)",
                            "default": "/"
                        }
                    }
                }
            },
            {
                "name": "get_network_speed",
                "description": "Get real-time network download/upload speed",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "get_top_processes",
                "description": "Get top N processes by CPU usage",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "count": {
                            "type": "integer",
                            "description": "Number of processes (default: 5)",
                            "default": 5
                        }
                    }
                }
            }
        ]
    }))
}

fn handle_tools_call(params: &serde_json::Value, id: Option<u64>) -> JsonRpcResponse {
    let name = params["name"].as_str().unwrap_or("");
    let args = &params["arguments"];

    let result = match name {
        "get_cpu_usage" => get_cpu_usage(),
        "get_gpu_stats" => get_gpu_stats(),
        "get_ram_stats" => get_ram_stats(),
        "get_disk_stats" => get_disk_stats(args),
        "get_network_speed" => get_network_speed(),
        "get_top_processes" => get_top_processes(args),
        _ => return err(id, -32602, format!("Unknown tool: {}", name)),
    };

    match result {
        Ok(text) => ok(id, serde_json::json!({
            "content": [{ "type": "text", "text": text }]
        })),
        Err(e) => err(id, -32000, e),
    }
}

fn get_cpu_usage() -> Result<String, String> {
    let snap1 = monitor::read_all_cpu_snapshots().ok_or("Failed to read CPU stats")?;
    if snap1.len() < 2 {
        return Err("No CPU cores detected".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    let snap2 = monitor::read_all_cpu_snapshots().ok_or("Failed to read CPU stats")?;
    if snap1.len() != snap2.len() {
        return Err("CPU count changed between samples".into());
    }

    let core_count = snap1.len() - 1;
    let agg = monitor::compute_cpu_usage(&snap1[0], &snap2[0]);
    let sensor = monitor::detect_temp_sensor();
    let temp = sensor.as_ref().and_then(monitor::read_temp);

    let mut out = String::new();
    out.push_str(&format!("CPU: {} cores\n", core_count));
    out.push_str(&format!("Aggregate: {:.1}%\n", agg.clamp(0.0, 100.0)));
    if let Some(t) = temp {
        out.push_str(&format!("Temperature: {:.0}°C\n", t));
    }
    out.push_str("\nPer-core:\n");
    for i in 0..core_count {
        let usage =
            monitor::compute_cpu_usage(&snap1[i + 1], &snap2[i + 1]).clamp(0.0, 100.0);
        out.push_str(&format!("  Core {}: {:.1}%\n", i, usage));
    }
    Ok(out)
}

fn get_gpu_stats() -> Result<String, String> {
    let stats = monitor::read_gpu_stats().ok_or("No NVIDIA GPU found or nvidia-smi not available")?;
    Ok(format!(
        "GPU Core: {:.0}%\nTemperature: {:.0}°C\nVRAM: {:.1}/{:.0} MiB ({:.0}%)",
        stats.usage,
        stats.temp,
        stats.mem_used_mib,
        stats.mem_total_mib,
        stats.mem_percent()
    ))
}

fn get_ram_stats() -> Result<String, String> {
    let stats = monitor::read_ram_stats().ok_or("Failed to read RAM stats")?;
    Ok(format!(
        "RAM: {:.1}/{:.0} GB ({:.0}% used)",
        stats.used_gb(),
        stats.total_gb(),
        stats.used_percent()
    ))
}

fn get_disk_stats(args: &serde_json::Value) -> Result<String, String> {
    let mount = args["mount"].as_str().unwrap_or("/");
    let stats = monitor::read_disk_stats(mount)
        .ok_or_else(|| format!("Failed to read disk stats for {}", mount))?;
    Ok(format!(
        "Disk {}: {:.1}/{:.0} GB ({:.0}% used)",
        mount,
        stats.used_gb(),
        stats.total_gb(),
        stats.used_percent()
    ))
}

fn get_network_speed() -> Result<String, String> {
    let iface = monitor::detect_active_interface().ok_or("No network interface found")?;
    let mut monitor = monitor::NetMonitor::new(iface.clone());
    let stats = monitor.poll().ok_or("Need one more sample for network delta")?;
    let rx = if stats.rx_bytes_per_sec >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", stats.rx_bytes_per_sec / 1024.0 / 1024.0)
    } else if stats.rx_bytes_per_sec >= 1024.0 {
        format!("{:.0} KB/s", stats.rx_bytes_per_sec / 1024.0)
    } else {
        format!("{:.0} B/s", stats.rx_bytes_per_sec)
    };
    let tx = if stats.tx_bytes_per_sec >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", stats.tx_bytes_per_sec / 1024.0 / 1024.0)
    } else if stats.tx_bytes_per_sec >= 1024.0 {
        format!("{:.0} KB/s", stats.tx_bytes_per_sec / 1024.0)
    } else {
        format!("{:.0} B/s", stats.tx_bytes_per_sec)
    };
    Ok(format!("Interface: {}\nDownload: {}\nUpload: {}", iface, rx, tx))
}

fn get_top_processes(args: &serde_json::Value) -> Result<String, String> {
    let count = args["count"].as_u64().unwrap_or(5) as usize;
    let procs = monitor::read_top_processes(count);
    if procs.is_empty() {
        return Err("No process data available".into());
    }
    let mut out = String::from("Top processes by CPU:\n");
    for (i, p) in procs.iter().enumerate() {
        out.push_str(&format!(
            "  {}. {} — CPU: {:.1}%, MEM: {:.1}%\n",
            i + 1,
            p.name,
            p.cpu_percent,
            p.mem_percent
        ));
    }
    Ok(out)
}
