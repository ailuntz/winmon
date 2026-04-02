use crate::metrics::Metrics;
use crate::sources::DeviceInfo;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, RwLock};
use std::thread;

pub type SharedMetrics = Arc<RwLock<Option<Metrics>>>;

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}

fn to_json(metrics: &Metrics, device: &DeviceInfo) -> String {
    let mut doc = serde_json::to_value(metrics).unwrap_or_default();
    doc["device"] = serde_json::to_value(device).unwrap_or_default();
    doc["timestamp"] = serde_json::to_value(chrono::Utc::now().to_rfc3339()).unwrap_or_default();
    serde_json::to_string(&doc).unwrap_or_default()
}

fn to_prometheus(metrics: &Metrics, device: &DeviceInfo) -> String {
    let labels = format!(
        r#"machine="{}",cpu="{}",gpu="{}""#,
        escape_label(&device.machine_name),
        escape_label(&device.cpu_name),
        escape_label(&device.gpu_name)
    );

    macro_rules! gauge {
        ($out:expr, $name:literal, $help:literal, $value:expr) => {
            $out.push_str(&format!(
                "# HELP {} {}\n# TYPE {} gauge\n{}{{{}}} {}\n\n",
                $name, $help, $name, $name, labels, $value
            ));
        };
    }

    macro_rules! gauge_opt {
        ($out:expr, $name:literal, $help:literal, $value:expr) => {
            if let Some(value) = $value {
                gauge!($out, $name, $help, value);
            }
        };
    }

    let mut out = String::new();
    gauge_opt!(
        out,
        "winmon_cpu_temp_celsius",
        "CPU temperature in Celsius",
        metrics.temp.cpu_temp
    );
    gauge_opt!(
        out,
        "winmon_gpu_temp_celsius",
        "GPU temperature in Celsius",
        metrics.temp.gpu_temp
    );
    gauge!(
        out,
        "winmon_memory_ram_total_bytes",
        "Total RAM in bytes",
        metrics.memory.ram_total
    );
    gauge!(
        out,
        "winmon_memory_ram_used_bytes",
        "Used RAM in bytes",
        metrics.memory.ram_usage
    );
    gauge!(
        out,
        "winmon_memory_swap_total_bytes",
        "Total swap in bytes",
        metrics.memory.swap_total
    );
    gauge!(
        out,
        "winmon_memory_swap_used_bytes",
        "Used swap in bytes",
        metrics.memory.swap_usage
    );
    gauge!(
        out,
        "winmon_cpu_usage_ratio",
        "Combined CPU utilization (0-1), weighted by core count when available",
        metrics.cpu_usage_pct
    );
    gauge!(
        out,
        "winmon_ecpu_freq_mhz",
        "Efficiency CPU average frequency in MHz",
        metrics.e_cpu_usage.0
    );
    gauge!(
        out,
        "winmon_ecpu_usage_ratio",
        "Efficiency CPU utilization (0-1)",
        metrics.e_cpu_usage.1
    );
    gauge!(
        out,
        "winmon_pcpu_freq_mhz",
        "Performance CPU average frequency in MHz",
        metrics.p_cpu_usage.0
    );
    gauge!(
        out,
        "winmon_pcpu_usage_ratio",
        "Performance CPU utilization (0-1)",
        metrics.p_cpu_usage.1
    );
    gauge!(
        out,
        "winmon_gpu_freq_mhz",
        "GPU frequency in MHz",
        metrics.gpu_usage.0
    );
    gauge!(
        out,
        "winmon_gpu_usage_ratio",
        "GPU utilization (0-1)",
        metrics.gpu_usage.1
    );
    gauge_opt!(
        out,
        "winmon_cpu_power_watts",
        "CPU power consumption in Watts",
        metrics.power.cpu_power
    );
    gauge_opt!(
        out,
        "winmon_gpu_power_watts",
        "GPU power consumption in Watts",
        metrics.power.gpu_power
    );
    gauge_opt!(
        out,
        "winmon_all_power_watts",
        "Tracked CPU plus GPU power in Watts",
        metrics.power.tracked_power
    );
    gauge_opt!(
        out,
        "winmon_sys_power_watts",
        "System power consumption in Watts",
        metrics.power.sys_power
    );
    gauge_opt!(
        out,
        "winmon_tracked_power_watts",
        "Tracked CPU plus GPU power in Watts",
        metrics.power.tracked_power
    );
    out
}

fn read_path(stream: &mut TcpStream) -> Option<String> {
    let mut buf = [0u8; 2048];
    let n = stream.read(&mut buf).ok()?;
    let text = std::str::from_utf8(&buf[..n]).ok()?;
    let path = text.lines().next()?.split_whitespace().nth(1)?;
    Some(path.split('?').next().unwrap_or(path).to_string())
}

fn write_response(stream: &mut TcpStream, status: u16, content_type: &str, body: String) {
    let status_text = match status {
        200 => "OK",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "OK",
    };

    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    );
}

fn handle_conn(mut stream: TcpStream, shared: SharedMetrics, device: Arc<DeviceInfo>) {
    let Some(path) = read_path(&mut stream) else {
        return;
    };

    let lock = shared.read().unwrap();
    let Some(metrics) = lock.as_ref() else {
        drop(lock);
        write_response(
            &mut stream,
            503,
            "application/json",
            r#"{"error":"no data yet"}"#.to_string(),
        );
        return;
    };

    match path.as_str() {
        "/json" => {
            let body = to_json(metrics, &device);
            drop(lock);
            write_response(&mut stream, 200, "application/json", body);
        }
        "/metrics" => {
            let body = to_prometheus(metrics, &device);
            drop(lock);
            write_response(
                &mut stream,
                200,
                "text/plain; version=0.0.4; charset=utf-8",
                body,
            );
        }
        _ => {
            drop(lock);
            write_response(
                &mut stream,
                404,
                "application/json",
                r#"{"error":"not found"}"#.to_string(),
            );
        }
    }
}

pub fn run(
    port: u16,
    shared: SharedMetrics,
    device: Arc<DeviceInfo>,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(format!("0.0.0.0:{port}"))?;
    eprintln!("winmon serving on http://localhost:{port}");
    eprintln!("  GET /json    -> JSON metrics");
    eprintln!("  GET /metrics -> Prometheus format");

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let shared = Arc::clone(&shared);
        let device = Arc::clone(&device);
        thread::spawn(move || handle_conn(stream, shared, device));
    }

    Ok(())
}
