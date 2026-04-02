use crate::metrics::{MemMetrics, Metrics, PowerMetrics, TempMetrics, zero_div};
#[cfg(windows)]
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::error::Error;
#[cfg(windows)]
use std::mem::size_of;
#[cfg(windows)]
use std::process::Command;
use std::sync::{Arc, RwLock};
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
const EMBEDDED_OHM_DLL: &[u8] = include_bytes!("../third_party/ohm/OpenHardwareMonitorLib.dll");

pub type WithError<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub machine_name: String,
    pub os_version: String,
    pub cpu_name: String,
    pub cpu_vendor: String,
    pub cpu_cores: u32,
    pub cpu_threads: u32,
    pub cpu_base_freq_mhz: u32,
    pub cpu_p_cores: Option<u32>,
    pub cpu_e_cores: Option<u32>,
    pub cpu_p_base_freq_mhz: Option<u32>,
    pub cpu_e_base_freq_mhz: Option<u32>,
    pub cpu_p_max_freq_mhz: Option<u32>,
    pub cpu_e_max_freq_mhz: Option<u32>,
    pub gpu_name: String,
    pub gpu_vendor: String,
    pub gpu_backend: String,
}

#[derive(Debug, Default, Deserialize)]
struct Snapshot {
    cpu_usage_percent: f32,
    cpu_freq_mhz: u32,
    cpu_base_freq_mhz: u32,
    e_cpu_usage_percent: Option<f32>,
    e_cpu_freq_mhz: Option<u32>,
    p_cpu_usage_percent: Option<f32>,
    p_cpu_freq_mhz: Option<u32>,
    ram_total_bytes: u64,
    ram_used_bytes: u64,
    swap_total_bytes: u64,
    swap_used_bytes: u64,
    gpu_usage_percent: Option<f32>,
    gpu_freq_mhz: Option<u32>,
    cpu_temp_c: Option<f32>,
    gpu_temp_c: Option<f32>,
    cpu_power_w: Option<f32>,
    gpu_power_w: Option<f32>,
    sys_power_w: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct StaticMemoryInfo {
    ram_total_bytes: u64,
    swap_total_bytes: u64,
}

#[derive(Debug, Default, Deserialize)]
struct FastSnapshot {
    cpu_usage_percent: f32,
    cpu_freq_mhz: u32,
    e_cpu_usage_percent: Option<f32>,
    e_cpu_freq_mhz: Option<u32>,
    p_cpu_usage_percent: Option<f32>,
    p_cpu_freq_mhz: Option<u32>,
    gpu_usage_percent: Option<f32>,
    gpu_freq_mhz: Option<u32>,
    cpu_temp_c: Option<f32>,
    gpu_temp_c: Option<f32>,
    gpu_power_w: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct SlowSnapshot {
    swap_used_bytes: Option<u64>,
    cpu_power_w: Option<f32>,
    sys_power_w: Option<f32>,
}

#[derive(Debug, Default, Clone, Copy)]
struct SlowCache {
    swap_used_bytes: u64,
    cpu_power_w: Option<f32>,
    sys_power_w: Option<f32>,
}

impl Snapshot {
    fn into_metrics(self, device: &DeviceInfo) -> Metrics {
        let cpu_temp = normalize_value(self.cpu_temp_c);
        let gpu_temp = normalize_value(self.gpu_temp_c);
        let cpu_power = normalize_value(self.cpu_power_w);
        let gpu_power = normalize_value(self.gpu_power_w);
        let sys_power = normalize_value(self.sys_power_w);
        let cpu_usage_pct = normalize_ratio(self.cpu_usage_percent);
        let e_cpu_usage_pct = normalize_ratio(self.e_cpu_usage_percent.unwrap_or_default());
        let p_cpu_usage_pct = normalize_ratio(self.p_cpu_usage_percent.unwrap_or_default());

        let combined_cpu_usage_pct = match (device.cpu_p_cores, device.cpu_e_cores) {
            (Some(p_cores), Some(e_cores)) if p_cores + e_cores > 0 => {
                let total = (p_cores + e_cores) as f32;
                let weighted = p_cpu_usage_pct * p_cores as f32 + e_cpu_usage_pct * e_cores as f32;
                let combined = zero_div(weighted, total);
                if combined > 0.0 {
                    combined
                } else {
                    cpu_usage_pct
                }
            }
            _ => cpu_usage_pct,
        };

        let tracked_power = match (cpu_power, gpu_power) {
            (Some(cpu), Some(gpu)) => Some(cpu + gpu),
            (Some(cpu), None) => Some(cpu),
            (None, Some(gpu)) => Some(gpu),
            (None, None) => None,
        };

        Metrics {
            temp: TempMetrics { cpu_temp, gpu_temp },
            power: PowerMetrics {
                cpu_power,
                gpu_power,
                sys_power,
                tracked_power,
            },
            memory: MemMetrics {
                ram_total: self.ram_total_bytes,
                ram_usage: self.ram_used_bytes,
                swap_total: self.swap_total_bytes,
                swap_usage: self.swap_used_bytes,
            },
            cpu_usage: (
                if self.cpu_freq_mhz > 0 {
                    self.cpu_freq_mhz
                } else {
                    self.cpu_base_freq_mhz
                },
                cpu_usage_pct,
            ),
            e_cpu_usage: (self.e_cpu_freq_mhz.unwrap_or_default(), e_cpu_usage_pct),
            p_cpu_usage: (self.p_cpu_freq_mhz.unwrap_or_default(), p_cpu_usage_pct),
            cpu_usage_pct: combined_cpu_usage_pct,
            gpu_usage: (
                self.gpu_freq_mhz.unwrap_or_default(),
                normalize_ratio(self.gpu_usage_percent.unwrap_or_default()),
            ),
        }
    }
}

fn normalize_value(value: Option<f32>) -> Option<f32> {
    value.filter(|x| x.is_finite() && *x > 0.0)
}

fn normalize_ratio(value: f32) -> f32 {
    (value / 100.0).clamp(0.0, 1.0)
}

#[derive(Clone, Copy)]
struct IntelCpuSpec {
    p_cores: u32,
    e_cores: u32,
    p_base_freq_mhz: u32,
    e_base_freq_mhz: u32,
    p_max_freq_mhz: u32,
    e_max_freq_mhz: u32,
}

fn intel_cpu_spec(cpu_name: &str) -> Option<IntelCpuSpec> {
    let name = cpu_name.to_ascii_lowercase();

    if name.contains("i7-13700f") || name.contains("i7-13700") {
        Some(IntelCpuSpec {
            p_cores: 8,
            e_cores: 8,
            p_base_freq_mhz: 2100,
            e_base_freq_mhz: 1500,
            p_max_freq_mhz: 5100,
            e_max_freq_mhz: 4100,
        })
    } else {
        None
    }
}

fn enrich_device_info(mut info: DeviceInfo) -> DeviceInfo {
    if !info.cpu_vendor.to_ascii_lowercase().contains("intel") {
        return info;
    }

    if let Some(spec) = intel_cpu_spec(&info.cpu_name) {
        info.cpu_p_cores = Some(spec.p_cores);
        info.cpu_e_cores = Some(spec.e_cores);
        info.cpu_p_base_freq_mhz = Some(spec.p_base_freq_mhz);
        info.cpu_e_base_freq_mhz = Some(spec.e_base_freq_mhz);
        info.cpu_p_max_freq_mhz = Some(spec.p_max_freq_mhz);
        info.cpu_e_max_freq_mhz = Some(spec.e_max_freq_mhz);
        info.cpu_base_freq_mhz = spec.p_base_freq_mhz;
    }

    info
}

#[cfg(windows)]
const DEVICE_INFO_SCRIPT: &str = r#"
$os = Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue
$cpu = Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1
$gpu = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -and $_.Name -notmatch 'Intel' } |
  Sort-Object -Property AdapterRAM -Descending |
  Select-Object -First 1
$backend = if (Get-Command nvidia-smi -ErrorAction SilentlyContinue) { 'nvidia-smi' } else { 'none' }

[pscustomobject]@{
  machine_name = [string]$env:COMPUTERNAME
  os_version = if ($os) { "$($os.Caption) $($os.Version)" } else { 'Windows' }
  cpu_name = if ($cpu) { [string]$cpu.Name } else { 'Unknown CPU' }
  cpu_vendor = if ($cpu) { [string]$cpu.Manufacturer } else { 'Unknown' }
  cpu_cores = if ($cpu) { [int]$cpu.NumberOfCores } else { 0 }
  cpu_threads = if ($cpu) { [int]$cpu.NumberOfLogicalProcessors } else { 0 }
  cpu_base_freq_mhz = if ($cpu) { [int]$cpu.MaxClockSpeed } else { 0 }
  gpu_name = if ($gpu) { [string]$gpu.Name } else { 'Unknown GPU' }
  gpu_vendor = if ($gpu) {
    if ($gpu.AdapterCompatibility) { [string]$gpu.AdapterCompatibility }
    elseif ($gpu.PNPDeviceID -match 'VEN_10DE') { 'NVIDIA' }
    elseif ($gpu.PNPDeviceID -match 'VEN_1002|VEN_1022') { 'AMD' }
    else { 'Unknown' }
  } else { 'Unknown' }
  gpu_backend = $backend
} | ConvertTo-Json -Compress
"#;

#[cfg(windows)]
const STATIC_MEMORY_SCRIPT: &str = r#"
$os = Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue
$pageFiles = Get-CimInstance Win32_PageFileUsage -ErrorAction SilentlyContinue

$ramTotalBytes = if ($os) { [uint64]([int64]$os.TotalVisibleMemorySize * 1KB) } else { 0 }
$swapTotalMb = [double](($pageFiles | Measure-Object -Property AllocatedBaseSize -Sum).Sum)
if (-not $swapTotalMb) { $swapTotalMb = 0 }

[pscustomobject]@{
  ram_total_bytes = $ramTotalBytes
  swap_total_bytes = [uint64]($swapTotalMb * 1MB)
} | ConvertTo-Json -Compress
"#;

#[cfg(windows)]
const FAST_SCRIPT: &str = r#"
$cpuUsage = 0.0
$cpuFreq = 0
$gpuUsage = $null
$gpuFreq = $null
$gpuTemp = $null
$gpuPower = $null
$cpuTemp = $null
$eCpuUsage = $null
$eCpuFreq = $null
$pCpuUsage = $null
$pCpuFreq = $null

function Get-OhmSensors {
  $dllCandidates = @(
    $(if ($env:WINMON_STABLE_DIR) { Join-Path $env:WINMON_STABLE_DIR 'third_party\ohm\OpenHardwareMonitorLib.dll' } else { $null }),
    $(if ($env:WINMON_EXE_DIR) { Join-Path $env:WINMON_EXE_DIR 'third_party\ohm\OpenHardwareMonitorLib.dll' } else { $null }),
    (Join-Path (Get-Location) 'third_party\ohm\OpenHardwareMonitorLib.dll')
  ) | Where-Object { $_ -and (Test-Path $_) }

  $dll = $dllCandidates | Select-Object -First 1
  if (-not $dll) {
    return $null
  }

  $computer = $null
  try {
    $null = [System.Reflection.Assembly]::LoadFrom($dll)
    $computer = New-Object OpenHardwareMonitor.Hardware.Computer
    $computer.IsCpuEnabled = $true
    $computer.Open($false)

    $cpuHardware = @($computer.Hardware | Where-Object {
      $_.HardwareType -eq [OpenHardwareMonitor.Hardware.HardwareType]::Cpu
    })

    foreach ($hardware in $cpuHardware) {
      $hardware.Update()
    }

    return @(
      foreach ($hardware in $cpuHardware) {
        foreach ($sensor in $hardware.Sensors) {
          if ($null -ne $sensor.Value) {
            [pscustomobject]@{
              SensorType = [string]$sensor.SensorType
              Name = [string]$sensor.Name
              Value = [double]$sensor.Value
            }
          }
        }
      }
    )
  } catch {
    return $null
  } finally {
    if ($computer) {
      $computer.Close()
    }
  }
}

function Get-OhmCpuTemp {
  param([object[]]$sensors)

  if (-not $sensors) {
    return $null
  }

  $sensor = $sensors |
    Where-Object {
      $_.SensorType -eq 'Temperature' -and $_.Name -eq 'CPU Package'
    } |
    Select-Object -First 1

  if (-not $sensor) {
    $sensor = $sensors |
      Where-Object {
        $_.SensorType -eq 'Temperature' -and ($_.Name -eq 'Core Max' -or $_.Name -eq 'Core Average')
      } |
      Sort-Object Name |
      Select-Object -First 1
  }

  if ($sensor -and $null -ne $sensor.Value -and [double]$sensor.Value -gt 0) {
    return [math]::Round([double]$sensor.Value, 1)
  }

  return $null
}

function Get-OhmHybridCpu {
  param([object[]]$sensors)

  if (-not $sensors) {
    return $null
  }

  $clockSensors = $sensors | Where-Object { $_.SensorType -eq 'Clock' }
  $loadSensors = $sensors | Where-Object { $_.SensorType -eq 'Load' }
  $totalLoad = $loadSensors | Where-Object { $_.Name -eq 'Total' -and $null -ne $_.Value } | Select-Object -First 1

  $pFreqValues = @($clockSensors | Where-Object { $_.Name -match '^P-Core #' -and $null -ne $_.Value } | ForEach-Object { [double]$_.Value })
  $eFreqValues = @($clockSensors | Where-Object { $_.Name -match '^E-Core #' -and $null -ne $_.Value } | ForEach-Object { [double]$_.Value })
  $pCoreCount = $pFreqValues.Count
  $eCoreCount = $eFreqValues.Count

  $pLoadValues = @()
  $eLoadValues = @()
  foreach ($sensor in $loadSensors) {
    if ($null -eq $sensor.Value) {
      continue
    }
    if ($sensor.Name -match '^Core #(\d+)( Thread #\d+)?$') {
      $coreIndex = [int]$matches[1]
      $value = [double]$sensor.Value
      if ($pCoreCount -gt 0 -and $coreIndex -le $pCoreCount) {
        $pLoadValues += $value
      } elseif ($eCoreCount -gt 0 -and $coreIndex -le ($pCoreCount + $eCoreCount)) {
        $eLoadValues += $value
      }
    }
  }

  return [pscustomobject]@{
    cpu_freq_mhz = if (($pFreqValues.Count + $eFreqValues.Count) -gt 0) { [int][math]::Round(((@($pFreqValues + $eFreqValues) | Measure-Object -Average).Average)) } else { $null }
    cpu_usage_percent = if ($totalLoad) { [math]::Round([double]$totalLoad.Value, 2) } elseif (($pLoadValues.Count + $eLoadValues.Count) -gt 0) { [math]::Round(((@($pLoadValues + $eLoadValues) | Measure-Object -Average).Average), 2) } else { $null }
    p_cpu_freq_mhz = if ($pFreqValues.Count -gt 0) { [int][math]::Round((($pFreqValues | Measure-Object -Average).Average)) } else { $null }
    p_cpu_usage_percent = if ($pLoadValues.Count -gt 0) { [math]::Round((($pLoadValues | Measure-Object -Average).Average), 2) } else { $null }
    e_cpu_freq_mhz = if ($eFreqValues.Count -gt 0) { [int][math]::Round((($eFreqValues | Measure-Object -Average).Average)) } else { $null }
    e_cpu_usage_percent = if ($eLoadValues.Count -gt 0) { [math]::Round((($eLoadValues | Measure-Object -Average).Average), 2) } else { $null }
  }
}

$ohmSensors = Get-OhmSensors
$cpuTemp = Get-OhmCpuTemp $ohmSensors
if ($ohmSensors) {
  $hybridCpu = Get-OhmHybridCpu $ohmSensors
  if ($hybridCpu) {
    $cpuUsage = $hybridCpu.cpu_usage_percent
    $cpuFreq = $hybridCpu.cpu_freq_mhz
    $pCpuUsage = $hybridCpu.p_cpu_usage_percent
    $pCpuFreq = $hybridCpu.p_cpu_freq_mhz
    $eCpuUsage = $hybridCpu.e_cpu_usage_percent
    $eCpuFreq = $hybridCpu.e_cpu_freq_mhz
  }
}

$smi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
if ($smi) {
  $line = & $smi.Source --query-gpu=utilization.gpu,clocks.current.graphics,temperature.gpu,power.draw --format=csv,noheader,nounits 2>$null |
    Select-Object -First 1
  if ($line) {
    $parts = $line -split '\s*,\s*'
    if ($parts.Length -ge 4) {
      $gpuUsage = [double]$parts[0]
      $gpuFreq = [int]$parts[1]
      $gpuTemp = [double]$parts[2]
      $gpuPower = [double]$parts[3]
    }
  }
}

[pscustomobject]@{
  cpu_usage_percent = [math]::Round($cpuUsage, 2)
  cpu_freq_mhz = $cpuFreq
  e_cpu_usage_percent = $eCpuUsage
  e_cpu_freq_mhz = $eCpuFreq
  p_cpu_usage_percent = $pCpuUsage
  p_cpu_freq_mhz = $pCpuFreq
  gpu_usage_percent = if ($null -ne $gpuUsage) { [math]::Round($gpuUsage, 2) } else { $null }
  gpu_freq_mhz = $gpuFreq
  cpu_temp_c = $cpuTemp
  gpu_temp_c = $gpuTemp
  gpu_power_w = $gpuPower
} | ConvertTo-Json -Compress
"#;

#[cfg(windows)]
const SLOW_SCRIPT: &str = r#"
$swapUsedBytes = 0
$cpuPower = $null
$sysPower = $null

try {
  $pageFiles = Get-CimInstance Win32_PageFileUsage -ErrorAction SilentlyContinue
  $swapUsedMb = [double](($pageFiles | Measure-Object -Property CurrentUsage -Sum).Sum)
  if ($swapUsedMb -gt 0) {
    $swapUsedBytes = [uint64]($swapUsedMb * 1MB)
  }
} catch {}

try {
  $energyCounter = Get-Counter '\Energy Meter(*)\Power' -ErrorAction Stop
  $pkgPower = $energyCounter.CounterSamples |
    Where-Object { $_.Path -match 'rapl_package\d+_pkg' } |
    Select-Object -First 1
  if ($pkgPower -and $pkgPower.CookedValue -gt 0) {
    $cpuPower = [math]::Round(([double]$pkgPower.CookedValue) / 1000.0, 2)
  }
} catch {}

[pscustomobject]@{
  swap_used_bytes = [uint64]$swapUsedBytes
  cpu_power_w = $cpuPower
  sys_power_w = $sysPower
} | ConvertTo-Json -Compress
"#;

pub fn load_device_info() -> WithError<DeviceInfo> {
    #[cfg(windows)]
    {
        let info: DeviceInfo = run_powershell_json(DEVICE_INFO_SCRIPT)?;
        return Ok(enrich_device_info(info));
    }

    #[cfg(not(windows))]
    {
        Err("winmon 仅支持 Windows 运行".into())
    }
}

pub fn bootstrap_runtime_assets() {
    #[cfg(windows)]
    {
        let _ = bootstrap_runtime_assets_windows();
    }
}

pub struct Sampler {
    device: DeviceInfo,
    ram_total_bytes: u64,
    swap_total_bytes: u64,
    slow_cache: Arc<RwLock<SlowCache>>,
}

impl Sampler {
    pub fn new() -> WithError<Self> {
        let device = load_device_info()?;
        let memory = load_static_memory_info()?;
        if !device.cpu_vendor.to_ascii_lowercase().contains("intel") {
            return Err("当前版本只支持 Intel CPU".into());
        }
        let slow_cache = Arc::new(RwLock::new(SlowCache::default()));
        #[cfg(windows)]
        {
            *slow_cache.write().unwrap() = load_slow_cache().unwrap_or_default();
            spawn_slow_cache_updater(Arc::clone(&slow_cache));
        }
        Ok(Self {
            device,
            ram_total_bytes: memory.ram_total_bytes,
            swap_total_bytes: memory.swap_total_bytes,
            slow_cache,
        })
    }

