use std::{
    collections::{HashMap, HashSet},
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, TableState, Wrap,
    },
};
use serde::{Deserialize, Serialize};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const APP_NAME: &str = "remhub";
const DEFAULT_CONFIG_FILE: &str = "servers.toml";
const MAX_RECENT: usize = 5;

// ---------------------------------------------------------------------------
// Language / i18n
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Lang {
    #[default]
    En,
    Zh,
}

impl Lang {
    fn app_title(self) -> &'static str {
        match self {
            Lang::En => "RDP & SSH Launcher",
            Lang::Zh => "RDP 与 SSH 启动器",
        }
    }

    fn ungrouped_label(self) -> &'static str {
        match self {
            Lang::En => "Ungrouped",
            Lang::Zh => "未分组",
        }
    }

    // -- status messages --------------------------------------------------

    fn loaded_servers(self, count: usize) -> String {
        match self {
            Lang::En => format!("Loaded {count} server(s). Press h for help."),
            Lang::Zh => format!("已加载 {count} 台服务器。按 h 查看帮助。"),
        }
    }

    fn group_filter_msg(self, group: &str) -> String {
        match self {
            Lang::En => format!("Group filter: {group}"),
            Lang::Zh => format!("分组筛选：{group}"),
        }
    }

    fn group_filter_all(self) -> &'static str {
        match self {
            Lang::En => "Group filter: all",
            Lang::Zh => "分组筛选：全部",
        }
    }

    fn group_filter_label_all(self) -> &'static str {
        match self {
            Lang::En => "all",
            Lang::Zh => "全部",
        }
    }

    fn recent_save_failed<E: std::fmt::Display>(self, err: &E) -> String {
        match self {
            Lang::En => format!("Could not save recent list: {err:#}"),
            Lang::Zh => format!("无法保存最近连接列表：{err:#}"),
        }
    }

    fn reloaded(self, path: &Path) -> String {
        match self {
            Lang::En => format!("Reloaded {}", path.display()),
            Lang::Zh => format!("已重新加载 {}", path.display()),
        }
    }

    fn reload_failed<E: std::fmt::Display>(self, err: &E) -> String {
        match self {
            Lang::En => format!("Reload failed: {err:#}"),
            Lang::Zh => format!("重新加载失败：{err:#}"),
        }
    }

    fn no_server_selected(self) -> &'static str {
        match self {
            Lang::En => "No server selected.",
            Lang::Zh => "未选择服务器。",
        }
    }

    fn copied(self, cmd: &str) -> String {
        match self {
            Lang::En => format!("Copied: {cmd}"),
            Lang::Zh => format!("已复制：{cmd}"),
        }
    }

    fn copy_failed<E: std::fmt::Display>(self, err: &E) -> String {
        match self {
            Lang::En => format!("Copy failed: {err:#}"),
            Lang::Zh => format!("复制失败：{err:#}"),
        }
    }

    fn add_start(self) -> &'static str {
        match self {
            Lang::En => "Adding server. Enter/Tab next, Esc cancel.",
            Lang::Zh => "正在新增服务器。Enter/Tab 下一项，Esc 取消。",
        }
    }

    fn add_name_exists(self, name: &str) -> String {
        match self {
            Lang::En => format!("Server name already exists: {name}"),
            Lang::Zh => format!("服务器名称已存在：{name}"),
        }
    }

    fn add_saved(self, name: &str) -> String {
        match self {
            Lang::En => format!("Added server: {name}"),
            Lang::Zh => format!("已添加服务器：{name}"),
        }
    }

    fn add_save_failed<E: std::fmt::Display>(self, err: &E) -> String {
        match self {
            Lang::En => format!("Failed to save new server: {err:#}"),
            Lang::Zh => format!("保存新增服务器失败：{err:#}"),
        }
    }

    fn add_cancelled(self) -> &'static str {
        match self {
            Lang::En => "Add cancelled.",
            Lang::Zh => "已取消新增。",
        }
    }

    fn edit_start(self) -> &'static str {
        match self {
            Lang::En => "Editing server. Enter/Tab next, Esc cancel.",
            Lang::Zh => "正在编辑服务器。Enter/Tab 下一项，Esc 取消。",
        }
    }

    fn edit_saved(self, name: &str) -> String {
        match self {
            Lang::En => format!("Updated server: {name}"),
            Lang::Zh => format!("已更新服务器：{name}"),
        }
    }

    fn edit_save_failed<E: std::fmt::Display>(self, err: &E) -> String {
        match self {
            Lang::En => format!("Failed to save changes: {err:#}"),
            Lang::Zh => format!("保存修改失败：{err:#}"),
        }
    }

    fn edit_cancelled(self) -> &'static str {
        match self {
            Lang::En => "Edit cancelled.",
            Lang::Zh => "已取消编辑。",
        }
    }

    fn delete_confirm_msg(self, name: &str) -> String {
        match self {
            Lang::En => format!("Delete {name}? Press y to confirm, n or Esc to cancel."),
            Lang::Zh => format!("确认删除 {name}？按 y 删除，按 n 或 Esc 取消。"),
        }
    }

    fn delete_cancelled(self) -> &'static str {
        match self {
            Lang::En => "Delete cancelled.",
            Lang::Zh => "已取消删除。",
        }
    }

    fn delete_pending_none(self) -> &'static str {
        match self {
            Lang::En => "No server pending deletion.",
            Lang::Zh => "没有待删除的服务器。",
        }
    }

    fn delete_missing(self) -> &'static str {
        match self {
            Lang::En => "Server to delete no longer exists.",
            Lang::Zh => "待删除服务器不存在。",
        }
    }

    fn delete_saved(self, name: &str) -> String {
        match self {
            Lang::En => format!("Deleted server: {name}"),
            Lang::Zh => format!("已删除服务器：{name}"),
        }
    }

    fn delete_recent_failed<E: std::fmt::Display>(self, name: &str, err: &E) -> String {
        match self {
            Lang::En => format!("Deleted {name}, but failed to save recent list: {err:#}"),
            Lang::Zh => format!("已删除 {name}，但最近连接保存失败：{err:#}"),
        }
    }

    fn delete_failed<E: std::fmt::Display>(self, err: &E) -> String {
        match self {
            Lang::En => format!("Delete failed: {err:#}"),
            Lang::Zh => format!("删除失败：{err:#}"),
        }
    }

    fn search_prompt(self) -> &'static str {
        match self {
            Lang::En => "Type to filter servers. Enter confirms, Esc returns.",
            Lang::Zh => "输入关键字筛选服务器。Enter 确认，Esc 返回。",
        }
    }

    fn search_kept(self) -> &'static str {
        match self {
            Lang::En => "Search kept. Press / to edit or Backspace to clear text.",
            Lang::Zh => "已保留搜索条件。按 / 继续编辑，按 Backspace 删除文本。",
        }
    }

    fn ready_enter(self) -> &'static str {
        match self {
            Lang::En => "Ready. Press Enter again to connect.",
            Lang::Zh => "已就绪。再次按 Enter 连接。",
        }
    }

    fn shortcut_no_server(self, n: usize) -> String {
        match self {
            Lang::En => format!("No server at shortcut {}.", n),
            Lang::Zh => format!("快捷键 {} 没有对应服务器。", n),
        }
    }

    fn launch_failed<E: std::fmt::Display>(self, err: &E) -> String {
        match self {
            Lang::En => format!("Launch failed: {err:#}"),
            Lang::Zh => format!("启动失败：{err:#}"),
        }
    }

    fn rdp_started(self, name: &str, has_cred: bool) -> String {
        match self {
            Lang::En => {
                let tail = if has_cred { " with saved credential" } else { "" };
                format!("Started RDP session for {name}{tail}")
            }
            Lang::Zh => {
                let tail = if has_cred { "（已保存凭据）" } else { "" };
                format!("已启动 {name} 的 RDP 会话{tail}")
            }
        }
    }

    fn ssh_new_window(self, name: &str) -> String {
        match self {
            Lang::En => format!("Started SSH session for {name} in a new window"),
            Lang::Zh => format!("已在新窗口启动 {name} 的 SSH 会话"),
        }
    }

    fn ssh_ended(self, name: &str) -> String {
        match self {
            Lang::En => format!("SSH session ended for {name}"),
            Lang::Zh => format!("{name} 的 SSH 会话已结束"),
        }
    }

    // -- form validation errors -------------------------------------------

    fn err_name_required(self) -> &'static str {
        match self {
            Lang::En => "Name is required.",
            Lang::Zh => "名称不能为空。",
        }
    }

    fn err_host_required(self) -> &'static str {
        match self {
            Lang::En => "Host is required.",
            Lang::Zh => "主机不能为空。",
        }
    }

    fn err_expires_whitespace(self) -> &'static str {
        match self {
            Lang::En => "Expiry date must not contain whitespace.",
            Lang::Zh => "过期日期不能包含空格。",
        }
    }

    fn err_invalid_protocol(self) -> &'static str {
        match self {
            Lang::En => "Protocol must be rdp or ssh.",
            Lang::Zh => "协议只能填写 rdp 或 ssh。",
        }
    }

    fn err_port_range(self) -> &'static str {
        match self {
            Lang::En => "Port must be a number between 1 and 65535.",
            Lang::Zh => "端口必须是 1-65535 之间的数字。",
        }
    }

    // -- error contexts (with_context / bail) -----------------------------

    fn err_cant_start(self, cmd: &str) -> String {
        match self {
            Lang::En => format!("could not start {cmd}"),
            Lang::Zh => format!("无法启动 {cmd}"),
        }
    }

    fn err_cant_start_wt(self, cmd: &str) -> String {
        match self {
            Lang::En => format!("could not start {cmd} in Windows Terminal"),
            Lang::Zh => format!("无法在 Windows Terminal 中启动 {cmd}"),
        }
    }

    fn err_cant_start_new_window(self, cmd: &str) -> String {
        match self {
            Lang::En => format!("could not start {cmd} in a new window"),
            Lang::Zh => format!("无法在新窗口中启动 {cmd}"),
        }
    }

    fn err_cant_run_cmdkey(self, target: &str) -> String {
        match self {
            Lang::En => format!("could not run cmdkey for {target}"),
            Lang::Zh => format!("无法为 {target} 运行 cmdkey"),
        }
    }

    fn err_cmdkey_failed(self, target: &str) -> String {
        match self {
            Lang::En => format!("cmdkey failed for {target}"),
            Lang::Zh => format!("cmdkey 处理 {target} 失败"),
        }
    }

    fn err_cant_create_key_dir(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not create key directory {}", path.display()),
            Lang::Zh => format!("无法创建密钥目录 {}", path.display()),
        }
    }

    fn err_cant_write_key(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not write SSH private key {}", path.display()),
            Lang::Zh => format!("无法写入 SSH 私钥 {}", path.display()),
        }
    }

    fn err_cant_read_config(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not read config {}", path.display()),
            Lang::Zh => format!("无法读取配置 {}", path.display()),
        }
    }

    fn err_cant_parse_config(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not parse config {}", path.display()),
            Lang::Zh => format!("无法解析配置 {}", path.display()),
        }
    }

    fn err_cant_create_config_dir(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not create config directory {}", path.display()),
            Lang::Zh => format!("无法创建配置目录 {}", path.display()),
        }
    }

    fn err_cant_write_sample_config(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not create sample config {}", path.display()),
            Lang::Zh => format!("无法创建示例配置 {}", path.display()),
        }
    }

    fn err_cant_write_config(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not write config {}", path.display()),
            Lang::Zh => format!("无法写入配置 {}", path.display()),
        }
    }

    fn err_cant_read_recent(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not read recent file {}", path.display()),
            Lang::Zh => format!("无法读取最近连接文件 {}", path.display()),
        }
    }

    fn err_cant_parse_recent(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not parse recent file {}", path.display()),
            Lang::Zh => format!("无法解析最近连接文件 {}", path.display()),
        }
    }

    fn err_cant_create_recent_dir(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not create recent directory {}", path.display()),
            Lang::Zh => format!("无法创建最近连接目录 {}", path.display()),
        }
    }

    fn err_cant_write_recent(self, path: &Path) -> String {
        match self {
            Lang::En => format!("could not write recent file {}", path.display()),
            Lang::Zh => format!("无法写入最近连接文件 {}", path.display()),
        }
    }

    fn err_cant_run_clip(self) -> &'static str {
        match self {
            Lang::En => "could not run clip.exe",
            Lang::Zh => "无法运行 clip.exe",
        }
    }

    fn err_clip_stdin(self) -> &'static str {
        match self {
            Lang::En => "clip stdin unavailable",
            Lang::Zh => "clip 标准输入不可用",
        }
    }

    fn err_cant_write_clip(self) -> &'static str {
        match self {
            Lang::En => "could not write to clip.exe",
            Lang::Zh => "无法写入 clip.exe",
        }
    }

    fn err_clip_exit(self) -> &'static str {
        match self {
            Lang::En => "clip.exe did not exit cleanly",
            Lang::Zh => "clip.exe 未正常退出",
        }
    }

    #[cfg(not(windows))]
    fn err_cant_run_program(self, program: &str) -> String {
        match self {
            Lang::En => format!("could not run {program}"),
            Lang::Zh => format!("无法运行 {program}"),
        }
    }

    #[cfg(not(windows))]
    fn err_clipboard_stdin(self) -> &'static str {
        match self {
            Lang::En => "clipboard stdin unavailable",
            Lang::Zh => "剪贴板工具标准输入不可用",
        }
    }

    #[cfg(not(windows))]
    fn err_clip_not_found(self) -> &'static str {
        match self {
            Lang::En => "no clipboard tool found (pbcopy, xclip, wl-copy)",
            Lang::Zh => "未找到剪贴板工具（pbcopy、xclip、wl-copy）",
        }
    }

    // -- column headers ---------------------------------------------------

    fn col_type(self) -> &'static str {
        match self {
            Lang::En => "Type",
            Lang::Zh => "类型",
        }
    }

    fn col_name(self) -> &'static str {
        match self {
            Lang::En => "Name",
            Lang::Zh => "名称",
        }
    }

    fn col_address(self) -> &'static str {
        match self {
            Lang::En => "Address",
            Lang::Zh => "地址",
        }
    }

    fn col_expires(self) -> &'static str {
        match self {
            Lang::En => "Expires",
            Lang::Zh => "过期",
        }
    }

    fn col_group(self) -> &'static str {
        match self {
            Lang::En => "Group",
            Lang::Zh => "分组",
        }
    }

    fn servers_title(self, filtered: usize, total: usize) -> String {
        match self {
            Lang::En => format!(" Servers ({filtered}/{total}) "),
            Lang::Zh => format!(" 服务器 ({filtered}/{total}) "),
        }
    }

    // -- side panel -------------------------------------------------------

    fn detail_title(self) -> &'static str {
        match self {
            Lang::En => " Details ",
            Lang::Zh => " 详情 ",
        }
    }

    fn detail_host(self) -> &'static str {
        match self {
            Lang::En => "Host     : ",
            Lang::Zh => "主机     : ",
        }
    }

    fn detail_port(self) -> &'static str {
        match self {
            Lang::En => "Port     : ",
            Lang::Zh => "端口     : ",
        }
    }

    fn detail_user(self) -> &'static str {
        match self {
            Lang::En => "User     : ",
            Lang::Zh => "用户     : ",
        }
    }

    fn detail_expires(self) -> &'static str {
        match self {
            Lang::En => "Expires  : ",
            Lang::Zh => "过期     : ",
        }
    }

    fn detail_password(self) -> &'static str {
        match self {
            Lang::En => "Password : ",
            Lang::Zh => "密码     : ",
        }
    }

    fn detail_ssh_key(self) -> &'static str {
        match self {
            Lang::En => "SSH key  : ",
            Lang::Zh => "SSH 密钥 : ",
        }
    }

    fn detail_group_label(self) -> &'static str {
        match self {
            Lang::En => "Group    : ",
            Lang::Zh => "分组     : ",
        }
    }

    fn detail_recent(self) -> &'static str {
        match self {
            Lang::En => "Recent   : ",
            Lang::Zh => "最近     : ",
        }
    }

    fn detail_command(self) -> &'static str {
        match self {
            Lang::En => "Command  : ",
            Lang::Zh => "命令     : ",
        }
    }

    fn detail_tags(self) -> &'static str {
        match self {
            Lang::En => "Tags     : ",
            Lang::Zh => "标签     : ",
        }
    }

    fn detail_saved(self) -> &'static str {
        match self {
            Lang::En => "saved",
            Lang::Zh => "已保存",
        }
    }

    fn detail_yes(self) -> &'static str {
        match self {
            Lang::En => "yes",
            Lang::Zh => "是",
        }
    }

    fn detail_no(self) -> &'static str {
        match self {
            Lang::En => "no",
            Lang::Zh => "否",
        }
    }

    fn detail_no_note(self) -> &'static str {
        match self {
            Lang::En => "No note.",
            Lang::Zh => "暂无备注。",
        }
    }

    fn detail_no_match(self) -> &'static str {
        match self {
            Lang::En => "No matching server.",
            Lang::Zh => "没有匹配的服务器。",
        }
    }

    // -- footer -----------------------------------------------------------

    fn footer_connect(self) -> &'static str {
        match self {
            Lang::En => " connect  ",
            Lang::Zh => " 连接  ",
        }
    }

    fn footer_quick(self) -> &'static str {
        match self {
            Lang::En => " quick  ",
            Lang::Zh => " 快连  ",
        }
    }

    fn footer_copy(self) -> &'static str {
        match self {
            Lang::En => " copy  ",
            Lang::Zh => " 复制  ",
        }
    }

    fn footer_group_short(self) -> &'static str {
        match self {
            Lang::En => "group",
            Lang::Zh => "分组",
        }
    }

    fn footer_help(self) -> &'static str {
        match self {
            Lang::En => " help  ",
            Lang::Zh => " 帮助  ",
        }
    }

    fn footer_quit(self) -> &'static str {
        match self {
            Lang::En => " quit",
            Lang::Zh => " 退出",
        }
    }

    fn footer_search(self) -> &'static str {
        match self {
            Lang::En => "/ filter",
            Lang::Zh => "/ 筛选",
        }
    }

    // -- help panel -------------------------------------------------------

    fn help_title(self) -> String {
        match self {
            Lang::En => format!("{APP_NAME} help"),
            Lang::Zh => format!("{APP_NAME} 帮助"),
        }
    }

    fn help_block_title(self) -> &'static str {
        match self {
            Lang::En => " Help ",
            Lang::Zh => " 帮助 ",
        }
    }

    fn help_lines(self) -> Vec<&'static str> {
        match self {
            Lang::En => vec![
                "Enter       Connect to selected server",
                "1-9         Quick connect to visible servers 1 through 9",
                "c           Copy connection command to clipboard",
                "a           Add a new server (saved to servers.toml)",
                "i           Edit selected server (saved to servers.toml)",
                "d / Delete  Delete selected server (with confirmation)",
                "g           Cycle group filter (all -> group1 -> ...)",
                "/           Search by name, host, group, protocol, or tags",
                "h           This help panel",
                "q / Esc     Quit",
                "Up/Down     Move selection",
                "j / k       Move selection (vim-style)",
                "PageUp/Down Jump 10 rows",
                "Home / End  Jump to first / last server",
                "r           Reload servers.toml",
                "",
                "Recent connections (last 5) are pinned to the top on startup.",
                "RDP uses mstsc by default. SSH uses ssh by default.",
                "On Windows, SSH opens in a new terminal window by default.",
                "Set defaults.ssh_new_window = false to connect in this window.",
                "Press any key to close this panel.",
            ],
            Lang::Zh => vec![
                "Enter       连接选中的服务器",
                "1-9         快速连接当前可见列表中的第 1-9 台",
                "c           复制连接命令到剪贴板",
                "a           新增服务器，保存到 servers.toml",
                "i           编辑选中的服务器，保存到 servers.toml",
                "d / Delete  删除选中的服务器，需要确认",
                "g           循环切换分组筛选（全部 -> 分组1 -> ...）",
                "/           按名称、主机、分组、协议或标签搜索",
                "h           显示此帮助面板",
                "q / Esc     退出",
                "Up/Down     移动选中项",
                "j / k       移动选中项（vim 风格）",
                "PageUp/Down 跳转 10 行",
                "Home / End  跳到第一台 / 最后一台服务器",
                "r           重新加载 servers.toml",
                "",
                "最近连接的 5 台服务器会在启动时置顶。",
                "RDP 默认使用 mstsc，SSH 默认使用 ssh。",
                "在 Windows 上，SSH 默认会在新终端窗口中打开。",
                "设置 defaults.ssh_new_window = false 可在当前窗口连接。",
                "按任意键关闭此面板。",
            ],
        }
    }

    // -- add form ---------------------------------------------------------

    fn edit_form_title(self) -> &'static str {
        match self {
            Lang::En => "Edit Server",
            Lang::Zh => "编辑服务器",
        }
    }

    fn edit_form_block_title(self) -> &'static str {
        match self {
            Lang::En => " Edit ",
            Lang::Zh => " 编辑 ",
        }
    }

    fn add_form_title(self) -> &'static str {
        match self {
            Lang::En => "Add Server",
            Lang::Zh => "新增服务器",
        }
    }

    fn add_form_block_title(self) -> &'static str {
        match self {
            Lang::En => " Add ",
            Lang::Zh => " 新增 ",
        }
    }

    fn add_form_instructions(self) -> &'static str {
        match self {
            Lang::En => "Enter/Tab next, Shift+Tab/\u{2191} previous, Enter on last field saves, Esc cancels.",
            Lang::Zh => "Enter/Tab 下一项，Shift+Tab/↑ 上一项，最后一项 Enter 保存，Esc 取消。",
        }
    }

    fn add_form_protocol_instructions(self) -> &'static str {
        match self {
            Lang::En => "Space toggles rdp/ssh · Backspace resets to rdp",
            Lang::Zh => "空格切换 rdp/ssh · Backspace 重置为 rdp",
        }
    }

    fn add_field_label(self, field: AddField) -> &'static str {
        match self {
            Lang::En => match field {
                AddField::Name => "Name",
                AddField::Host => "Host",
                AddField::Protocol => "Protocol",
                AddField::Port => "Port",
                AddField::User => "User",
                AddField::Password => "Password",
                AddField::Group => "Group",
                AddField::ExpiresAt => "Expiry",
                AddField::Tags => "Tags",
                AddField::Note => "Note",
            },
            Lang::Zh => match field {
                AddField::Name => "名称",
                AddField::Host => "主机",
                AddField::Protocol => "协议",
                AddField::Port => "端口",
                AddField::User => "用户",
                AddField::Password => "密码",
                AddField::Group => "分组",
                AddField::ExpiresAt => "过期日期",
                AddField::Tags => "标签",
                AddField::Note => "备注",
            },
        }
    }

    fn add_field_hint(self, field: AddField, editing: bool) -> &'static str {
        match self {
            Lang::En => match field {
                AddField::Name => "Required, e.g. Windows Jumpbox",
                AddField::Host => "Required, e.g. 10.0.0.10",
                AddField::Protocol => "rdp or ssh, press Space to toggle",
                AddField::Port => "Optional, e.g. 3389 or 22",
                AddField::User => "Optional, SSH builds user@host",
                AddField::Password if editing => "Leave blank to keep current",
                AddField::Password => "Optional, RDP saves via cmdkey",
                AddField::Group => "Optional, used for filtering",
                AddField::ExpiresAt => "Optional, format YYYY-MM-DD",
                AddField::Tags => "Optional, comma-separated",
                AddField::Note => "Optional, shown in details panel",
            },
            Lang::Zh => match field {
                AddField::Name => "必填，例如 Windows Jumpbox",
                AddField::Host => "必填，例如 10.0.0.10",
                AddField::Protocol => "rdp 或 ssh，按空格切换",
                AddField::Port => "可选，例如 3389 或 22",
                AddField::User => "可选，SSH 会生成 user@host",
                AddField::Password if editing => "留空表示不修改",
                AddField::Password => "可选，仅 RDP 会保存到 cmdkey",
                AddField::Group => "可选，用于筛选",
                AddField::ExpiresAt => "可选，格式 YYYY-MM-DD",
                AddField::Tags => "可选，用英文逗号分隔",
                AddField::Note => "可选，显示在详情面板",
            },
        }
    }

    fn password_unchanged_label(self) -> &'static str {
        match self {
            Lang::En => "(unchanged)",
            Lang::Zh => "（未修改）",
        }
    }

    // -- delete confirm ---------------------------------------------------

    fn delete_confirm_title(self) -> &'static str {
        match self {
            Lang::En => "Delete Server",
            Lang::Zh => "删除服务器",
        }
    }

    fn delete_confirm_block_title(self) -> &'static str {
        match self {
            Lang::En => " Confirm Delete ",
            Lang::Zh => " 确认删除 ",
        }
    }

    fn delete_confirm_prompt(self, name: &str) -> String {
        match self {
            Lang::En => format!("Are you sure you want to delete \"{name}\"?"),
            Lang::Zh => format!("确定要删除「{name}」吗？"),
        }
    }

    fn delete_confirm_yes_label(self) -> &'static str {
        match self {
            Lang::En => " Delete ",
            Lang::Zh => " 删除   ",
        }
    }

    fn delete_confirm_no_label(self) -> &'static str {
        match self {
            Lang::En => " Cancel",
            Lang::Zh => " 取消",
        }
    }

    // -- header -----------------------------------------------------------

    fn header_info(
        self,
        shown: usize,
        rdp: usize,
        ssh: usize,
        group_label: &str,
    ) -> String {
        match self {
            Lang::En => format!("{shown} shown · {rdp} RDP · {ssh} SSH · group:{group_label}"),
            Lang::Zh => format!("显示 {shown} · RDP {rdp} · SSH {ssh} · 分组：{group_label}"),
        }
    }

    // -- sample config notes ----------------------------------------------

    fn sample_note_rdp(self) -> &'static str {
        match self {
            Lang::En => "Example RDP host. Replace it with your real server.",
            Lang::Zh => "示例 RDP 主机，请替换为你的真实服务器。",
        }
    }

    fn sample_note_ssh(self) -> &'static str {
        match self {
            Lang::En => "Example SSH host.",
            Lang::Zh => "示例 SSH 主机。",
        }
    }

    // -- message_style classifier -----------------------------------------

    fn is_success_msg(self, msg: &str) -> bool {
        match self {
            Lang::En => {
                msg.starts_with("Started")
                    || msg.starts_with("SSH session ended")
                    || msg.starts_with("Loaded")
                    || msg.starts_with("Copied")
                    || msg.contains("Reloaded")
                    || msg.starts_with("Ready.")
                    || msg.starts_with("Group filter:")
                    || msg.starts_with("Added")
                    || msg.starts_with("Updated")
                    || msg.starts_with("Deleted")
            }
            Lang::Zh => {
                msg.starts_with("已启动")
                    || msg.starts_with("已加载")
                    || msg.starts_with("已复制")
                    || msg.contains("已重新加载")
                    || msg.starts_with("已就绪")
                    || msg.starts_with("分组筛选")
                    || msg.ends_with("的 SSH 会话已结束")
                    || msg.starts_with("已添加")
                    || msg.starts_with("已更新")
                    || msg.starts_with("已删除")
                    || msg.starts_with("已在新窗口启动")
            }
        }
    }

    fn is_error_msg(self, msg: &str) -> bool {
        msg == self.err_name_required()
            || msg == self.err_host_required()
            || msg == self.err_expires_whitespace()
            || msg == self.err_invalid_protocol()
            || msg == self.err_port_range()
            || match self {
                Lang::En => {
                    msg.starts_with("Launch failed")
                        || msg.starts_with("Reload failed")
                        || msg.starts_with("Copy failed")
                        || msg.starts_with("could not")
                        || msg.starts_with("Delete failed")
                        || msg.starts_with("Failed to save")
                        || msg.starts_with("Failed to save changes")
                        || msg.starts_with("Could not save recent")
                        || msg.starts_with("Server name already exists")
                }
                Lang::Zh => {
                    msg.starts_with("启动失败")
                        || msg.starts_with("重新加载失败")
                        || msg.starts_with("复制失败")
                        || msg.starts_with("无法")
                        || msg.starts_with("删除失败")
                        || msg.starts_with("保存新增服务器失败")
                        || msg.starts_with("保存修改失败")
                        || msg.starts_with("无法保存最近连接列表")
                        || msg.starts_with("服务器名称已存在")
                }
            }
    }
}

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Config {
    #[serde(default)]
    defaults: Defaults,
    #[serde(default)]
    servers: Vec<Server>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Defaults {
    #[serde(default = "default_lang")]
    lang: Lang,
    #[serde(default = "default_rdp_command")]
    rdp_command: String,
    #[serde(default = "default_ssh_command")]
    ssh_command: String,
    #[serde(default)]
    rdp_extra_args: Vec<String>,
    #[serde(default)]
    ssh_extra_args: Vec<String>,
    /// When true, SSH sessions open in a new terminal window instead of taking over this one.
    #[serde(default = "default_ssh_new_window")]
    ssh_new_window: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Server {
    name: String,
    host: String,
    #[serde(default)]
    group: String,
    #[serde(default)]
    protocol: Protocol,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    private_key: Option<String>,
    #[serde(default)]
    private_key_path: Option<PathBuf>,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    rdp_file: Option<PathBuf>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Protocol {
    #[default]
    Rdp,
    Ssh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Browse,
    Search,
    Help,
    Add,
    DeleteConfirm,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentStore {
    #[serde(default)]
    by_config: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddField {
    Name,
    Host,
    Protocol,
    Port,
    User,
    Password,
    Group,
    ExpiresAt,
    Tags,
    Note,
}

const ADD_FIELDS: &[AddField] = &[
    AddField::Name,
    AddField::Host,
    AddField::Protocol,
    AddField::Port,
    AddField::User,
    AddField::Password,
    AddField::Group,
    AddField::ExpiresAt,
    AddField::Tags,
    AddField::Note,
];

#[derive(Debug, Clone)]
struct AddForm {
    active: usize,
    name: String,
    host: String,
    protocol: String,
    port: String,
    user: String,
    password: String,
    group: String,
    expires_at: String,
    tags: String,
    note: String,
}

#[derive(Debug)]
struct App {
    config: Config,
    config_path: PathBuf,
    config_key: String,
    filtered: Vec<usize>,
    selected: usize,
    search: String,
    group_filter: Option<String>,
    groups: Vec<String>,
    recent_names: Vec<String>,
    recent_set: HashSet<String>,
    recent_store_path: PathBuf,
    today_iso: String,
    filtered_rdp: usize,
    filtered_ssh: usize,
    mode: Mode,
    add_form: AddForm,
    editing_index: Option<usize>,
    pending_delete_index: Option<usize>,
    message: String,
    ignore_enter_until: Instant,
    should_quit: bool,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn default_lang() -> Lang {
    Lang::En
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            lang: default_lang(),
            rdp_command: default_rdp_command(),
            ssh_command: default_ssh_command(),
            rdp_extra_args: Vec::new(),
            ssh_extra_args: Vec::new(),
            ssh_new_window: default_ssh_new_window(),
        }
    }
}

impl Default for AddForm {
    fn default() -> Self {
        Self {
            active: 0,
            name: String::new(),
            host: String::new(),
            protocol: "rdp".to_string(),
            port: String::new(),
            user: String::new(),
            password: String::new(),
            group: String::new(),
            expires_at: String::new(),
            tags: String::new(),
            note: String::new(),
        }
    }
}

impl AddForm {
    fn from_server(server: &Server) -> Self {
        Self {
            active: 0,
            name: server.name.clone(),
            host: server.host.clone(),
            protocol: match server.protocol {
                Protocol::Rdp => "rdp".to_string(),
                Protocol::Ssh => "ssh".to_string(),
            },
            port: server.port.map(|p| p.to_string()).unwrap_or_default(),
            user: server.user.clone().unwrap_or_default(),
            password: String::new(),
            group: server.group.clone(),
            expires_at: server.expires_at.clone().unwrap_or_default(),
            tags: server.tags.join(", "),
            note: server.note.clone().unwrap_or_default(),
        }
    }

    fn active_field(&self) -> AddField {
        ADD_FIELDS[self.active]
    }

    fn value(&self, field: AddField) -> &str {
        match field {
            AddField::Name => &self.name,
            AddField::Host => &self.host,
            AddField::Protocol => &self.protocol,
            AddField::Port => &self.port,
            AddField::User => &self.user,
            AddField::Password => &self.password,
            AddField::Group => &self.group,
            AddField::ExpiresAt => &self.expires_at,
            AddField::Tags => &self.tags,
            AddField::Note => &self.note,
        }
    }

    fn value_mut(&mut self, field: AddField) -> &mut String {
        match field {
            AddField::Name => &mut self.name,
            AddField::Host => &mut self.host,
            AddField::Protocol => &mut self.protocol,
            AddField::Port => &mut self.port,
            AddField::User => &mut self.user,
            AddField::Password => &mut self.password,
            AddField::Group => &mut self.group,
            AddField::ExpiresAt => &mut self.expires_at,
            AddField::Tags => &mut self.tags,
            AddField::Note => &mut self.note,
        }
    }

    fn move_next(&mut self) {
        self.active = (self.active + 1).min(ADD_FIELDS.len() - 1);
    }

    fn move_previous(&mut self) {
        self.active = self.active.saturating_sub(1);
    }

    fn is_last_field(&self) -> bool {
        self.active + 1 == ADD_FIELDS.len()
    }

    fn toggle_protocol(&mut self) {
        self.protocol = match parse_protocol(&self.protocol) {
            Some(Protocol::Rdp) => "ssh".to_string(),
            Some(Protocol::Ssh) => "rdp".to_string(),
            None => "rdp".to_string(),
        };
    }

    fn to_server(&self, lang: Lang, existing: Option<&Server>) -> std::result::Result<Server, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(lang.err_name_required().to_string());
        }
        let host = self.host.trim();
        if host.is_empty() {
            return Err(lang.err_host_required().to_string());
        }
        if self.expires_at.trim().chars().any(|ch| ch.is_whitespace()) {
            return Err(lang.err_expires_whitespace().to_string());
        }

        let protocol = parse_protocol(&self.protocol)
            .ok_or_else(|| lang.err_invalid_protocol().to_string())?;
        let port = parse_optional_port(&self.port, lang)?;
        let tags = split_tags(&self.tags);
        let password = optional_string(&self.password)
            .or_else(|| existing.and_then(|server| server.password.clone()));

        Ok(Server {
            name: name.to_string(),
            host: host.to_string(),
            group: self.group.trim().to_string(),
            protocol,
            port,
            user: optional_string(&self.user),
            password,
            private_key: existing.and_then(|server| server.private_key.clone()),
            private_key_path: existing.and_then(|server| server.private_key_path.clone()),
            domain: existing.and_then(|server| server.domain.clone()),
            expires_at: optional_string(&self.expires_at),
            note: optional_string(&self.note),
            rdp_file: existing.and_then(|server| server.rdp_file.clone()),
            tags,
        })
    }
}

fn default_ssh_new_window() -> bool {
    cfg!(windows)
}

fn default_rdp_command() -> String {
    "mstsc".to_string()
}

fn default_ssh_command() -> String {
    "ssh".to_string()
}

// ---------------------------------------------------------------------------
// main / event loop
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let config_path = resolve_config_path();
    ensure_sample_config(&config_path)?;
    let config = load_config(&config_path)?;
    let mut app = App::new(config, config_path);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    drain_pending_events()?;

    let result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| draw(frame, app))?;

        match event::read()? {
            Event::Key(key) => {
                handle_key(terminal, app, key)?;
                if is_navigation_key(&key) {
                    while event::poll(Duration::from_millis(0))? {
                        if let Event::Key(next) = event::read()? {
                            handle_key(terminal, app, next)?;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn is_navigation_key(key: &KeyEvent) -> bool {
    if key.kind == KeyEventKind::Release {
        return false;
    }
    matches!(
        key.code,
        KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Char('j')
            | KeyCode::Char('k')
    )
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

impl App {
    fn new(config: Config, config_path: PathBuf) -> Self {
        let lang = config.defaults.lang;
        let config_key = config_path.to_string_lossy().into_owned();
        let recent_store_path = recent_store_path();
        let recent_names = load_recent_names(&recent_store_path, &config_key, lang).unwrap_or_default();
        let recent_set: HashSet<String> = recent_names.iter().cloned().collect();
        let groups = collect_groups(&config.servers, lang);
        let today_iso = today_iso_date().unwrap_or_default();
        let mut app = Self {
            config,
            config_path,
            config_key,
            filtered: Vec::new(),
            selected: 0,
            search: String::new(),
            group_filter: None,
            groups,
            recent_names,
            recent_set,
            recent_store_path,
            today_iso,
            filtered_rdp: 0,
            filtered_ssh: 0,
            mode: Mode::Browse,
            add_form: AddForm::default(),
            editing_index: None,
            pending_delete_index: None,
            message: String::new(),
            ignore_enter_until: Instant::now() + Duration::from_millis(700),
            should_quit: false,
        };
        app.refresh_filter();
        app.message = lang.loaded_servers(app.config.servers.len());
        app
    }

    fn lang(&self) -> Lang {
        self.config.defaults.lang
    }

    fn is_editing(&self) -> bool {
        self.editing_index.is_some()
    }

    fn editing_server(&self) -> Option<&Server> {
        self.editing_index
            .and_then(|idx| self.config.servers.get(idx))
    }

    fn rebuild_groups(&mut self) {
        self.groups = collect_groups(&self.config.servers, self.lang());
        if let Some(filter) = &self.group_filter {
            if !self.groups.iter().any(|group| group == filter) {
                self.group_filter = None;
            }
        }
    }

    fn cycle_group_filter(&mut self) {
        let lang = self.lang();
        self.group_filter = match &self.group_filter {
            None => self.groups.first().cloned(),
            Some(current) => {
                let pos = self.groups.iter().position(|group| group == current);
                match pos {
                    Some(index) if index + 1 < self.groups.len() => {
                        Some(self.groups[index + 1].clone())
                    }
                    _ => None,
                }
            }
        };
        self.refresh_filter();
        self.message = match &self.group_filter {
            Some(group) => lang.group_filter_msg(group),
            None => lang.group_filter_all().to_string(),
        };
    }

    fn group_filter_label(&self) -> String {
        match &self.group_filter {
            Some(group) => group.clone(),
            None => self.lang().group_filter_label_all().to_string(),
        }
    }

    fn record_recent(&mut self, server_name: &str) {
        let lang = self.lang();
        self.recent_names.retain(|name| name != server_name);
        self.recent_names.insert(0, server_name.to_string());
        self.recent_names.truncate(MAX_RECENT);
        self.recent_set = self.recent_names.iter().cloned().collect();
        if let Err(err) = save_recent_names(
            &self.recent_store_path,
            &self.config_key,
            &self.recent_names,
            lang,
        ) {
            self.message = lang.recent_save_failed(&err);
        }
        self.refresh_filter();
    }

    fn is_recent(&self, server: &Server) -> bool {
        self.recent_set.contains(&server.name)
    }

    fn refresh_filter(&mut self) {
        let needle = self.search.to_lowercase();
        let mut matching: Vec<usize> = self
            .config
            .servers
            .iter()
            .enumerate()
            .filter_map(|(idx, server)| {
                if !matches_group_filter(server, self.group_filter.as_deref(), self.lang()) {
                    return None;
                }
                server_matches_search(server, &needle).then_some(idx)
            })
            .collect();

        let recent_rank: HashMap<&str, usize> = self
            .recent_names
            .iter()
            .enumerate()
            .map(|(rank, name)| (name.as_str(), rank))
            .collect();
        matching.sort_by(|left, right| {
            let left_name = self.config.servers[*left].name.as_str();
            let right_name = self.config.servers[*right].name.as_str();
            let left_rank = recent_rank.get(left_name).copied();
            let right_rank = recent_rank.get(right_name).copied();
            match (left_rank, right_rank) {
                (Some(l), Some(r)) => l.cmp(&r),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left.cmp(right),
            }
        });

        self.filtered = matching;
        self.filtered_rdp = 0;
        self.filtered_ssh = 0;
        for idx in &self.filtered {
            match self.config.servers[*idx].protocol {
                Protocol::Rdp => self.filtered_rdp += 1,
                Protocol::Ssh => self.filtered_ssh += 1,
            }
        }

        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    fn selected_server(&self) -> Option<&Server> {
        self.filtered
            .get(self.selected)
            .and_then(|idx| self.config.servers.get(*idx))
    }

    fn move_down(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1).min(self.filtered.len() - 1);
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn page_down(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 10).min(self.filtered.len() - 1);
        }
    }

    fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(10);
    }

    fn reload(&mut self) {
        let lang = self.lang();
        match load_config(&self.config_path) {
            Ok(config) => {
                self.config = config;
                self.rebuild_groups();
                self.refresh_filter();
                self.message = self.lang().reloaded(&self.config_path);
            }
            Err(err) => self.message = lang.reload_failed(&err),
        }
    }

    fn copy_selected_command(&mut self) {
        let lang = self.lang();
        let Some(server) = self.selected_server().cloned() else {
            self.message = lang.no_server_selected().to_string();
            return;
        };
        let command = connection_string(&server, &self.config.defaults);
        match copy_to_clipboard(&command, lang) {
            Ok(()) => self.message = lang.copied(&command),
            Err(err) => self.message = lang.copy_failed(&err),
        }
    }

    fn start_add_server(&mut self) {
        let lang = self.lang();
        self.editing_index = None;
        self.add_form = AddForm::default();
        self.mode = Mode::Add;
        self.message = lang.add_start().to_string();
    }

    fn start_edit_server(&mut self) {
        let lang = self.lang();
        let Some(idx) = self.filtered.get(self.selected).copied() else {
            self.message = lang.no_server_selected().to_string();
            return;
        };
        let Some(server) = self.config.servers.get(idx).cloned() else {
            self.message = lang.no_server_selected().to_string();
            return;
        };
        self.editing_index = Some(idx);
        self.add_form = AddForm::from_server(&server);
        self.mode = Mode::Add;
        self.message = lang.edit_start().to_string();
    }

    fn save_add_form(&mut self) {
        let lang = self.lang();
        let editing_index = self.editing_index;
        let existing = editing_index.and_then(|idx| self.config.servers.get(idx));
        match self.add_form.to_server(lang, existing) {
            Ok(server) => {
                let server_name = server.name.clone();
                if self.config.servers.iter().enumerate().any(|(idx, existing_server)| {
                    existing_server.name == server_name && Some(idx) != editing_index
                }) {
                    self.message = lang.add_name_exists(&server_name);
                    return;
                }

                match editing_index {
                    Some(idx) => self.save_edited_server(idx, server, lang),
                    None => self.save_new_server(server, lang),
                }
            }
            Err(err) => self.message = err,
        }
    }

    fn save_new_server(&mut self, server: Server, lang: Lang) {
        let server_name = server.name.clone();
        let new_idx = self.config.servers.len();
        self.config.servers.push(server);
        self.rebuild_groups();
        self.search.clear();
        self.group_filter = None;
        self.refresh_filter();
        self.selected = self
            .filtered
            .iter()
            .position(|visible| *visible == new_idx)
            .unwrap_or_else(|| self.filtered.len().saturating_sub(1));

        match save_config(&self.config_path, &self.config, lang) {
            Ok(()) => {
                self.mode = Mode::Browse;
                self.message = lang.add_saved(&server_name);
            }
            Err(err) => {
                let _ = self.config.servers.pop();
                self.rebuild_groups();
                self.refresh_filter();
                self.message = lang.add_save_failed(&err);
            }
        }
    }

    fn save_edited_server(&mut self, idx: usize, server: Server, lang: Lang) {
        let server_name = server.name.clone();
        let old_name = self.config.servers[idx].name.clone();
        let previous = self.config.servers[idx].clone();
        self.config.servers[idx] = server;
        self.rebuild_groups();
        self.refresh_filter();
        self.selected = self
            .filtered
            .iter()
            .position(|visible| *visible == idx)
            .unwrap_or(self.selected);

        match save_config(&self.config_path, &self.config, lang) {
            Ok(()) => {
                if old_name != server_name {
                    for name in &mut self.recent_names {
                        if name == &old_name {
                            *name = server_name.clone();
                        }
                    }
                    self.recent_set = self.recent_names.iter().cloned().collect();
                    if let Err(err) = save_recent_names(
                        &self.recent_store_path,
                        &self.config_key,
                        &self.recent_names,
                        lang,
                    ) {
                        self.message = lang.recent_save_failed(&err);
                    } else {
                        self.message = lang.edit_saved(&server_name);
                    }
                } else {
                    self.message = lang.edit_saved(&server_name);
                }
                self.editing_index = None;
                self.mode = Mode::Browse;
            }
            Err(err) => {
                self.config.servers[idx] = previous;
                self.rebuild_groups();
                self.refresh_filter();
                self.message = lang.edit_save_failed(&err);
            }
        }
    }

    fn request_delete_selected(&mut self) {
        let lang = self.lang();
        let Some(idx) = self.filtered.get(self.selected).copied() else {
            self.message = lang.no_server_selected().to_string();
            return;
        };
        let Some(server) = self.config.servers.get(idx) else {
            self.message = lang.no_server_selected().to_string();
            return;
        };
        self.pending_delete_index = Some(idx);
        self.mode = Mode::DeleteConfirm;
        self.message = lang.delete_confirm_msg(&server.name);
    }

    fn cancel_delete(&mut self) {
        let lang = self.lang();
        self.pending_delete_index = None;
        self.mode = Mode::Browse;
        self.message = lang.delete_cancelled().to_string();
    }

    fn confirm_delete(&mut self) {
        let lang = self.lang();
        let Some(idx) = self.pending_delete_index.take() else {
            self.mode = Mode::Browse;
            self.message = lang.delete_pending_none().to_string();
            return;
        };
        if idx >= self.config.servers.len() {
            self.mode = Mode::Browse;
            self.message = lang.delete_missing().to_string();
            return;
        }

        let removed = self.config.servers.remove(idx);
        let old_recent_names = self.recent_names.clone();
        self.recent_names.retain(|name| name != &removed.name);
        self.recent_set = self.recent_names.iter().cloned().collect();
        self.rebuild_groups();
        self.refresh_filter();
        // Keep selection within bounds after removal
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }

        match save_config(&self.config_path, &self.config, lang) {
            Ok(()) => {
                if let Err(err) = save_recent_names(
                    &self.recent_store_path,
                    &self.config_key,
                    &self.recent_names,
                    lang,
                ) {
                    self.message = lang.delete_recent_failed(&removed.name, &err);
                } else {
                    self.message = lang.delete_saved(&removed.name);
                }
                self.mode = Mode::Browse;
            }
            Err(err) => {
                self.config.servers.insert(idx, removed);
                self.recent_names = old_recent_names;
                self.recent_set = self.recent_names.iter().cloned().collect();
                self.rebuild_groups();
                self.refresh_filter();
                self.mode = Mode::Browse;
                self.message = lang.delete_failed(&err);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol helpers
// ---------------------------------------------------------------------------

impl Protocol {
    fn label(self) -> &'static str {
        match self {
            Protocol::Rdp => "RDP",
            Protocol::Ssh => "SSH",
        }
    }

    fn color(self) -> Color {
        match self {
            Protocol::Rdp => Color::Cyan,
            Protocol::Ssh => Color::Green,
        }
    }
}

fn parse_protocol(value: &str) -> Option<Protocol> {
    match value.trim().to_lowercase().as_str() {
        "rdp" => Some(Protocol::Rdp),
        "ssh" => Some(Protocol::Ssh),
        _ => None,
    }
}

fn parse_optional_port(value: &str, lang: Lang) -> std::result::Result<Option<u16>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<u16>()
        .map(Some)
        .map_err(|_| lang.err_port_range().to_string())
}

fn optional_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn split_tags(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToString::to_string)
        .collect()
}

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

fn handle_key(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<()> {
    if key.kind == KeyEventKind::Release {
        return Ok(());
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return Ok(());
    }

    match app.mode {
        Mode::Search => handle_search_key(app, key),
        Mode::Help => {
            app.mode = Mode::Browse;
            Ok(())
        }
        Mode::Add => handle_add_key(app, key),
        Mode::DeleteConfirm => handle_delete_confirm_key(app, key),
        Mode::Browse => handle_browse_key(terminal, app, key),
    }
}

fn handle_browse_key(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<()> {
    let lang = app.lang();
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('h') | KeyCode::Char('?') => app.mode = Mode::Help,
        KeyCode::Char('/') => {
            app.mode = Mode::Search;
            app.message = lang.search_prompt().to_string();
        }
        KeyCode::Char('r') => app.reload(),
        KeyCode::Char('g') => app.cycle_group_filter(),
        KeyCode::Char('c') => app.copy_selected_command(),
        KeyCode::Char('a') => app.start_add_server(),
        KeyCode::Char('i') => app.start_edit_server(),
        KeyCode::Char('d') | KeyCode::Delete => app.request_delete_selected(),
        KeyCode::Char(c @ '1'..='9') => launch_at(terminal, app, (c as u8 - b'1') as usize)?,
        KeyCode::Enter if Instant::now() >= app.ignore_enter_until => {
            launch_selected(terminal, app)?
        }
        KeyCode::Enter => {
            app.message = lang.ready_enter().to_string();
        }
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::PageDown => app.page_down(),
        KeyCode::PageUp => app.page_up(),
        KeyCode::Home => app.selected = 0,
        KeyCode::End => app.selected = app.filtered.len().saturating_sub(1),
        _ => {}
    }

    Ok(())
}

fn handle_add_key(app: &mut App, key: KeyEvent) -> Result<()> {
    let lang = app.lang();
    match key.code {
        KeyCode::Esc => {
            let editing = app.is_editing();
            app.editing_index = None;
            app.mode = Mode::Browse;
            app.message = if editing {
                lang.edit_cancelled().to_string()
            } else {
                lang.add_cancelled().to_string()
            };
        }
        KeyCode::Enter => {
            if app.add_form.is_last_field() {
                app.save_add_form();
            } else {
                app.add_form.move_next();
            }
        }
        KeyCode::Tab | KeyCode::Down => app.add_form.move_next(),
        KeyCode::BackTab | KeyCode::Up => app.add_form.move_previous(),
        KeyCode::Backspace => {
            if app.add_form.active_field() == AddField::Protocol {
                app.add_form.protocol = "rdp".to_string();
            } else {
                let field = app.add_form.active_field();
                app.add_form.value_mut(field).pop();
            }
        }
        KeyCode::Char(' ') if app.add_form.active_field() == AddField::Protocol => {
            app.add_form.toggle_protocol();
        }
        KeyCode::Char(ch) => {
            if app.add_form.active_field() != AddField::Protocol {
                let field = app.add_form.active_field();
                app.add_form.value_mut(field).push(ch);
            }
        }
        _ => {}
    }

    Ok(())
}

fn handle_delete_confirm_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => app.confirm_delete(),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.cancel_delete(),
        _ => {}
    }

    Ok(())
}

fn launch_at(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    visible_index: usize,
) -> Result<()> {
    let lang = app.lang();
    if visible_index >= app.filtered.len() {
        app.message = lang.shortcut_no_server(visible_index + 1);
        return Ok(());
    }
    app.selected = visible_index;
    launch_selected(terminal, app)
}

fn launch_selected(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let lang = app.lang();
    let Some(server) = app.selected_server().cloned() else {
        app.message = lang.no_server_selected().to_string();
        return Ok(());
    };

    match launch_server(terminal, &server, &app.config.defaults) {
        Ok(summary) => {
            app.record_recent(&server.name);
            app.message = summary;
        }
        Err(err) => app.message = lang.launch_failed(&err),
    }

    Ok(())
}

fn handle_search_key(app: &mut App, key: KeyEvent) -> Result<()> {
    let lang = app.lang();
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Browse;
            app.message = lang.search_kept().to_string();
        }
        KeyCode::Enter => app.mode = Mode::Browse,
        KeyCode::Backspace => {
            app.search.pop();
            app.refresh_filter();
        }
        KeyCode::Char(ch) => {
            app.search.push(ch);
            app.refresh_filter();
        }
        KeyCode::Down => app.move_down(),
        KeyCode::Up => app.move_up(),
        _ => {}
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, layout[0]);
    draw_body(frame, app, layout[1]);
    draw_footer(frame, app, layout[2]);

    let lang = app.lang();
    match app.mode {
        Mode::Help => draw_help(frame, lang, centered_rect(72, 68, area)),
        Mode::Add => draw_add_form(frame, app, centered_rect(76, 78, area)),
        Mode::DeleteConfirm => draw_delete_confirm(frame, app, centered_rect(58, 24, area)),
        _ => {}
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let lang = app.lang();
    let title = Line::from(vec![
        Span::styled(
            format!(" {APP_NAME} "),
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw(format!(" {}", lang.app_title())),
    ]);
    let config_label = truncate_config_path(&app.config_path, 28);
    let right = format!(
        "{}  |  {}",
        lang.header_info(
            app.filtered.len(),
            app.filtered_rdp,
            app.filtered_ssh,
            &app.group_filter_label(),
        ),
        config_label
    );

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(20),
            Constraint::Length((UnicodeWidthStr::width(right.as_str()) + 2) as u16),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(title).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        ),
        chunks[1],
    );
}

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(area);

    draw_server_list(frame, app, chunks[0]);
    draw_side_panel(frame, app, chunks[1]);
}

fn draw_server_list(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let lang = app.lang();

    let header = Row::new(vec![
        Cell::from("#").style(column_header_style()),
        Cell::from(lang.col_type()).style(column_header_style()),
        Cell::from(lang.col_name()).style(column_header_style()),
        Cell::from(lang.col_address()).style(column_header_style()),
        Cell::from(lang.col_expires()).style(column_header_style()),
        Cell::from(lang.col_group()).style(column_header_style()),
    ])
    .height(1)
    .bottom_margin(1);

    let rows: Vec<Row> = app
        .filtered
        .iter()
        .enumerate()
        .filter_map(|(visible_idx, idx)| {
            let server = app.config.servers.get(*idx)?;
            let endpoint = match server.port {
                Some(port) => format!("{}:{}", server.host, port),
                None => server.host.clone(),
            };
            let group = group_label(&server.group, lang);
            let expires = server_expires_at(server).unwrap_or_else(|| "-".to_string());
            let shortcut = if visible_idx < 9 {
                format!("{}", visible_idx + 1)
            } else if app.is_recent(server) {
                "★".to_string()
            } else {
                " ".to_string()
            };
            let shortcut_style = if visible_idx < 9 {
                Style::default().fg(Color::Black).bg(Color::Magenta)
            } else if app.is_recent(server) {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Some(Row::new(vec![
                Cell::from(Span::styled(format!(" {shortcut} "), shortcut_style)),
                Cell::from(Span::styled(
                    format!(" {:<4} ", server.protocol.label()),
                    Style::default()
                        .fg(Color::Black)
                        .bg(server.protocol.color()),
                )),
                Cell::from(Span::styled(
                    pad_visual(&server.name, 22),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(
                    pad_visual(&endpoint, 24),
                    Style::default().fg(Color::Gray),
                )),
                Cell::from(Span::styled(
                    pad_visual(&expires, 12),
                    Style::default().fg(expiry_color(&expires, &app.today_iso)),
                )),
                Cell::from(Span::styled(
                    pad_visual(&group, 12),
                    Style::default().fg(Color::Yellow),
                )),
            ]))
        })
        .collect();

    let widths = [
        Constraint::Length(3),
        Constraint::Length(6),
        Constraint::Length(24),
        Constraint::Min(18),
        Constraint::Length(12),
        Constraint::Length(14),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(lang.servers_title(app.filtered.len(), app.config.servers.len()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .row_highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = TableState::default();
    if !app.filtered.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(table, area, &mut state);

    let content_height = app.filtered.len().saturating_sub(1);
    if content_height > 0 {
        let mut scrollbar = ScrollbarState::new(content_height).position(app.selected);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar,
        );
    }
}

fn draw_side_panel(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let lang = app.lang();
    let selected = app.selected_server();
    let lines = if let Some(server) = selected {
        vec![
            Line::from(vec![
                Span::styled(
                    &server.name,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    format!(" {:<4} ", server.protocol.label()),
                    Style::default()
                        .fg(Color::Black)
                        .bg(server.protocol.color()),
                ),
            ]),
            Line::raw(""),
            Line::raw(format!("{}{}", lang.detail_host(), server.host)),
            Line::raw(format!(
                "{}{}",
                lang.detail_port(),
                server
                    .port
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "-".to_string())
            )),
            Line::raw(format!(
                "{}{}",
                lang.detail_user(),
                server.user.as_deref().unwrap_or("-")
            )),
            Line::raw(format!(
                "{}{}",
                lang.detail_expires(),
                server_expires_at(server).unwrap_or_else(|| "-".to_string())
            )),
            Line::raw(format!(
                "{}{}",
                lang.detail_password(),
                if has_text(server.password.as_deref()) {
                    lang.detail_saved()
                } else {
                    "-"
                }
            )),
            Line::raw(format!(
                "{}{}",
                lang.detail_ssh_key(),
                if server.private_key_path.is_some() || has_text(server.private_key.as_deref()) {
                    lang.detail_saved()
                } else {
                    "-"
                }
            )),
            Line::raw(format!(
                "{}{}",
                lang.detail_group_label(),
                group_label(&server.group, lang)
            )),
            Line::raw(format!(
                "{}{}",
                lang.detail_recent(),
                if app.is_recent(server) {
                    lang.detail_yes()
                } else {
                    lang.detail_no()
                }
            )),
            Line::raw(format!(
                "{}{}",
                lang.detail_command(),
                truncate_visual(&connection_string(server, &app.config.defaults), 42)
            )),
            Line::raw(format!(
                "{}{}",
                lang.detail_tags(),
                if server.tags.is_empty() {
                    "-".to_string()
                } else {
                    server.tags.join(", ")
                }
            )),
            Line::raw(""),
            Line::raw(server.note.as_deref().unwrap_or(lang.detail_no_note())),
        ]
    } else {
        vec![Line::raw(lang.detail_no_match())]
    };

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(lang.detail_title())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let lang = app.lang();
    let search_style = if app.mode == Mode::Search {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::Yellow)
    };
    let search = if app.search.is_empty() {
        lang.footer_search().to_string()
    } else {
        format!("/ {}", app.search)
    };

    let group_style = if app.group_filter.is_some() {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let group = format!(
        " g:{}/{} ",
        app.group_filter_label(),
        lang.footer_group_short()
    );

    let shortcuts = Line::from(vec![
        Span::styled(
            " Enter ",
            Style::default().fg(Color::Black).bg(Color::Green),
        ),
        Span::raw(lang.footer_connect()),
        Span::styled(
            " 1-9 ",
            Style::default().fg(Color::Black).bg(Color::Magenta),
        ),
        Span::raw(lang.footer_quick()),
        Span::styled(" c ", Style::default().fg(Color::Black).bg(Color::Blue)),
        Span::raw(lang.footer_copy()),
        Span::styled(group, group_style),
        Span::styled(" h ", Style::default().fg(Color::Black).bg(Color::White)),
        Span::raw(lang.footer_help()),
        Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::Red)),
        Span::raw(lang.footer_quit()),
    ]);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(1)])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {search} "), search_style),
            Span::raw("  "),
            Span::styled(&app.message, message_style(&app.message, lang)),
        ]))
        .block(Block::default().borders(Borders::TOP)),
        chunks[0],
    );
    frame.render_widget(Paragraph::new(shortcuts), chunks[1]);
}

