use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

const MACHINE_RELEASE_FINALIZE: &str = "--machine-release-finalize";

fn main() {
    let args = env::args_os().collect::<Vec<_>>();
    let result = if args.get(1).and_then(|value| value.to_str()) == Some(MACHINE_RELEASE_FINALIZE) {
        run_machine_release_finalize(args)
    } else {
        opsctl::parse_invocation(args).and_then(opsctl::execute)
    };

    match result {
        Ok(output) => print!("{output}"),
        Err(error) => {
            eprint!("{}", error.json());
            std::process::exit(2);
        }
    }
}

/// Internal build-pipeline transport for the current Release Set finalizer.
///
/// This deliberately bypasses the operator parser/registry: the input is an ephemeral
/// local packaging-observation DTO, the output is canonical Release Set bytes, and no
/// provider/network/process/production authority is granted here.
fn run_machine_release_finalize(args: Vec<OsString>) -> Result<String, opsctl::OpsctlError> {
    if args.len() != 4 {
        return Err(opsctl::OpsctlError::new(
            "release",
            "--machine-release-finalize requires exactly ROOT and REQUEST_JSON paths",
        ));
    }
    let root = PathBuf::from(args[2].clone());
    let request_json = PathBuf::from(args[3].clone());
    let input = fs::read_to_string(&request_json).map_err(|error| {
        opsctl::OpsctlError::new(
            "release",
            format!(
                "RELEASE_FINALIZE_REQUEST_UNAVAILABLE: {}: {error}",
                request_json.display()
            ),
        )
    })?;
    opsctl::release::finalize::finalize_json(&root, &input)
        .map_err(|error| opsctl::OpsctlError::new("release", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{MACHINE_RELEASE_FINALIZE, run_machine_release_finalize};
    use std::ffi::OsString;

    #[test]
    fn machine_release_finalize_requires_exact_local_paths() {
        assert!(
            run_machine_release_finalize(vec![
                OsString::from("opsctl"),
                OsString::from(MACHINE_RELEASE_FINALIZE),
            ])
            .is_err()
        );
        assert!(
            run_machine_release_finalize(vec![
                OsString::from("opsctl"),
                OsString::from(MACHINE_RELEASE_FINALIZE),
                OsString::from("."),
                OsString::from("missing-request.json"),
            ])
            .is_err()
        );
    }
}