    pub fn get_metrics(&mut self) -> WithError<Metrics> {
        #[cfg(windows)]
        {
            let envs = winmon_runtime_envs();
            let fast: FastSnapshot = run_powershell_json_with_env(FAST_SCRIPT, &envs)?;
            let ram_used_bytes = load_ram_used_bytes()?;
            let slow = *self.slow_cache.read().unwrap();
            let sample = Snapshot {
                cpu_usage_percent: fast.cpu_usage_percent,
                cpu_freq_mhz: fast.cpu_freq_mhz,
                cpu_base_freq_mhz: self.device.cpu_base_freq_mhz,
                e_cpu_usage_percent: fast.e_cpu_usage_percent,
                e_cpu_freq_mhz: fast.e_cpu_freq_mhz,
                p_cpu_usage_percent: fast.p_cpu_usage_percent,
                p_cpu_freq_mhz: fast.p_cpu_freq_mhz,
                ram_total_bytes: self.ram_total_bytes,
                ram_used_bytes,
                swap_total_bytes: self.swap_total_bytes,
                swap_used_bytes: slow.swap_used_bytes.min(self.swap_total_bytes),
                gpu_usage_percent: fast.gpu_usage_percent,
                gpu_freq_mhz: fast.gpu_freq_mhz,
                cpu_temp_c: fast.cpu_temp_c,
                gpu_temp_c: fast.gpu_temp_c,
                cpu_power_w: slow.cpu_power_w,
                gpu_power_w: fast.gpu_power_w,
                sys_power_w: slow.sys_power_w,
            };
            return Ok(sample.into_metrics(&self.device));
        }

        #[cfg(not(windows))]
        {
            Err("winmon 仅支持 Windows 运行".into())
        }
    }