fn draw_help(frame: &mut Frame, lang: Lang, area: Rect) {
    frame.render_widget(Clear, area);
    let mut lines: Vec<Line> = vec![
        Line::styled(
            lang.help_title(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
    ];
    for text in lang.help_lines() {
        lines.push(Line::raw(text));
    }
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(lang.help_block_title())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

fn draw_add_form(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let lang = app.lang();
    let editing = app.is_editing();
    let existing = app.editing_server();

    let rows: Vec<Line> = ADD_FIELDS
        .iter()
        .enumerate()
        .flat_map(|(idx, field)| {
            let is_active = idx == app.add_form.active;
            let label_style = if is_active {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            let value_style = if is_active {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let cursor = if is_active { "▸" } else { " " };
            let value = display_add_field_value(&app.add_form, *field, lang, existing);
            let hint_style = if is_active && *field == AddField::Protocol {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            [
                Line::from(vec![
                    Span::styled(
                        format!("{cursor} {:<8}", lang.add_field_label(*field)),
                        label_style,
                    ),
                    Span::raw(" "),
                    Span::styled(value, value_style),
                ]),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(lang.add_field_hint(*field, editing), hint_style),
                ]),
            ]
        })
        .collect();

    let mut lines = Vec::with_capacity(rows.len() + 4);
    lines.push(Line::styled(
        if editing {
            lang.edit_form_title()
        } else {
            lang.add_form_title()
        },
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::raw(""));
    lines.extend(rows);
    lines.push(Line::raw(""));
    let footer = if app.add_form.active_field() == AddField::Protocol {
        lang.add_form_protocol_instructions()
    } else {
        lang.add_form_instructions()
    };
    lines.push(Line::styled(
        footer,
        Style::default().fg(Color::Gray),
    ));

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(if editing {
                    lang.edit_form_block_title()
                } else {
                    lang.add_form_block_title()
                })
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

fn draw_delete_confirm(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let lang = app.lang();
    let server_name = app
        .pending_delete_index
        .and_then(|idx| app.config.servers.get(idx))
        .map(|server| server.name.as_str())
        .unwrap_or("-");
    let lines = vec![
        Line::styled(
            lang.delete_confirm_title(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::raw(lang.delete_confirm_prompt(server_name)),
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                " y / Enter ",
                Style::default().fg(Color::Black).bg(Color::Red),
            ),
            Span::raw(lang.delete_confirm_yes_label()),
            Span::styled(
                " n / Esc ",
                Style::default().fg(Color::Black).bg(Color::White),
            ),
            Span::raw(lang.delete_confirm_no_label()),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(lang.delete_confirm_block_title())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Red)),
        ),
        area,
    );
}

fn display_add_field_value(
    form: &AddForm,
    field: AddField,
    lang: Lang,
    existing: Option<&Server>,
) -> String {
    let value = form.value(field);
    if field == AddField::Password {
        if !value.is_empty() {
            return "*".repeat(UnicodeWidthStr::width(value));
        }
        if existing.is_some_and(|server| has_text(server.password.as_deref())) {
            return lang.password_unchanged_label().to_string();
        }
    }
    if value.is_empty() {
        "-".to_string()
    } else {
        value.to_string()
    }
}

// ---------------------------------------------------------------------------
// Launching
// ---------------------------------------------------------------------------

fn launch_server(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    server: &Server,
    defaults: &Defaults,
) -> Result<String> {
    match server.protocol {
        Protocol::Rdp => launch_rdp(server, defaults),
        Protocol::Ssh if defaults.ssh_new_window => launch_ssh_new_window(server, defaults),
        Protocol::Ssh => launch_ssh_foreground(terminal, server, defaults),
    }
}

fn launch_rdp(server: &Server, defaults: &Defaults) -> Result<String> {
    let lang = defaults.lang;
    if has_text(server.password.as_deref()) && has_text(server.user.as_deref()) {
        save_rdp_credential(server, lang)?;
    }

    let mut command = Command::new(&defaults.rdp_command);
    if let Some(rdp_file) = &server.rdp_file {
        command.arg(rdp_file);
    } else {
        command.arg(format!(
            "/v:{}",
            endpoint_for_protocol(server, Protocol::Rdp)
        ));
        if has_admin_tag(server) {
            command.arg("/admin");
        }
    }
    command.args(&defaults.rdp_extra_args);
    command
        .spawn()
        .with_context(|| lang.err_cant_start(&defaults.rdp_command))?;

    Ok(lang.rdp_started(&server.name, has_text(server.password.as_deref())))
}

fn build_ssh_command(server: &Server, defaults: &Defaults) -> Result<Command> {
    let mut command = Command::new(&defaults.ssh_command);
    if let Some(port) = server.port {
        command.args(["-p", &port.to_string()]);
    }
    let inline_key_path = materialize_inline_private_key(server, defaults.lang)?;
    if let Some(private_key_path) = server
        .private_key_path
        .as_ref()
        .or(inline_key_path.as_ref())
    {
        command.args(["-i", private_key_path.to_string_lossy().as_ref()]);
    }
    command.args(&defaults.ssh_extra_args);
    command.arg(endpoint_for_protocol(server, Protocol::Ssh));
    Ok(command)
}

fn launch_ssh_new_window(server: &Server, defaults: &Defaults) -> Result<String> {
    let lang = defaults.lang;
    let ssh = build_ssh_command(server, defaults)?;
    let program = ssh.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = ssh
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let window_title = format!("SSH - {}", server.name);

    let spawned = if windows_terminal_available() {
        let mut command = Command::new("wt");
        command
            .arg("new-tab")
            .arg("--title")
            .arg(&window_title)
            .arg("--");
        command.arg(&program);
        command.args(&args);
        command
            .spawn()
            .with_context(|| lang.err_cant_start_wt(&defaults.ssh_command))
    } else {
        let mut command = Command::new("cmd");
        command
            .arg("/C")
            .arg("start")
            .arg(window_title)
            .arg(program);
        command.args(args);
        command
            .spawn()
            .with_context(|| lang.err_cant_start_new_window(&defaults.ssh_command))
    };

    spawned?;

    Ok(lang.ssh_new_window(&server.name))
}

fn windows_terminal_available() -> bool {
    Command::new("where")
        .arg("wt")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn launch_ssh_foreground(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    server: &Server,
    defaults: &Defaults,
) -> Result<String> {
    let lang = defaults.lang;
    let mut command = build_ssh_command(server, defaults)?;
    suspend_terminal(terminal, || {
        command
            .status()
            .with_context(|| lang.err_cant_start(&defaults.ssh_command))
    })?;

    Ok(lang.ssh_ended(&server.name))
}

fn save_rdp_credential(server: &Server, lang: Lang) -> Result<()> {
    let Some(user) = server
        .user
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(());
    };
    let Some(password) = server
        .password
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(());
    };

    let user = match server.domain.as_deref().filter(|value| !value.is_empty()) {
        Some(domain) => format!("{domain}\\{user}"),
        None => user.to_string(),
    };

    let host = server.host.trim();
    save_cmdkey_target(&format!("TERMSRV/{host}"), &user, password, lang)?;
    if let Some(port) = server.port {
        save_cmdkey_target(&format!("TERMSRV/{host}:{port}"), &user, password, lang)?;
    }

    Ok(())
}

fn save_cmdkey_target(target: &str, user: &str, password: &str, lang: Lang) -> Result<()> {
    let status = Command::new("cmdkey")
        .arg(format!("/generic:{target}"))
        .arg(format!("/user:{user}"))
        .arg(format!("/pass:{password}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| lang.err_cant_run_cmdkey(target))?;

    if !status.success() {
        anyhow::bail!(lang.err_cmdkey_failed(target));
    }

    Ok(())
}

fn materialize_inline_private_key(server: &Server, lang: Lang) -> Result<Option<PathBuf>> {
    let Some(private_key) = server
        .private_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };

    let dir = env::temp_dir().join(format!("{APP_NAME}-keys"));
    fs::create_dir_all(&dir)
        .with_context(|| lang.err_cant_create_key_dir(&dir))?;
    let path = dir.join(format!("{}.key", sanitize_file_name(&server.name)));
    fs::write(&path, private_key.replace("\\n", "\n"))
        .with_context(|| lang.err_cant_write_key(&path))?;
    Ok(Some(path))
}

fn sanitize_file_name(value: &str) -> String {
    let name: String = value
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    if name.trim().is_empty() {
        "server".to_string()
    } else {
        name
    }
}

fn has_text(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

fn server_expires_at(server: &Server) -> Option<String> {
    server
        .expires_at
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .or_else(|| extract_date_from_note(server.note.as_deref()))
}

fn extract_date_from_note(note: Option<&str>) -> Option<String> {
    let note = note?;
    for (index, _) in note.char_indices() {
        let Some(candidate) = note.get(index..index.saturating_add(10)) else {
            continue;
        };
        let chars: Vec<char> = candidate.chars().collect();
        if chars.len() == 10
            && chars[0..4].iter().all(|ch| ch.is_ascii_digit())
            && chars[4] == '-'
            && chars[5..7].iter().all(|ch| ch.is_ascii_digit())
            && chars[7] == '-'
            && chars[8..10].iter().all(|ch| ch.is_ascii_digit())
        {
            return Some(candidate.to_string());
        }
    }
    None
}

fn expiry_color(expires: &str, today: &str) -> Color {
    if expires == "-" {
        return Color::DarkGray;
    }
    if today.is_empty() {
        return Color::LightMagenta;
    }
    if expires < today {
        Color::Red
    } else if days_between_iso(today, expires) <= 30 {
        Color::Yellow
    } else {
        Color::LightMagenta
    }
}

fn column_header_style() -> Style {
    Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD)
}

fn message_style(message: &str, lang: Lang) -> Style {
    if lang.is_success_msg(message) {
        Style::default().fg(Color::Green)
    } else if lang.is_error_msg(message) {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn group_label(group: &str, lang: Lang) -> String {
    if group.is_empty() {
        lang.ungrouped_label().to_string()
    } else {
        group.to_string()
    }
}

fn has_admin_tag(server: &Server) -> bool {
    server
        .tags
        .iter()
        .any(|tag| tag.eq_ignore_ascii_case("admin"))
}

fn collect_groups(servers: &[Server], lang: Lang) -> Vec<String> {
    let ungrouped = lang.ungrouped_label().to_string();
    let mut groups: Vec<String> = servers
        .iter()
        .map(|server| group_label(&server.group, lang))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    groups.sort_by(|left, right| {
        if left == &ungrouped {
            std::cmp::Ordering::Greater
        } else if right == &ungrouped {
            std::cmp::Ordering::Less
        } else {
            left.cmp(right)
        }
    });
    groups
}

fn matches_group_filter(server: &Server, group_filter: Option<&str>, lang: Lang) -> bool {
    match group_filter {
        None => true,
        Some(filter) => group_label(&server.group, lang) == filter,
    }
}

fn server_matches_search(server: &Server, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }

    contains_case_insensitive(&server.name, needle)
        || contains_case_insensitive(&server.host, needle)
        || contains_case_insensitive(&server.group, needle)
        || contains_case_insensitive(server.protocol.label(), needle)
        || server_expires_at(server)
            .as_deref()
            .is_some_and(|expires| contains_case_insensitive(expires, needle))
        || server
            .tags
            .iter()
            .any(|tag| contains_case_insensitive(tag, needle))
}

fn contains_case_insensitive(value: &str, needle: &str) -> bool {
    value.to_lowercase().contains(needle)
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

fn recent_store_path() -> PathBuf {
    if let Ok(local) = env::var("LOCALAPPDATA") {
        return PathBuf::from(local).join(APP_NAME).join("recent.toml");
    }
    env::temp_dir().join(format!("{APP_NAME}-recent.toml"))
}

fn load_recent_names(store_path: &Path, config_key: &str, lang: Lang) -> Result<Vec<String>> {
    if !store_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(store_path)
        .with_context(|| lang.err_cant_read_recent(store_path))?;
    let store: RecentStore = toml::from_str(&content)
        .with_context(|| lang.err_cant_parse_recent(store_path))?;
    Ok(store
        .by_config
        .get(config_key)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(MAX_RECENT)
        .collect())
}

fn save_recent_names(
    store_path: &Path,
    config_key: &str,
    names: &[String],
    lang: Lang,
) -> Result<()> {
    if let Some(parent) = store_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| lang.err_cant_create_recent_dir(parent))?;
    }

    let mut store = if store_path.exists() {
        let content = fs::read_to_string(store_path)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        RecentStore::default()
    };

    store.by_config.insert(
        config_key.to_string(),
        names.iter().take(MAX_RECENT).cloned().collect(),
    );
    let content = toml::to_string_pretty(&store)?;
    fs::write(store_path, content)
        .with_context(|| lang.err_cant_write_recent(store_path))?;
    Ok(())
}

fn connection_string(server: &Server, defaults: &Defaults) -> String {
    match server.protocol {
        Protocol::Rdp => {
            if let Some(rdp_file) = &server.rdp_file {
                format!("{} {}", defaults.rdp_command, rdp_file.display())
            } else {
                let mut parts = vec![
                    defaults.rdp_command.clone(),
                    format!("/v:{}", endpoint_for_protocol(server, Protocol::Rdp)),
                ];
                if has_admin_tag(server) {
                    parts.push("/admin".to_string());
                }
                parts.extend(defaults.rdp_extra_args.clone());
                parts.join(" ")
            }
        }
        Protocol::Ssh => {
            let mut parts = vec![defaults.ssh_command.clone()];
            if let Some(port) = server.port.filter(|port| *port != 22) {
                parts.push("-p".to_string());
                parts.push(port.to_string());
            }
            if let Some(private_key_path) = &server.private_key_path {
                parts.push("-i".to_string());
                parts.push(private_key_path.to_string_lossy().into_owned());
            }
            parts.extend(defaults.ssh_extra_args.clone());
            parts.push(endpoint_for_protocol(server, Protocol::Ssh));
            parts.join(" ")
        }
    }
}

fn copy_to_clipboard(text: &str, lang: Lang) -> Result<()> {
    use std::io::Write;

    #[cfg(windows)]
    {
        let mut child = Command::new("cmd")
            .args(["/C", "clip"])
            .stdin(Stdio::piped())
            .spawn()
            .context(lang.err_cant_run_clip())?;
        child
            .stdin
            .as_mut()
            .context(lang.err_clip_stdin())?
            .write_all(text.as_bytes())
            .context(lang.err_cant_write_clip())?;
        child.wait().context(lang.err_clip_exit())?;
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        for (program, args) in [
            ("pbcopy", Vec::<&str>::new()),
            ("xclip", vec!["-selection", "clipboard"]),
            ("wl-copy", Vec::<&str>::new()),
        ] {
            if Command::new("which")
                .arg(program)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
            {
                let mut child = Command::new(program)
                    .args(&args)
                    .stdin(Stdio::piped())
                    .spawn()
                    .with_context(|| lang.err_cant_run_program(program))?;
                child
                    .stdin
                    .as_mut()
                    .context(lang.err_clipboard_stdin())?
                    .write_all(text.as_bytes())?;
                child.wait()?;
                return Ok(());
            }
        }
        anyhow::bail!(lang.err_clip_not_found());
    }
}

fn truncate_config_path(path: &Path, max_width: usize) -> String {
    let full = path.display().to_string();
    if UnicodeWidthStr::width(full.as_str()) <= max_width {
        return full;
    }
    if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
        let short = format!("…/{name}");
        if UnicodeWidthStr::width(short.as_str()) <= max_width {
            return short;
        }
    }
    truncate_visual(&full, max_width)
}

// ---------------------------------------------------------------------------
// Date utilities
// ---------------------------------------------------------------------------

fn today_iso_date() -> Option<String> {
    #[cfg(windows)]
    {
        return windows_local_iso_date().or_else(today_utc_iso_date);
    }

    #[cfg(not(windows))]
    today_utc_iso_date()
}

#[cfg(windows)]
#[repr(C)]
struct WindowsSystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLocalTime(system_time: *mut WindowsSystemTime);
}

#[cfg(windows)]
fn windows_local_iso_date() -> Option<String> {
    // SAFETY: GetLocalTime is synchronous and always fills all fields of SYSTEMTIME.
    let mut system_time = std::mem::MaybeUninit::<WindowsSystemTime>::uninit();
    unsafe {
        GetLocalTime(system_time.as_mut_ptr());
        let system_time = system_time.assume_init();
        if system_time.year == 0 || system_time.month == 0 || system_time.day == 0 {
            return None;
        }
        Some(format!(
            "{:04}-{:02}-{:02}",
            system_time.year, system_time.month, system_time.day
        ))
    }
}

fn today_utc_iso_date() -> Option<String> {
    let days_since_epoch =
        SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() / 86_400;
    let (year, month, day) = ordinal_to_date(days_since_epoch as i64);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn days_between_iso(from: &str, to: &str) -> i64 {
    let Some((fy, fm, fd)) = parse_iso_date(from) else {
        return i64::MAX;
    };
    let Some((ty, tm, td)) = parse_iso_date(to) else {
        return i64::MAX;
    };
    let from_days = date_to_ordinal(fy, fm, fd);
    let to_days = date_to_ordinal(ty, tm, td);
    to_days - from_days
}

fn parse_iso_date(value: &str) -> Option<(i32, u32, u32)> {
    let value = value.trim();
    if value.len() != 10 || value.as_bytes()[4] != b'-' || value.as_bytes()[7] != b'-' {
        return None;
    }
    let year = value[0..4].parse().ok()?;
    let month = value[5..7].parse().ok()?;
    let day = value[8..10].parse().ok()?;
    (month >= 1 && month <= 12 && day >= 1 && day <= 31).then_some((year, month, day))
}

fn date_to_ordinal(year: i32, month: u32, day: u32) -> i64 {
    let mut y = year as i64;
    let m = month as i64;
    let d = day as i64;
    if m <= 2 {
        y -= 1;
    }
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn ordinal_to_date(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_part = (5 * doy + 2) / 153;
    let day = doy - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

// ---------------------------------------------------------------------------
// Text / visual helpers
// ---------------------------------------------------------------------------

fn pad_visual(value: &str, width: usize) -> String {
    let clipped = truncate_visual(value, width);
    let padding = width.saturating_sub(UnicodeWidthStr::width(clipped.as_str()));
    format!("{clipped}{}", " ".repeat(padding))
}

fn truncate_visual(value: &str, max_width: usize) -> String {
    let mut output = String::new();
    let mut width = 0;
    for ch in value.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max_width {
            break;
        }
        output.push(ch);
        width += ch_width;
    }
    output
}

fn drain_pending_events() -> Result<()> {
    while event::poll(Duration::from_millis(0))? {
        let _ = event::read()?;
    }
    Ok(())
}

fn suspend_terminal<T>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let result = action();

    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    terminal.clear()?;

    result
}

fn endpoint_for_protocol(server: &Server, protocol: Protocol) -> String {
    match protocol {
        Protocol::Rdp => match server.port {
            Some(port) => format!("{}:{port}", server.host),
            None => server.host.clone(),
        },
        Protocol::Ssh => {
            let host = match &server.user {
                Some(user) if !user.is_empty() => format!("{user}@{}", server.host),
                _ => server.host.clone(),
            };
            host
        }
    }
}

// ---------------------------------------------------------------------------
// Config paths / load / save
// ---------------------------------------------------------------------------

fn exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

fn resolve_config_path() -> PathBuf {
    if let Some(path) = env::args().nth(1) {
        return PathBuf::from(path);
    }
    if let Ok(path) = env::var("REMHUB_CONFIG") {
        return PathBuf::from(path);
    }

    let mut candidates = Vec::new();
    if let Some(dir) = exe_dir() {
        candidates.push(dir.join(DEFAULT_CONFIG_FILE));
    }
    if let Ok(cwd) = env::current_dir() {
        let in_cwd = cwd.join(DEFAULT_CONFIG_FILE);
        if !candidates.contains(&in_cwd) {
            candidates.push(in_cwd);
        }
    }

    for path in &candidates {
        if path.exists() {
            return path.clone();
        }
    }

    default_config_path()
}

fn default_config_path() -> PathBuf {
    if let Ok(cwd) = env::current_dir() {
        if cwd.join("Cargo.toml").exists() {
            return cwd.join(DEFAULT_CONFIG_FILE);
        }
    }
    exe_dir()
        .map(|dir| dir.join(DEFAULT_CONFIG_FILE))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_FILE))
}

fn load_config(path: &Path) -> Result<Config> {
    // Load with English error messages (Lang isn't known until after parse).
    // These errors only surface during startup, before the TUI starts, so they
    // are printed to stderr — English is the universal fallback.
    let lang = Lang::En;
    let content =
        fs::read_to_string(path).with_context(|| lang.err_cant_read_config(path))?;
    let mut config: Config =
        toml::from_str(&content).with_context(|| lang.err_cant_parse_config(path))?;
    if config.servers.is_empty() {
        config.servers = sample_config(config.defaults.lang).servers;
    }
    Ok(config)
}

fn ensure_sample_config(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    let lang = Lang::En;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| lang.err_cant_create_config_dir(parent))?;
    }

    let content = toml::to_string_pretty(&sample_config(lang))?;
    fs::write(path, content)
        .with_context(|| lang.err_cant_write_sample_config(path))?;
    Ok(())
}

