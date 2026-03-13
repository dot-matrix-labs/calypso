use calypso_cli::app::{run_doctor, run_status};
use calypso_cli::{BuildInfo, render_help, render_version};

fn build_info() -> BuildInfo<'static> {
    const VERSION: &str = concat!(
        env!("CARGO_PKG_VERSION"),
        "+",
        env!("CALYPSO_BUILD_GIT_HASH")
    );

    BuildInfo {
        version: VERSION,
        git_hash: env!("CALYPSO_BUILD_GIT_HASH"),
        build_time: env!("CALYPSO_BUILD_TIME"),
        git_tags: env!("CALYPSO_BUILD_GIT_TAGS"),
    }
}

fn main() {
    let info = build_info();
    let arg = std::env::args().nth(1);

    match arg.as_deref() {
        Some("-v") | Some("--version") => {
            println!("{}", render_version(info));
        }
        Some("doctor") => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            println!("{}", run_doctor(&cwd));
        }
        Some("status") => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            match run_status(&cwd) {
                Ok(output) => println!("{output}"),
                Err(error) => {
                    eprintln!("status error: {error}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!("{}", render_help(info));
        }
    }
}