    pub fn get_device_info(&self) -> &DeviceInfo {
        &self.device
    }
}

fn load_static_memory_info() -> WithError<StaticMemoryInfo> {
    #[cfg(windows)]
    {
        let info: StaticMemoryInfo = run_powershell_json(STATIC_MEMORY_SCRIPT)?;
        return Ok(info);
    }

    #[cfg(not(windows))]
    {
        Err("winmon 仅支持 Windows 运行".into())
    }
}

#[cfg(windows)]
fn bootstrap_runtime_assets_windows() -> WithError<()> {
    let Some(stable_dir) = winmon_stable_dir() else {
        return Ok(());
    };

    std::fs::create_dir_all(&stable_dir)?;

    if let Some(current_exe) = current_exe_path() {
        let stable_exe = stable_dir.join("winmon.exe");
        if current_exe != stable_exe {
            copy_file_if_needed(&current_exe, &stable_exe)?;
        }
    }

    let stable_ohm = stable_dir
        .join("third_party")
        .join("ohm")
        .join("OpenHardwareMonitorLib.dll");
    write_embedded_file_if_needed(EMBEDDED_OHM_DLL, &stable_ohm)?;

    if std::env::var_os("WINMON_SKIP_USER_PATH").is_none() {
        ensure_user_path_contains(&stable_dir)?;
    }
    Ok(())
}

#[cfg(windows)]
fn winmon_stable_dir() -> Option<std::path::PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|appdata| std::path::PathBuf::from(appdata).join("winmon"))
}

