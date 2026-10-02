//! The mode without a window (ADR 0006): no WebView2 on Windows, or no display on Linux. The work still happens;
//! the player is told with a native dialog on Windows (in the log and on stderr elsewhere). With
//! `ASTERIUM_PRESTARTER_NONINTERACTIVE=1` the dialog texts go to the log instead, so CI never waits for a click.

use crate::flow::{LogReporter, Plan, Session};
use crate::i18n::{self, Lang};

use super::worker::Start;

fn announce(text: &str, noninteractive: bool) {
    log::info!("dialog (info): {text}");
    #[cfg(windows)]
    if !noninteractive {
        let text = text.to_owned();
        std::thread::spawn(move || {
            crate::platform::windows::message_box("Asterium", &text, crate::platform::windows::BoxKind::Info)
        });
    }
    #[cfg(not(windows))]
    {
        let _ = noninteractive;
        eprintln!("{text}");
    }
}

fn show_error(text: &str, noninteractive: bool) {
    log::error!("dialog (error): {text}");
    #[cfg(windows)]
    if !noninteractive {
        crate::platform::windows::message_box("Asterium", text, crate::platform::windows::BoxKind::Error);
    }
    #[cfg(not(windows))]
    {
        let _ = noninteractive;
        eprintln!("{text}");
    }
}

pub fn run(mut session: Session, start: Start) -> i32 {
    let lang: Lang = session.context().lang;
    let noninteractive = session.context().overrides.noninteractive;
    let reporter = LogReporter;
    let result = match start {
        Start::ShowError(err) => Err(err),
        Start::Work(work) => {
            if work.downloads() {
                announce(&session.preparing_text(), noninteractive);
            }
            session.run(&work, &reporter)
        }
        Start::Retry => match session.plan() {
            Plan::Launch => session.launch(&reporter),
            Plan::Window(work) => {
                if work.downloads() {
                    announce(&session.preparing_text(), noninteractive);
                }
                session.run(&work, &reporter)
            }
            Plan::Fail(err) => Err(err),
        },
    };
    match result {
        Ok(launched) => {
            log::info!("launcher started without a window ({launched:?})");
            0
        }
        Err(err) => {
            log::error!("{err}");
            let logs = session.logs_dir().display().to_string();
            let text = format!("{}\n\n{}", err.message(lang), i18n::t(lang, "headless.logs", &[("path", &logs)]));
            show_error(&text, noninteractive);
            err.exit_code()
        }
    }
}
