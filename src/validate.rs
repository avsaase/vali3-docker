use std::{collections::HashMap, io::Write as _, process::Stdio};

use bytes::Bytes;
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use tokio::{process::Command, time::timeout};

use crate::{
    config::Config,
    error::ValiError,
    utils::{find_module_file, to_utf8, wine_path},
};

pub async fn run_ivshell(
    config: &Config,
    modules: &HashMap<String, (String, String)>,
    igc: Bytes,
) -> Result<IvshellOutput, ValiError> {
    let ccc = igc
        .get(1..4)
        .map(|ccc| String::from_utf8_lossy(ccc).into_owned())
        .ok_or_else(|| {
            tracing::warn!("IGC data is too short to determine the validator");
            ValiError::Format
        })?;
    let (vali_name, pattern) = modules.get(&ccc).ok_or_else(|| {
        tracing::warn!(ccc = %ccc, "no validator configured for IGC program");
        ValiError::Unsupported { ccc: ccc.clone() }
    })?;
    let vali = find_module_file(&config.vali_modules, vali_name).ok_or_else(|| {
        tracing::error!(
            ccc = %ccc,
            module = %vali_name,
            directory = %config.vali_modules.display(),
            "validator module not found"
        );
        ValiError::Service
    })?;

    // Run each validation in it's own working directory with a short path name to avoid problems
    // with old Windows executables and some write files next to themselves.
    let workdir = tempfile::Builder::new()
        .prefix("iv-")
        .tempdir()
        .map_err(|error| {
            tracing::error!(%error, "failed to create temporary working directory");
            ValiError::Service
        })?;
    let igc_path = workdir.path().join("flight.igc");
    std::fs::File::create(&igc_path)
        .and_then(|mut f| f.write_all(&igc))
        .map_err(|error| {
            tracing::error!(
                path = %igc_path.display(),
                %error,
                "failed to write IGC to temporary file"
            );
            ValiError::Service
        })?;

    // Redirect output to files instead of pipes. wineserver inherits the pipe file
    // descriptors and only exits a few seconds after the last wine process, so waiting on a
    // pipe for EOF would stall for several seconds after the validator has already finished.
    let stdout_path = workdir.path().join("ivshell.out");
    let stderr_path = workdir.path().join("ivshell.err");
    let stdout_file = std::fs::File::create(&stdout_path).map_err(|error| {
        tracing::error!(
            path = %stdout_path.display(),
            %error,
            "failed to create ivshell stdout file"
        );
        ValiError::Service
    })?;
    let stderr_file = std::fs::File::create(&stderr_path).map_err(|error| {
        tracing::error!(
            path = %stderr_path.display(),
            %error,
            "failed to create ivshell stderr file"
        );
        ValiError::Service
    })?;

    // Run the executable in its own process group so it gets killed when a timeout occurs
    let mut child = Command::new(&config.wine)
        .arg(wine_path(&config.ivshell))
        .arg("-s")
        .arg(wine_path(&vali))
        .arg(pattern)
        .arg(wine_path(&igc_path))
        .env("WINEDEBUG", "-all")
        .current_dir(workdir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            tracing::error!(
                wine = %config.wine.display(),
                %error,
                "failed to spawn ivshell"
            );
            ValiError::Service
        })?;
    let pgid = Pid::from_raw(-(child.id().expect("just spawned") as i32));

    match timeout(config.timeout, child.wait()).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            tracing::error!(%error, "failed to wait for ivshell");
            return Err(ValiError::Service);
        }
        Err(_) => {
            tracing::error!(
                timeout = ?config.timeout,
                "ivshell timed out and was killed"
            );
            let _ = kill(pgid, Signal::SIGKILL);
            return Err(ValiError::Service);
        }
    }

    let out = std::fs::read(&stdout_path).map_err(|error| {
        tracing::error!(
            path = %stdout_path.display(),
            %error,
            "failed to read ivshell output"
        );
        ValiError::Service
    })?;

    // Split the output into lines the same way PHP's exec() does: a trailing newline does not
    // produce an additional empty line, and every line is trimmed.
    let mut raw_lines: Vec<&[u8]> = out.split(|byte| *byte == b'\n').collect();
    while raw_lines
        .last()
        .is_some_and(|line| trim_bytes(line).is_empty())
    {
        raw_lines.pop();
    }

    let last_line = raw_lines.pop().ok_or_else(|| {
        tracing::error!("ivshell produced no output");
        ValiError::Service
    })?;
    let ivshell_line = to_utf8(trim_bytes(last_line));
    let response = ivshell_line.strip_prefix("IVSHELL:").ok_or_else(|| {
        tracing::error!(output = %ivshell_line, "invalid ivshell output");
        ValiError::Service
    })?;

    let mut parts = response.splitn(3, ',');
    let result = parts.next().unwrap_or_default();
    let code = parts
        .next()
        .and_then(|code| code.trim().parse::<u32>().ok())
        .ok_or_else(|| {
            tracing::error!(output = %ivshell_line, "invalid status in ivshell output");
            ValiError::Service
        })?;
    let message = parts.next().unwrap_or_default();

    if result.is_empty() || message.is_empty() || !matches!(code, 1..=3) {
        tracing::error!(output = %ivshell_line, "invalid ivshell response");
        return Err(ValiError::Service);
    }

    let status = match (result, code) {
        ("PASSED", 1) => "IGC_PASSED",
        ("FAILED", 2) => "IGC_FAILED",
        ("FAILED", 3) => "IGC_FAILED_NC",
        ("ERROR", 1) => "IGC_PASSED",
        ("ERROR", 2) => "IGC_FAILED",
        ("ERROR", 3) => "IGC_FAILED_NC",
        _ => {
            tracing::error!(
                result = %result,
                code,
                "unknown result in ivshell output"
            );
            return Err(ValiError::Service);
        }
    }
    .to_string();

    // Drop the empty lines at the start, like PHP does, but keep empty lines after the first
    // non-empty line because they are part of the validator output.
    let lines = raw_lines
        .into_iter()
        .map(|line| to_utf8(trim_bytes(line)))
        .skip_while(String::is_empty)
        .collect();

    Ok(IvshellOutput {
        result: result.to_string(),
        status,
        message: message.to_string(),
        lines,
    })
}

fn trim_bytes(mut line: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = line {
        if is_trimmed(*first) {
            line = rest;
        } else {
            break;
        }
    }

    while let [rest @ .., last] = line {
        if is_trimmed(*last) {
            line = rest;
        } else {
            break;
        }
    }

    line
}

fn is_trimmed(byte: u8) -> bool {
    // The default character set used by PHP's trim().
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0 | 0x0b)
}

#[derive(Debug)]
pub struct IvshellOutput {
    pub result: String,
    pub status: String,
    pub message: String,
    pub lines: Vec<String>,
}
