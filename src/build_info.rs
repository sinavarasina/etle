use crate::{
    file::descriptor::ETLE_DESCRIPTOR_VERSION,
    protocol::message::{
        CAPABILITY_RAW_CHUNK_FRAME, CAPABILITY_WINDOWED_REQUESTS, ETLE_WIRE_PROTOCOL_VERSION,
    },
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const GIT_COMMIT: &str = env!("ETLE_GIT_COMMIT");
pub const GIT_BRANCH: &str = env!("ETLE_GIT_BRANCH");
pub const GIT_TAG: &str = env!("ETLE_GIT_TAG");
pub const GIT_COMMIT_COUNT: &str = env!("ETLE_GIT_COMMIT_COUNT");
pub const GIT_COMMIT_MESSAGE: &str = env!("ETLE_GIT_COMMIT_MESSAGE");
pub const GIT_DIRTY: &str = env!("ETLE_GIT_DIRTY");

pub const BUILD_DATE: &str = env!("ETLE_BUILD_DATE");
pub const BUILD_PROFILE: &str = env!("ETLE_BUILD_PROFILE");
pub const BUILD_TARGET: &str = env!("ETLE_BUILD_TARGET");
pub const BUILD_HOST: &str = env!("ETLE_BUILD_HOST");
pub const RUSTC_VERSION: &str = env!("ETLE_RUSTC_VERSION");
pub const BUILD_CI: &str = env!("ETLE_BUILD_CI");
pub const BUILD_REPOSITORY: &str = env!("ETLE_BUILD_REPOSITORY");
pub const BUILD_RUN_ID: &str = env!("ETLE_BUILD_RUN_ID");

#[must_use]
pub fn long_version(binary: &str) -> String {
    format!(
        "{binary} {version} built from branch {branch} at commit {commit} {dirty} ({message}).\n\
         Date: {date}\n\
         Tag: {tag}, commits: {commits}\n\n\
         Build:\n\
           Profile: {profile}\n\
           Target: {target}\n\
           Host: {host}\n\
           Rustc: {rustc}\n\
           CI: {ci}\n\
           Repository: {repo}\n\
           Run ID: {run_id}\n\n\
         Protocol:\n\
           Wire protocol: {wire_protocol}\n\
           Descriptor format: {descriptor_version}\n\
           Capabilities: {raw_chunk}, {windowed_requests}\n\n\
         Version ABI string: {abi}\n",
        version = VERSION,
        branch = GIT_BRANCH,
        commit = GIT_COMMIT,
        dirty = GIT_DIRTY,
        message = GIT_COMMIT_MESSAGE,
        date = BUILD_DATE,
        tag = GIT_TAG,
        commits = GIT_COMMIT_COUNT,
        profile = BUILD_PROFILE,
        target = BUILD_TARGET,
        host = BUILD_HOST,
        rustc = RUSTC_VERSION,
        ci = BUILD_CI,
        repo = BUILD_REPOSITORY,
        run_id = BUILD_RUN_ID,
        wire_protocol = ETLE_WIRE_PROTOCOL_VERSION,
        descriptor_version = ETLE_DESCRIPTOR_VERSION,
        raw_chunk = CAPABILITY_RAW_CHUNK_FRAME,
        windowed_requests = CAPABILITY_WINDOWED_REQUESTS,
        abi = version_abi_string(),
    )
}

#[must_use]
pub fn version_abi_string() -> String {
    let dirty_suffix = if GIT_DIRTY == "clean" { "" } else { "_dirty" };

    format!(
        "{}{}_wire{}_desc{}",
        short_commit(),
        dirty_suffix,
        ETLE_WIRE_PROTOCOL_VERSION,
        ETLE_DESCRIPTOR_VERSION,
    )
}

pub fn print(binary: &str) {
    print!("{}", long_version(binary));
}

#[must_use]
pub fn args_request_version(args: impl IntoIterator<Item = String>) -> bool {
    let filtered = args
        .into_iter()
        .filter(|arg| !matches!(arg.as_str(), "-v" | "--verbose"))
        .collect::<Vec<_>>();

    filtered.len() == 1
        && matches!(
            filtered[0].as_str(),
            "--version" | "-V" | "version" | "build-info" | "--build-info"
        )
}

fn short_commit() -> &'static str {
    if GIT_COMMIT.len() >= 7 {
        &GIT_COMMIT[..7]
    } else {
        GIT_COMMIT
    }
}
