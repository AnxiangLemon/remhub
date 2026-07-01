# remhub

**RDP 与 SSH 服务器的终端界面启动器。**

[English](README.md)

`remhub`（Remote Hub）是一个用 Rust 编写的远程服务器启动器。通过 TUI 管理 `servers.toml` 中的服务器列表，一键连接 Windows 远程桌面（RDP）或 SSH 会话。

## 界面预览

![remhub TUI](img/main.png)

## 功能特性

- **RDP** — 自动写入 Windows 凭据（`cmdkey`），调用 `mstsc`；也支持直接打开 `.rdp` 文件
- **SSH** — 调用系统 `ssh`；Windows 上默认在新终端窗口打开，可同时连接多台主机
- **TUI** — 搜索、分组筛选、详情面板、最近连接置顶
- **快捷键** — 数字键 1–9 快速连接，一键复制连接命令到剪贴板
- **配置** — 纯 TOML 配置，首次运行自动生成示例文件

## 环境要求

- [Rust](https://rustup.rs/)（stable）
- **RDP**：Windows + `mstsc`
- **SSH**：OpenSSH 客户端（`ssh`）；Windows 上推荐 [Windows Terminal](https://aka.ms/terminal)（`wt`）

## 快速开始

```powershell
# 克隆并编译
git clone https://github.com/AnxiangLemon/remhub.git
cd remhub
cargo build --release

# 从示例创建配置文件
Copy-Item .\servers.example.toml .\servers.toml
# 编辑 servers.toml 填入你的服务器，然后运行：
.\target\release\remhub.exe
```

开发模式：

```powershell
cargo run
```

### 配置文件位置

| 方式 | 示例 |
| --- | --- |
| 默认 | `remhub.exe` 同目录下的 `servers.toml`；`cargo run` 开发时回退到当前目录的 `./servers.toml` |
| 命令行参数 | `remhub.exe D:\configs\servers.toml` |
| 环境变量 | `$env:REMHUB_CONFIG="D:\configs\servers.toml"` |

## 配置说明

完整示例见 [`servers.example.toml`](servers.example.toml)。

```toml
[defaults]
rdp_command = "mstsc"
ssh_command = "ssh"
ssh_new_window = true          # Windows 默认：在新终端标签页中打开 SSH
ssh_extra_args = ["-o", "ServerAliveInterval=30"]

[[servers]]
name = "Windows Jumpbox"
host = "10.0.0.10"
group = "production"
protocol = "rdp"
port = 3389
user = "administrator"
password = "your-rdp-password"
expires_at = "2028-09-02"
tags = ["windows", "rdp"]

[[servers]]
name = "Linux Bastion"
host = "10.0.0.20"
group = "production"
protocol = "ssh"
port = 22
user = "ops"
private_key_path = "C:\\Users\\you\\.ssh\\id_ed25519"
tags = ["linux", "ssh"]
```

### 服务器字段

| 字段 | 说明 |
| --- | --- |
| `name` | TUI 中显示的名称 |
| `host` | 主机名或 IP |
| `protocol` | `rdp` 或 `ssh` |
| `port` | 可选端口（RDP 默认 3389，SSH 默认 22） |
| `user` | 用户名（SSH 为 `user@host`；RDP 与 `password` 配合使用） |
| `password` | RDP 密码（通过 `cmdkey` 保存；**不会**传给 SSH） |
| `private_key_path` | SSH 私钥文件路径（`ssh -i`） |
| `private_key` | 内联 SSH 私钥（运行时写入临时文件） |
| `domain` | RDP 的 Windows 域（`domain\user`） |
| `expires_at` | 过期日期 `YYYY-MM-DD`，在列表中显示 |
| `group` | 分组标签，用于筛选和显示 |
| `note` | 详情面板中的备注 |
| `tags` | 搜索用标签 |
| `rdp_file` | 直接启动 `.rdp` 文件，而非 `/v:host` |

## 键盘快捷键

| 按键 | 操作 |
| --- | --- |
| `Enter` | 连接选中的服务器 |
| `1`–`9` | 快速连接当前可见列表中的第 1–9 台 |
| `c` | 复制连接命令到剪贴板 |
| `g` | 循环切换分组筛选 |
| `/` | 搜索（名称、主机、分组、标签等） |
| `↑`/`↓` 或 `j`/`k` | 移动选中项 |
| `PageUp`/`PageDown` | 跳转 10 行 |
| `Home`/`End` | 跳到首/末台服务器 |
| `r` | 重新加载 `servers.toml` |
| `h` | 帮助 |
| `q` / `Esc` / `Ctrl+C` | 退出 |

最近连接的 5 台服务器会在启动时置顶显示。

按 `h` 可查看应用内完整快捷键列表。

## 连接方式

**RDP**

```text
cmdkey /generic:TERMSRV/host /user:user /pass:password
mstsc /v:host:port
```

**SSH**（新窗口，Windows 默认）

```text
wt new-tab --title "SSH - name" -- ssh -p port user@host
```

在 `[defaults]` 中设置 `ssh_new_window = false` 可在当前终端中运行 SSH。

## 安全提示

> **请勿将 `servers.toml` 提交到版本控制。**

该文件通常包含真实的主机名、用户名和密码。本仓库默认忽略 `servers.toml`，请仅使用 `servers.example.toml` 作为占位符模板。

最近连接记录保存在本地：

```text
%LOCALAPPDATA%\remhub\recent.toml
```

内联 SSH 私钥会写入：

```text
%TEMP%\remhub-keys\
```

## 许可证

[MIT](LICENSE)
