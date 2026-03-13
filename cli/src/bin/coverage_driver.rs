use std::fs;
use std::process::Command;

fn main() {
    run().unwrap_or_else(|message| {
        eprintln!("{message}");
        std::process::exit(1);
    });
}

fn run() -> Result<(), String> {
    let status = Command::new("cargo")
        .args([
            "llvm-cov",
            "--all-features",
            "--workspace",
            "--lcov",
            "--output-path",
            "lcov.info",
        ])
        .status()
        .map_err(|error| format!("failed to run cargo llvm-cov: {error}"))?;

    if !status.success() {
        return Err(format!("cargo llvm-cov exited with status {status}"));
    }

    let lcov = fs::read_to_string("lcov.info")
        .map_err(|error| format!("failed to read lcov.info: {error}"))?;
    let (covered, total) = coverage_totals(&lcov);

    if meets_line_threshold(covered, total) {
        Ok(())
    } else {
        Err(format!(
            "Line coverage below target: {covered}/{total} ({:.2}% < 99.00%)",
            coverage_percent(covered, total)
        ))
    }
}

fn meets_line_threshold(covered: u64, total: u64) -> bool {
    coverage_percent(covered, total) >= 99.0
}

fn coverage_percent(covered: u64, total: u64) -> f64 {
    if total == 0 {
        100.0
    } else {
        (covered as f64 / total as f64) * 100.0
    }
}

fn coverage_totals(lcov: &str) -> (u64, u64) {
    let mut file = String::new();
    let mut lines_found = None;
    let mut covered = 0;
    let mut total = 0;

    for line in lcov.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            file.clear();
            file.push_str(path);
            lines_found = None;
        } else if let Some(found) = line.strip_prefix("LF:") {
            lines_found = found.parse::<u64>().ok();
        } else {
            match line.strip_prefix("LH:") {
                Some(hit) if !is_excluded_from_gate(&file) => {
                    total += lines_found.unwrap_or_default();
                    covered += hit.parse::<u64>().unwrap_or_default();
                }
                _ => {}
            }
        }
    }

    (covered, total)
}

fn is_excluded_from_gate(file: &str) -> bool {
    file.ends_with("/src/main.rs") || file.ends_with("/src/bin/coverage_driver.rs")
}

#[cfg(test)]
mod tests {
    use super::coverage_totals;
    use super::meets_line_threshold;

    #[test]
    fn coverage_totals_excludes_main_entrypoint() {
        let lcov = "\
SF:/tmp/project/src/main.rs
LF:10
LH:7
end_of_record
SF:/tmp/project/src/bin/coverage_driver.rs
LF:8
LH:4
end_of_record
SF:/tmp/project/src/tui.rs
LF:12
LH:12
end_of_record
";

        assert_eq!(coverage_totals(lcov), (12, 12));
    }

    #[test]
    fn line_threshold_accepts_99_percent_or_more() {
        assert!(meets_line_threshold(99, 100));
        assert!(meets_line_threshold(100, 100));
        assert!(!meets_line_threshold(98, 100));
    }
}
