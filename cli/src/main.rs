use calypso_cli::{BuildInfo, render_help, render_version};

fn build_info() -> BuildInfo<'static> {
    const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
    const GIT_HASH: &str = env!("CALYPSO_BUILD_GIT_HASH");
    const BUILD_TIME: &str = env!("CALYPSO_BUILD_TIME");
    const GIT_TAGS: &str = env!("CALYPSO_BUILD_GIT_TAGS");
    const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("CALYPSO_BUILD_GIT_HASH"));

    BuildInfo {
        version: if GIT_HASH.is_empty() { PKG_VERSION } else { VERSION },
        git_hash: GIT_HASH,
        build_time: BUILD_TIME,
        git_tags: GIT_TAGS,
    }
}

fn main() {
    let info = build_info();
    let arg = std::env::args().nth(1);

    match arg.as_deref() {
        Some("-v") | Some("--version") => {
            println!("{}", render_version(info));
        }
        _ => {
            println!("{}", render_help(info));
        }
    }
}
