use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct CpuSnapshot {
    pub total: u64,
    pub idle: u64,
}

fn parse_cpu_line(line: &str) -> Option<CpuSnapshot> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }
    let vals: Vec<u64> = parts[1..].iter().filter_map(|s| s.parse().ok()).collect();
    if vals.len() < 4 {
        return None;
    }
    let idle = vals[3] + vals.get(4).copied().unwrap_or(0);
    let total: u64 = vals.iter().sum();
    Some(CpuSnapshot { total, idle })
}

pub fn read_all_cpu_snapshots() -> Option<Vec<CpuSnapshot>> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let mut snapshots = Vec::new();
    for line in stat.lines() {
        if let Some(rest) = line.strip_prefix("cpu") {
            if !rest.is_empty() {
                if let Some(s) = parse_cpu_line(line) {
                    snapshots.push(s);
                }
            }
        }
    }
    if snapshots.is_empty() { None } else { Some(snapshots) }
}

pub fn compute_cpu_usage(prev: &CpuSnapshot, curr: &CpuSnapshot) -> f64 {
    let total_delta = curr.total.saturating_sub(prev.total);
    let idle_delta = curr.idle.saturating_sub(prev.idle);
    if total_delta == 0 {
        return 0.0;
    }
    100.0 * (total_delta - idle_delta) as f64 / total_delta as f64
}

pub struct UsageHistory {
    buffer: Vec<f64>,
    max_len: usize,
}

impl UsageHistory {
    pub fn new(max_len: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(max_len),
            max_len,
        }
    }

    pub fn push(&mut self, value: f64) {
        if self.buffer.len() >= self.max_len {
            self.buffer.remove(0);
        }
        self.buffer.push(value);
    }

    pub fn as_slice(&self) -> &[f64] {
        &self.buffer
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn max(&self) -> f64 {
        self.buffer.iter().cloned().fold(0.0f64, f64::max)
    }

    pub fn min(&self) -> f64 {
        self.buffer.iter().cloned().fold(f64::INFINITY, f64::min)
    }
}

#[derive(Clone)]
pub struct TempSensor {
    path: PathBuf,
}

pub fn detect_temp_sensor() -> Option<TempSensor> {
    let hwmon = PathBuf::from("/sys/class/hwmon");
    let dir = fs::read_dir(&hwmon).ok()?;
    for entry in dir.flatten() {
        let name_file = entry.path().join("name");
        if let Ok(name) = fs::read_to_string(name_file) {
            if name.trim() == "k10temp" || name.trim() == "coretemp" {
                let temp_path = entry.path().join("temp1_input");
                if temp_path.exists() {
                    return Some(TempSensor { path: temp_path });
                }
            }
        }
    }
    for entry in fs::read_dir(&hwmon).ok()?.flatten() {
        if let Ok(mut sub) = fs::read_dir(entry.path()) {
            while let Some(Ok(f)) = sub.next() {
                let fname = f.file_name().to_string_lossy().to_string();
                if fname.starts_with("temp") && fname.ends_with("_input") {
                    return Some(TempSensor { path: f.path() });
                }
            }
        }
    }
    None
}

pub fn read_temp(sensor: &TempSensor) -> Option<f64> {
    let raw = fs::read_to_string(&sensor.path).ok()?;
    let millideg: f64 = raw.trim().parse().ok()?;
    Some(millideg / 1000.0)
}

pub struct GpuStats {
    pub usage: f64,
    pub temp: f64,
    pub mem_used_mib: f64,
    pub mem_total_mib: f64,
}

impl GpuStats {
    pub fn mem_percent(&self) -> f64 {
        if self.mem_total_mib == 0.0 {
            0.0
        } else {
            self.mem_used_mib / self.mem_total_mib * 100.0
        }
    }
}

pub fn read_gpu_stats() -> Option<GpuStats> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,temperature.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let parts: Vec<&str> = stdout.trim().split(',').collect();
    if parts.len() != 4 {
        return None;
    }
    let usage: f64 = parts[0].trim().parse().ok()?;
    let temp: f64 = parts[1].trim().parse().ok()?;
    let mem_used_mib: f64 = parts[2].trim().parse().ok()?;
    let mem_total_mib: f64 = parts[3].trim().parse().ok()?;
    Some(GpuStats {
        usage,
        temp,
        mem_used_mib,
        mem_total_mib,
    })
}

pub struct RamStats {
    pub total_kb: f64,
    pub available_kb: f64,
}

