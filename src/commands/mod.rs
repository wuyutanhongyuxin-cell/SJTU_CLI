//! 子命令实现层。每个 CLI 子命令（login / logout / status / ...）对应一个 `cmd_*` 函数。

pub mod auth_cmds;
pub mod canvas;
pub mod canvas_video;
pub mod card;
pub mod elec;
pub mod jwbmessage;
pub mod jwc;
pub mod library;
pub mod services;
pub mod shuiyuan;
