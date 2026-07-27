# LAN Save Sync

LAN Save Sync 是一个面向 Windows PC 与 Steam Deck 的安全手动文件夹同步工具。两端运行同一个 Rust Agent，通过局域网直接连接；Eden、其他模拟器或普通应用只需要配置不同的文件夹，不需要专用适配器。

> 当前状态：`0.1.0` MVP。核心同步和冲突保护已经可以运行；Steam Deck 真机、Decky Gaming Mode 和安装脚本仍需要设备验证。

## 设计目标

- 同一套 Agent 同时运行在 Windows 和 SteamOS/Linux。
- 一个配置可管理多个文件夹，两端路径不必相同，只需使用相同的 `folder.id`。
- 默认只执行能从共同版本基线安全推导方向的同步。
- 双端均被修改时停止操作，由用户明确选择 push 或 pull。
- 覆盖本地文件前自动创建历史版本。
- 收到文件后校验每个文件和整个目录的 SHA-256。
- 使用同文件系统内的目录替换，失败时尝试恢复原目录。
- 支持便携运行以及当前用户级别的安装和卸载。

## 仓库结构

```text
packages/
  sync-core/       扫描、哈希、归档、版本基线、冲突判断和 HTTP 协议
  agent/           Windows 与 Steam Deck 共用的 CLI/常驻服务
apps/
  desktop/         Windows 界面；MVP 暂时使用 Agent CLI
  decky-plugin/    Decky TypeScript UI 与 Python 本地桥接
install/
  windows/         便携、安装和卸载脚本
  steam-deck/      便携、systemd user service、Decky 安装与卸载
examples/          两端配置示例
```

平台差异只存在于界面与安装方式中。同步 server、客户端、协议和文件安全逻辑不会维护两个版本。

## 同步判定

Agent 比较三份内容哈希：

```text
local   当前本地目录
remote  当前对端目录
base    两端上一次成功同步后共同确认的版本
```

| 状态 | `auto` 行为 |
|---|---|
| local = remote | 不操作 |
| local = base，remote 已变化 | pull |
| remote = base，local 已变化 | push |
| local 与 remote 都偏离 base | 冲突，拒绝覆盖 |
| 首次连接且仅一端为空 | 从非空端同步到空端 |
| 首次连接且两端都非空且不同 | 冲突，要求选择 |

修改时间只用于显示，不参与版本胜负判断。目录哈希由排序后的相对路径、文件大小和文件 SHA-256 生成。

## 配置

先生成包含随机令牌的配置：

```powershell
lan-save-sync.exe init `
  --device-id windows-pc `
  --name "Gaming PC" `
  --output "$env:APPDATA\LanSaveSync\agent.json"
```

```bash
lan-save-sync init \
  --device-id steam-deck \
  --name "Steam Deck" \
  --output "$HOME/.config/lan-save-sync/agent.json"
