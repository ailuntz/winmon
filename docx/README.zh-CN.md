# winmon

`winmon` 是一个给 Windows 用的终端监控工具。

英文版说明见仓库根目录 [README.md](/Volumes/usb_main/usb_main/test_bug/winmon/README.md)。

## 说明

- Windows 10/11 x64
- 设置保存在 `%APPDATA%\winmon\config.json`
- 某些机器上，`CPU temp` 和 `E-CPU` / `P-CPU` 传感器需要管理员权限；非管理员时会回落到 `N/A`

## 用法

默认直接起终端界面，也支持命令模式：

```powershell
winmon
winmon pipe -s 1 --device-info
winmon debug
winmon serve
```

## 安装

最稳的方式还是下载 release 里的 zip，解压后运行一次 `winmon.exe`。

首次运行后会把稳定副本和运行时写到 `%APPDATA%\winmon`，后面新开的 `cmd` 或 `PowerShell` 可以直接输入：

```powershell
winmon
```

如果 release 对当前账号可访问，也可以直接用 PowerShell 安装。Windows PowerShell 5.1 下更稳的是先落盘再执行：

```powershell
$p = Join-Path $env:TEMP "winmon-install.ps1"
iwr "https://github.com/ailuntz/winmon/releases/latest/download/install.ps1" -UseBasicParsing -OutFile $p
powershell -NoProfile -ExecutionPolicy Bypass -File $p
```

如果当前是在 `cmd` 里，或者是从 macOS 用 `ssh win` 连进去，直接用这两条：

```cmd
curl.exe -L --fail --silent --show-error "https://github.com/ailuntz/winmon/releases/latest/download/install.ps1" -o "%TEMP%\winmon-install.ps1"
powershell -NoProfile -ExecutionPolicy Bypass -File "%TEMP%\winmon-install.ps1"
```

## HTTP 服务

可以通过 HTTP 暴露当前指标：

```powershell
winmon serve
winmon serve --port 9090
```

可用端点：

- `GET /json`
- `GET /metrics`

仓库里也放了一个 [example-grafana](/Volumes/usb_main/usb_main/test_bug/winmon/example-grafana) 目录，方便直接接 Prometheus / Grafana。

如果 Prometheus 跑在另一台机器上，把 `example-grafana/prometheus.yml` 里的 `192.168.8.16:9090` 改成实际的 `winmon serve` 地址即可。

在当前这套局域网环境里，可以直接：

```bash
cd example-grafana
docker compose up -d
```

默认入口：

- Prometheus: `http://localhost:9091`
- Grafana: `http://localhost:9000`
- Grafana 默认账号: `winmon`
- Grafana 默认密码: `winmon`

## 致谢

- [macmon](https://github.com/vladkens/macmon) 提供了原始布局和交互方向
- [OpenHardwareMonitorLib](https://www.nuget.org/packages/OpenHardwareMonitorLib) 提供 Windows 硬件传感器访问能力

## 许可证

仓库自身代码按 `MIT` 发布。