impl RamStats {
    pub fn used_percent(&self) -> f64 {
        if self.total_kb == 0.0 {
            0.0
        } else {
            (self.total_kb - self.available_kb) / self.total_kb * 100.0
        }
    }
    pub fn used_gb(&self) -> f64 {
        (self.total_kb - self.available_kb) / 1024.0 / 1024.0
    }
    pub fn total_gb(&self) -> f64 {
        self.total_kb / 1024.0 / 1024.0
    }
}

pub fn read_ram_stats() -> Option<RamStats> {
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
    let mut total_kb = 0.0;
    let mut available_kb = 0.0;
    for line in meminfo.lines() {
        if let Some(val) = line.strip_prefix("MemTotal:") {
            total_kb = val.split_whitespace().next()?.parse().ok()?;
        }
        if let Some(val) = line.strip_prefix("MemAvailable:") {
            available_kb = val.split_whitespace().next()?.parse().ok()?;
        }
    }
    if total_kb == 0.0 {
        return None;
    }
    Some(RamStats {
        total_kb,
        available_kb,
    })
}

pub struct DiskStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
}

impl DiskStats {
    pub fn used_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.used_bytes as f64 / self.total_bytes as f64 * 100.0
        }
    }
    pub fn used_gb(&self) -> f64 {
        self.used_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    }
    pub fn total_gb(&self) -> f64 {
        self.total_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    }
}

pub fn read_disk_stats(mount: &str) -> Option<DiskStats> {
    let output = std::process::Command::new("df")
        .args(["-B1", "--output=size,used", mount])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut lines = stdout.lines();
    lines.next()?;
    let line = lines.next()?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let total_bytes: u64 = parts[0].parse().ok()?;
    let used_bytes: u64 = parts[1].parse().ok()?;
    Some(DiskStats {
        total_bytes,
        used_bytes,
    })
}

pub struct ProcessInfo {
    pub cpu_percent: f64,
    pub mem_percent: f64,
    pub name: String,
}

pub fn read_top_processes(n: usize) -> Vec<ProcessInfo> {
    let output = std::process::Command::new("ps")
        .args([
            "-eo",
            "pid,pcpu,pmem,comm",
            "--sort=-pcpu",
            "--no-headers",
        ])
        .output()
        .ok();
    let output = match output {
        Some(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let stdout = String::from_utf8(output.stdout).unwrap_or_default();
    stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let _pid = parts.next()?;
            let cpu: f64 = parts.next()?.parse().ok()?;
            let mem: f64 = parts.next()?.parse().ok()?;
            let name = parts.next().unwrap_or("?").to_string();
            Some(ProcessInfo {
                cpu_percent: cpu,
                mem_percent: mem,
                name,
            })
        })
        .take(n)
        .collect()
}

pub struct NetSpeed {
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
}

pub struct NetMonitor {
    prev_rx: u64,
    prev_tx: u64,
    prev_time: Option<std::time::Instant>,
    pub interface: String,
}

impl NetMonitor {
    pub fn new(interface: String) -> Self {
        Self {
            prev_rx: 0,
            prev_tx: 0,
            prev_time: None,
            interface,
        }
    }

    pub fn poll(&mut self) -> Option<NetSpeed> {
        let dev = fs::read_to_string("/proc/net/dev").ok()?;
        let prefix = format!("{}:", self.interface);
        for line in dev.lines() {
            if let Some(stats) = line.trim().strip_prefix(&prefix) {
                let parts: Vec<&str> = stats.split_whitespace().collect();
                if parts.len() < 10 {
                    return None;
                }
                let rx: u64 = parts[0].parse().ok()?;
                let tx: u64 = parts[8].parse().ok()?;
                let now = std::time::Instant::now();
                match self.prev_time {
                    Some(t) => {
                        let elapsed = now.duration_since(t).as_secs_f64();
                        if elapsed < 0.01 {
                            return None;
                        }
                        let result = NetSpeed {
                            rx_bytes_per_sec: (rx - self.prev_rx) as f64 / elapsed,
                            tx_bytes_per_sec: (tx - self.prev_tx) as f64 / elapsed,
                        };
                        self.prev_rx = rx;
                        self.prev_tx = tx;
                        self.prev_time = Some(now);
                        return Some(result);
                    }
                    None => {
                        self.prev_rx = rx;
                        self.prev_tx = tx;
                        self.prev_time = Some(now);
                        return None;
                    }
                }
            }
        }
        None
    }

}

pub fn detect_active_interface() -> Option<String> {
    let dev = fs::read_to_string("/proc/net/dev").ok()?;
    for line in dev.lines() {
        let trimmed = line.trim();
        if trimmed.contains(':') {
            if let Some(name) = trimmed.split(':').next() {
                let name = name.trim();
                if !name.is_empty() && name != "lo" {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}