```

然后分别参考 [PC 配置](examples/pc.agent.example.json) 与 [Steam Deck 配置](examples/steam-deck.agent.example.json)。关键规则：

- 两端同一个同步项目必须使用相同的 `folder.id`。
- `folder.path` 是各自系统上的绝对路径，可以完全不同。
- `peer.token` 必须填写对端配置中的 `api_token`。
- `api_token` 和真实配置禁止提交到 Git。
- 初次配置完成前可以把文件夹设置为 `"enabled": false`。

## 手动使用

启动两端 Agent：

```text
lan-save-sync --config <配置文件> serve
```

检查版本，不修改文件：

```text
lan-save-sync --config <配置文件> plan --peer steam-deck --folder eden
```

按照安全判定自动同步：

```text
lan-save-sync --config <配置文件> sync --peer steam-deck --folder eden --action auto
```

发生冲突时，先检查 `plan` 输出中的两端哈希，再明确选择：

```text
lan-save-sync ... sync --peer steam-deck --folder eden --action push --accept-conflict
lan-save-sync ... sync --peer steam-deck --folder eden --action pull --accept-conflict
```

`push` 表示当前设备覆盖对端；`pull` 表示对端覆盖当前设备。显式选择与安全建议相反的方向时也必须添加 `--accept-conflict`。

查看和恢复覆盖前的本地历史：

```text
lan-save-sync --config <配置文件> history --folder eden
lan-save-sync --config <配置文件> restore --folder eden --version <版本> --accept-overwrite
```

恢复历史前还会再备份一次当前目录。

## Windows

### 便携版

把以下文件放在同一个目录：

```text
lan-save-sync.exe
run-portable.ps1
```

运行 `run-portable.ps1`。第一次运行会在当前目录生成 `agent.json` 和 `data/`，编辑配置后再次运行即可。

### 安装

把构建产物放到 `install/windows/lan-save-sync.exe`，然后运行：

```powershell
.\install\windows\install.ps1
```

它会：

- 安装到当前用户的 `%LOCALAPPDATA%\LanSaveSync`；
- 在 `%APPDATA%\LanSaveSync` 创建配置和历史目录；
- 注册当前用户登录后自动启动，不要求管理员权限。

卸载程序并保留配置与历史：

```powershell
.\install\windows\uninstall.ps1
```

同时删除配置与历史：

```powershell
.\install\windows\uninstall.ps1 -RemoveData
```

## Steam Deck

### Agent 安装

发布包中 `install.sh`、`lan-save-sync` 与 `lan-save-sync.service` 位于同一目录：

```bash
chmod +x install.sh uninstall.sh lan-save-sync
./install.sh
```

安装脚本使用用户目录：

```text
~/.local/bin/lan-save-sync
~/.config/lan-save-sync/agent.json
~/.config/systemd/user/lan-save-sync.service
```

编辑配置后启动：

```bash
systemctl --user start lan-save-sync.service
systemctl --user status lan-save-sync.service
```

### Decky 插件

Decky 插件只是 Agent 的界面，不包含第二套同步实现。构建：

```bash
cd apps/decky-plugin
pnpm install
pnpm build
```

开发版本需要形成以下目录并放入：

```text
/home/deck/homebrew/plugins/LanSaveSync/
  dist/index.js
  main.py
  plugin.json
  package.json
  README.md
  LICENSE
```

安装包中的 `install.sh` 如果发现 `decky-plugin/LanSaveSync` 会自动复制；之后从 Decky 设置中 reload 插件或重新进入 Gaming Mode。插件当前使用配置中的第一个 peer，并列出所有已启用文件夹。

### 卸载

保留配置与历史：

```bash
./uninstall.sh
```

彻底删除：

```bash
./uninstall.sh --purge
```

移除 Decky UI 本身不会删除 Agent 配置和存档历史。

## 构建

Windows：

```powershell
cargo test --workspace
cargo build --release -p lan-save-sync
```

Steam Deck/Linux 可以在 WSL 中构建，不需要在 Deck 本机安装 Rust：

```bash
cd /mnt/c/path/to/lan-save-sync
CARGO_TARGET_DIR=target/linux cargo test --workspace
CARGO_TARGET_DIR=target/linux cargo build --release -p lan-save-sync
```

正式发布由 GitHub Actions 分别使用 Windows 与 Ubuntu Runner 构建，并额外构建 Decky 插件。

## 当前安全边界

MVP 使用 Bearer token 认证，但局域网传输还是 HTTP，尚未提供 TLS。请只在可信的家庭私有网络上使用，不要把端口转发到互联网，也不要在公共 Wi-Fi 使用。Windows 防火墙只应允许 Private network。

在加入设备证书固定或双向 TLS 前，不应把它宣传为适用于不可信网络的版本。

同步或恢复前请退出模拟器。MVP 会通过目录替换失败来阻止一部分正在使用中的文件，但尚未实现 Eden/模拟器进程检测。文件同步也不是独立备份；重要存档仍应另存一份离线备份。

## 已知限制与下一步

- 通过配置填写 IP/主机名，尚未加入 mDNS 自动发现。
- Decky UI 尚未在真实 Steam Deck Gaming Mode 验证。
- Windows MVP 使用 CLI，托盘/桌面 UI 尚未实现。
- 暂无 TLS 设备配对。
- 暂无自动模拟器运行状态检测。
- 同步是完整压缩快照传输，尚未做块级增量。

优先顺序是：Steam Deck 真机验证 → Decky UI 修正 → TLS 配对 → Windows 桌面 UI → mDNS。

## License

[MIT](LICENSE)
