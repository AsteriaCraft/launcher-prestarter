//! The command line that starts Gravit's wrapper (ADR 0006). A pure function, so every OS and mode is a unit test.

use std::ffi::OsString;
use std::path::Path;

/// Arguments for `<jre>/bin/java(w)`: `-Dlauncher.noJavaCheck=true [-Dlauncher.waitProcess=true] -jar <jar> <args>`.
///
/// - `noJavaCheck`: the wrapper must not go looking for another Java; ours has JavaFX.
/// - `waitProcess`: only for the one retry after the launcher JVM died early (copy mode): the wrapper then inherits
///   its output into `launcher-start.log` and waits, so the crash is visible.
/// - The prestarter's own arguments are passed on (Gravit understands `--debug`), except macOS `-psn_*`
///   process-serial-number arguments that Finder may add.
pub fn launcher_args(jar: &Path, wait_process: bool, prestarter_args: &[OsString]) -> Vec<OsString> {
    let mut args = vec![OsString::from("-Dlauncher.noJavaCheck=true")];
    if wait_process {
        args.push(OsString::from("-Dlauncher.waitProcess=true"));
    }
    args.push(OsString::from("-jar"));
    args.push(jar.as_os_str().to_owned());
    args.extend(prestarter_args.iter().filter(|a| !a.to_string_lossy().starts_with("-psn_")).cloned());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn plain_start() {
        let args = launcher_args(Path::new("/opt/Asterium_linux"), false, &[]);
        assert_eq!(strings(&args), ["-Dlauncher.noJavaCheck=true", "-jar", "/opt/Asterium_linux"]);
    }

    #[test]
    fn retry_waits_for_the_launcher() {
        let args = launcher_args(Path::new("/s/launcher/Asterium.jar"), true, &[]);
        assert_eq!(
            strings(&args),
            ["-Dlauncher.noJavaCheck=true", "-Dlauncher.waitProcess=true", "-jar", "/s/launcher/Asterium.jar"]
        );
    }

    #[test]
    fn passes_arguments_but_not_psn() {
        let given = [OsString::from("--debug"), OsString::from("-psn_0_12345"), OsString::from("Ігри з пробілом")];
        let args = launcher_args(Path::new(r"C:\Ігри\Asterium.exe"), false, &given);
        assert_eq!(strings(&args)[3..], ["--debug".to_owned(), "Ігри з пробілом".to_owned()]);
        assert_eq!(strings(&args)[2], r"C:\Ігри\Asterium.exe");
    }
}
