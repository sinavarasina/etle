use std::{
    env,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() {
    println!("cargo:rerun-if-env-changed=ETLE_BUILD_DATE");
    println!("cargo:rerun-if-env-changed=ETLE_RELEASE_TAG");
    println!("cargo:rerun-if-env-changed=GITHUB_ACTIONS");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_TYPE");
    println!("cargo:rerun-if-env-changed=GITHUB_REPOSITORY");
    println!("cargo:rerun-if-env-changed=GITHUB_RUN_ID");
    println!("cargo:rerun-if-changed=.git/HEAD");

    emit(
        "ETLE_GIT_COMMIT",
        git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into()),
    );
    emit(
        "ETLE_GIT_BRANCH",
        env_value("GITHUB_REF_NAME")
            .or_else(|| git(&["branch", "--show-current"]))
            .unwrap_or_else(|| "unknown".into()),
    );
    emit(
        "ETLE_GIT_TAG",
        env_value("ETLE_RELEASE_TAG")
            .or_else(github_tag)
            .or_else(|| git(&["describe", "--tags", "--exact-match", "HEAD"]))
            .unwrap_or_else(|| "unknown".into()),
    );
    emit(
        "ETLE_GIT_COMMIT_COUNT",
        git(&["rev-list", "--count", "HEAD"]).unwrap_or_else(|| "unknown".into()),
    );
    emit(
        "ETLE_GIT_COMMIT_MESSAGE",
        git(&["log", "-1", "--pretty=%s"]).unwrap_or_else(|| "unknown".into()),
    );
    emit("ETLE_GIT_DIRTY", git_dirty_state());

    emit(
        "ETLE_BUILD_DATE",
        env_value("ETLE_BUILD_DATE")
            .or_else(|| git(&["show", "-s", "--format=%cI", "HEAD"]))
            .unwrap_or_else(unix_now_string),
    );
    emit(
        "ETLE_BUILD_PROFILE",
        env::var("PROFILE").unwrap_or_else(|_| "unknown".into()),
    );
    emit(
        "ETLE_BUILD_TARGET",
        env::var("TARGET").unwrap_or_else(|_| "unknown".into()),
    );
    emit(
        "ETLE_BUILD_HOST",
        env::var("HOST").unwrap_or_else(|_| "unknown".into()),
    );
    emit(
        "ETLE_RUSTC_VERSION",
        command_one_line("rustc", &["--version"]).unwrap_or_else(|| "unknown".into()),
    );
    emit(
        "ETLE_BUILD_CI",
        env_value("GITHUB_ACTIONS").unwrap_or_else(|| "false".into()),
    );
    emit(
        "ETLE_BUILD_REPOSITORY",
        env_value("GITHUB_REPOSITORY").unwrap_or_else(|| "unknown".into()),
    );
    emit(
        "ETLE_BUILD_RUN_ID",
        env_value("GITHUB_RUN_ID").unwrap_or_else(|| "unknown".into()),
    );
}

fn emit(name: &str, value: impl AsRef<str>) {
    println!("cargo:rustc-env={name}={}", one_line(value.as_ref()));
}

fn env_value(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn github_tag() -> Option<String> {
    if env::var("GITHUB_REF_TYPE").ok().as_deref() == Some("tag") {
        env_value("GITHUB_REF_NAME")
    } else {
        None
    }
}

fn git(args: &[&str]) -> Option<String> {
    command_one_line("git", args)
}

fn command_one_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn git_dirty_state() -> String {
    let output = Command::new("git").args(["status", "--porcelain"]).output();

    match output {
        Ok(output) if output.status.success() && output.stdout.is_empty() => "clean".into(),
        Ok(output) if output.status.success() => "dirty".into(),
        _ => "unknown".into(),
    }
}

fn one_line(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

fn unix_now_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    format!("unix:{seconds}")
}
