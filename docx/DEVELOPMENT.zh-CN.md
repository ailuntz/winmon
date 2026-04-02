# 开发

## 环境

开发和打包都按 Windows x64 走，默认工具链是 `x86_64-pc-windows-msvc`。

本地开发常用入口：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\run.ps1 debug
powershell -ExecutionPolicy Bypass -File .\scripts\run.ps1 pipe -s 1 --device-info
```

`scripts/run.ps1` 只给开发和构建用，不进最终发布包。最终用户走 release zip 或 release 里的 `install.ps1`。

## 打包

release 包由 `scripts/package.ps1` 生成：

```powershell
.\scripts\package.ps1 -Version vX.Y.Z -TargetDir target
```

当前包里只放：

- `winmon.exe`
- `LICENSE`
- `README.txt`
- `third_party/licenses/*`

`OHM` 不再带外部 exe。首次运行时由 `winmon.exe` 自己把内嵌的 `OpenHardwareMonitorLib.dll` 写到 `%APPDATA%\winmon\third_party\ohm`。

## 自举

程序启动时会先做一轮自举：

- 把当前 `winmon.exe` 同步到 `%APPDATA%\winmon\winmon.exe`
- 把内嵌的 `OpenHardwareMonitorLib.dll` 写到 `%APPDATA%\winmon\third_party\ohm`
- 把 `%APPDATA%\winmon` 写进用户 `PATH`

安装脚本和发布流程都依赖这条链，所以不要随便绕开。

`scripts/install.ps1` 现在优先用 `curl.exe` 下载 release 资产，避开 `Windows PowerShell 5.1` 下 `Invoke-WebRequest` 对 GitHub 链路不稳的问题。

## 发布

分两条：

1. push 到普通分支，触发 `check` workflow，验证格式、编译、打包
2. 打 `v*` tag，触发 `release` workflow，上传 zip、安装脚本和哈希文件

如果以后要改资产命名或下载地址，优先一起看：

- `.github/workflows/release.yml`
- `scripts/install.ps1`
- `scripts/package.ps1`

## example-grafana

`example-grafana` 现在默认按局域网直连 Windows 主机来写，Prometheus 直接抓：

- `<winmon-host>:9090`

如果 `winmon serve` 的主机或端口变了，只改 `example-grafana/prometheus.yml` 即可。

本机有 Docker 时可以直接：

```bash
cd example-grafana
docker compose up -d
```

当前面板只保留 `winmon` 已稳定输出的指标，`sys_power` 不再继续追，长期接受 `N/A`。

## 许可证

仓库自身代码现在按 `MIT` 发布。

注意区分：

- 仓库根目录 `LICENSE` 只管 `winmon` 自己的代码
- `third_party/licenses` 里保留外部依赖和参考项目的原始许可证文本

## winget

仓库里已经放了一份 `winget` manifest，路径在 `winget/manifests/a/Ailuntz/Winmon/`。

后续发新版时可以直接用：

```powershell
.\scripts\gen-winget.ps1 -Version X.Y.Z -InstallerSha256 <sha256>
```

注意两点：

- 正式提交到 `microsoft/winget-pkgs` 之前，安装包链接必须公开可访问
- schema 版本跟着社区要求走，当前默认生成 `1.12.0`