#[cfg(windows)]
fn current_exe_path() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()
}

#[cfg(windows)]
fn current_exe_dir() -> Option<std::path::PathBuf> {
    current_exe_path().and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
}

#[cfg(windows)]
fn winmon_runtime_envs() -> Vec<(&'static str, String)> {
    let mut envs = Vec::new();
    if let Some(dir) = winmon_stable_dir() {
        envs.push(("WINMON_STABLE_DIR", dir.to_string_lossy().into_owned()));
    }
    if let Some(dir) = current_exe_dir() {
        envs.push(("WINMON_EXE_DIR", dir.to_string_lossy().into_owned()));
    }
    envs
}

#[cfg(windows)]
fn copy_file_if_needed(source: &std::path::Path, target: &std::path::Path) -> WithError<()> {
    let same_content = std::fs::read(source)
        .ok()
        .zip(std::fs::read(target).ok())
        .map(|(src, dst)| src == dst)
        .unwrap_or(false);

    if same_content {
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, target)?;
    Ok(())
}

#[cfg(windows)]
fn write_embedded_file_if_needed(bytes: &[u8], target: &std::path::Path) -> WithError<()> {
    if std::fs::read(target).ok().as_deref() == Some(bytes) {
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, bytes)?;
    Ok(())
}

#[cfg(windows)]
fn ensure_user_path_contains(dir: &std::path::Path) -> WithError<()> {
    let dir = dir.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"
$dir = '{dir}'
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$parts = @($userPath -split ';' | Where-Object {{ $_ }})
if ($parts -notcontains $dir) {{
  $newPath = if ([string]::IsNullOrWhiteSpace($userPath)) {{ $dir }} else {{ (@($parts + $dir) | Select-Object -Unique) -join ';' }}
  [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
  Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class WinmonUser32 {{
  [DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Auto)]
  public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
}}
"@ -ErrorAction SilentlyContinue
  [UIntPtr]$result = [UIntPtr]::Zero
  [void][WinmonUser32]::SendMessageTimeout([IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, 'Environment', 2, 3000, [ref]$result)
}}
"#
    );

    let output = powershell_command()
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let msg = if stderr.is_empty() {
            "更新用户 PATH 失败".to_string()
        } else {
            stderr
        };
        Err(msg.into())
    }
}