fn save_config(path: &Path, config: &Config, lang: Lang) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| lang.err_cant_create_config_dir(parent))?;
    }

    let content = toml::to_string_pretty(config)?;
    fs::write(path, content).with_context(|| lang.err_cant_write_config(path))?;
    Ok(())
}

fn sample_config(lang: Lang) -> Config {
    Config {
        defaults: Defaults::default(),
        servers: vec![
            Server {
                name: "Windows Jumpbox".to_string(),
                host: "10.0.0.10".to_string(),
                group: "production".to_string(),
                protocol: Protocol::Rdp,
                port: Some(3389),
                user: Some("administrator".to_string()),
                password: None,
                private_key: None,
                private_key_path: None,
                domain: None,
                expires_at: Some("2028-09-02".to_string()),
                note: Some(lang.sample_note_rdp().to_string()),
                rdp_file: None,
                tags: vec!["windows".to_string(), "rdp".to_string()],
            },
            Server {
                name: "Linux Bastion".to_string(),
                host: "10.0.0.20".to_string(),
                group: "production".to_string(),
                protocol: Protocol::Ssh,
                port: Some(22),
                user: Some("ops".to_string()),
                password: None,
                private_key: None,
                private_key_path: None,
                domain: None,
                expires_at: None,
                note: Some(lang.sample_note_ssh().to_string()),
                rdp_file: None,
                tags: vec!["linux".to_string(), "ssh".to_string()],
            },
        ],
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
