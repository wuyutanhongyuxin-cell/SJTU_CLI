//! `--via` flag 模型 + 路径选择器（auto 模式根据本地 OAuth2 token 存在性选 weixin 或 oauth2）。

use clap::ValueEnum;

/// CLI `--via` flag 值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum CardVia {
    /// 自动：本地有有效 OAuth2 token → oauth2；否则 weixin。
    #[default]
    Auto,
    /// 强制 OAuth2 path（api.sjtu.edu.cn）。任何错误透传，不 fallback。
    Oauth2,
    /// 强制 weixin path（weixin.sjtu.edu.cn HTML scrape）。
    Weixin,
}

/// 路径选择结果。命令层据此分支调用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedVia {
    Oauth2,
    Weixin,
}

impl ResolvedVia {
    /// 输出到 `Envelope.meta.via`。
    pub fn name(&self) -> &'static str {
        match self {
            Self::Oauth2 => "oauth2",
            Self::Weixin => "weixin",
        }
    }
    /// 输出到 `Envelope.meta.source_hint`。
    pub fn source_hint(&self) -> &'static str {
        match self {
            Self::Oauth2 => "api.sjtu.edu.cn",
            Self::Weixin => "card.sjtu.edu.cn",
        }
    }
}

/// 据 flag + 本地 token 存在性选实际路径。纯函数，可单测。
pub fn select_via(flag: CardVia, has_oauth_token: bool) -> ResolvedVia {
    match flag {
        CardVia::Oauth2 => ResolvedVia::Oauth2,
        CardVia::Weixin => ResolvedVia::Weixin,
        CardVia::Auto => {
            if has_oauth_token {
                ResolvedVia::Oauth2
            } else {
                ResolvedVia::Weixin
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_no_token_picks_weixin() {
        assert_eq!(select_via(CardVia::Auto, false), ResolvedVia::Weixin);
    }

    #[test]
    fn auto_with_token_picks_oauth2() {
        assert_eq!(select_via(CardVia::Auto, true), ResolvedVia::Oauth2);
    }

    #[test]
    fn oauth2_forces_oauth2_regardless_of_token() {
        assert_eq!(select_via(CardVia::Oauth2, false), ResolvedVia::Oauth2);
        assert_eq!(select_via(CardVia::Oauth2, true), ResolvedVia::Oauth2);
    }

    #[test]
    fn weixin_forces_weixin_regardless_of_token() {
        assert_eq!(select_via(CardVia::Weixin, false), ResolvedVia::Weixin);
        assert_eq!(select_via(CardVia::Weixin, true), ResolvedVia::Weixin);
    }

    #[test]
    fn name_and_source_hint_are_distinct() {
        assert_eq!(ResolvedVia::Oauth2.name(), "oauth2");
        assert_eq!(ResolvedVia::Weixin.name(), "weixin");
        assert_eq!(ResolvedVia::Oauth2.source_hint(), "api.sjtu.edu.cn");
        assert_eq!(ResolvedVia::Weixin.source_hint(), "card.sjtu.edu.cn");
    }
}
