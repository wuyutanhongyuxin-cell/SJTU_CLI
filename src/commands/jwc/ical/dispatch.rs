//! Calendar dispatch wrapper：把 with_cas_refresh 闭包包装从 cli/jwc 层迁来，
//! cli/jwc/mod.rs 保持单行 dispatch 形式（200 行硬限合规）。

use anyhow::Result;

use crate::apps::jwc::{Client, LOGIN_URL};
use crate::auth::cas::with_cas_refresh;
use crate::cli::jwc::CalendarArgs;
use crate::output::OutputFormat;

/// `sjtu jwc calendar` 入口：包 with_cas_refresh，闭包内 from_session → cmd_calendar。
pub async fn run_calendar(args: CalendarArgs, fmt: Option<OutputFormat>) -> Result<()> {
    let a_xnm = args.xnm.clone();
    let a_xqm = args.xqm.clone();
    let a_to = args.to.clone();
    let a_no_acad = args.no_academic;
    let a_no_exams = args.no_exams;
    with_cas_refresh("jwc", LOGIN_URL, |session| {
        let xnm = a_xnm.clone();
        let xqm = a_xqm.clone();
        let to = a_to.clone();
        async move {
            let client = Client::from_session(session)?;
            super::handler::cmd_calendar(&client, xnm, xqm, to, a_no_acad, a_no_exams, fmt).await
        }
    })
    .await
}
