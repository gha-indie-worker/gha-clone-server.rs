//! Offline, read-only workflow YAML planner. No network, repository checkout or execution.
use std::fs::File;
use std::io::Read;
use std::process;

use gha_clone_server::{build_plan, PlanRequest, PlannerLimits};

const USAGE: &str = "Usage: plan_workflow [--require-independent] OWNER/REPO COMMIT_SHA .github/workflows/FILE.yml WORKFLOW_FILE\n\nReads a local GitHub Actions YAML file and prints a JSON capability plan.\n--require-independent exits 3 if any job cannot use the fixed-profile independent lane.\nParsing/usage failures exit 2. This command never executes a workflow.";

fn run() -> Result<i32, String> {
    let mut args = std::env::args().skip(1);
    let first = args.next().ok_or_else(|| USAGE.to_string())?;
    if first == "-h" || first == "--help" {
        println!("{USAGE}");
        return Ok(0);
    }
    let require_independent = first == "--require-independent";
    let repository = if require_independent {
        args.next().ok_or_else(|| USAGE.to_string())?
    } else {
        first
    };
    let revision = args.next().ok_or_else(|| USAGE.to_string())?;
    let workflow_path = args.next().ok_or_else(|| USAGE.to_string())?;
    let file_path = args.next().ok_or_else(|| USAGE.to_string())?;
    if args.next().is_some() {
        return Err(USAGE.to_string());
    }

    let limits = PlannerLimits::default();
    // Bound untrusted local input before parsing (rather than reading an
    // arbitrary-size file into memory).
    let input = File::open(&file_path)
        .map_err(|error| format!("cannot read workflow file {file_path:?}: {error}"))?;
    let mut workflow_yaml = String::new();
    input
        .take(limits.max_workflow_bytes as u64 + 1)
        .read_to_string(&mut workflow_yaml)
        .map_err(|error| format!("workflow file is not readable UTF-8: {error}"))?;

    let request = PlanRequest {
        repository,
        revision,
        workflow_path,
        workflow_yaml,
    };
    let plan = build_plan(&request, &limits).map_err(|errors| errors.join("\n"))?;
    let output = serde_json::to_string_pretty(&plan)
        .map_err(|error| format!("cannot serialize workflow plan: {error}"))?;
    println!("{output}");

    if require_independent && !plan.independent_executable {
        eprintln!(
            "independent-lane admission denied: inspect independentReasons and warnings in JSON"
        );
        Ok(3)
    } else {
        Ok(0)
    }
}

fn main() {
    match run() {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            process::exit(2);
        }
    }
}