#[cfg(windows)]
fn powershell_command() -> Command {
    if let Ok(system_root) = std::env::var("SystemRoot") {
        let candidate = std::path::PathBuf::from(system_root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        if candidate.is_file() {
            return Command::new(candidate);
        }
    }

    Command::new("powershell")
}

#[cfg(windows)]
fn load_slow_cache() -> WithError<SlowCache> {
    let snapshot: SlowSnapshot = run_powershell_json(SLOW_SCRIPT)?;
    Ok(SlowCache {
        swap_used_bytes: snapshot.swap_used_bytes.unwrap_or_default(),
        cpu_power_w: normalize_value(snapshot.cpu_power_w),
        sys_power_w: normalize_value(snapshot.sys_power_w),
    })
}

#[cfg(windows)]
fn spawn_slow_cache_updater(cache: Arc<RwLock<SlowCache>>) {
    std::thread::spawn(move || {
        loop {
            if let Ok(snapshot) = load_slow_cache() {
                *cache.write().unwrap() = snapshot;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

#[cfg(not(windows))]
fn spawn_slow_cache_updater(_cache: Arc<RwLock<SlowCache>>) {}

#[cfg(windows)]
#[allow(non_snake_case)]
#[repr(C)]
struct MEMORYSTATUSEX {
    dwLength: u32,
    dwMemoryLoad: u32,
    ullTotalPhys: u64,
    ullAvailPhys: u64,
    ullTotalPageFile: u64,
    ullAvailPageFile: u64,
    ullTotalVirtual: u64,
    ullAvailVirtual: u64,
    ullAvailExtendedVirtual: u64,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalMemoryStatusEx(lpBuffer: *mut MEMORYSTATUSEX) -> i32;
}

#[cfg(windows)]
fn load_ram_used_bytes() -> WithError<u64> {
    let mut mem = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        dwMemoryLoad: 0,
        ullTotalPhys: 0,
        ullAvailPhys: 0,
        ullTotalPageFile: 0,
        ullAvailPageFile: 0,
        ullTotalVirtual: 0,
        ullAvailVirtual: 0,
        ullAvailExtendedVirtual: 0,
    };

    if unsafe { GlobalMemoryStatusEx(&mut mem) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    Ok(mem.ullTotalPhys.saturating_sub(mem.ullAvailPhys))
}

#[cfg(not(windows))]
fn load_ram_used_bytes() -> WithError<u64> {
    Err("winmon 仅支持 Windows 运行".into())
}

#[cfg(windows)]
fn run_powershell_json<T: DeserializeOwned>(script: &str) -> WithError<T> {
    run_powershell_json_with_env(script, &[])
}

#[cfg(windows)]
fn run_powershell_json_with_env<T: DeserializeOwned>(
    script: &str,
    envs: &[(&str, String)],
) -> WithError<T> {
    let command = format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $OutputEncoding = [System.Text.Encoding]::UTF8; {script}"
    );
    let mut cmd = powershell_command();
    cmd.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &command,
    ]);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let msg = if stderr.is_empty() {
            "PowerShell 执行失败".to_string()
        } else {
            stderr
        };
        return Err(msg.into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let stdout = stdout.trim();
    if stdout.is_empty() {
        return Err("PowerShell 输出为空".into());
    }

    Ok(serde_json::from_str(stdout)?)
}
