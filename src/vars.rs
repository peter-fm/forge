use crate::error::ForgeError;
use crate::model::RunContext;
use std::collections::BTreeMap;

pub fn substitute_text(
    input: &str,
    variables: &BTreeMap<String, String>,
) -> Result<String, ForgeError> {
    substitute_text_with_mode(input, variables, MissingVariableMode::Error)
}

pub fn substitute_known_text(
    input: &str,
    variables: &BTreeMap<String, String>,
) -> Result<String, ForgeError> {
    substitute_text_with_mode(input, variables, MissingVariableMode::Preserve)
}

#[derive(Clone, Copy)]
enum MissingVariableMode {
    Error,
    Preserve,
}

fn substitute_text_with_mode(
    input: &str,
    variables: &BTreeMap<String, String>,
    missing_variable_mode: MissingVariableMode,
) -> Result<String, ForgeError> {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '{' {
            output.push(ch);
            continue;
        }

        let mut key = String::new();
        let mut found_end = false;
        for next in chars.by_ref() {
            if next == '}' {
                found_end = true;
                break;
            }
            key.push(next);
        }

        if !found_end {
            return Err(ForgeError::message("unterminated variable placeholder"));
        }

        if let Some(value) = variables.get(&key) {
            output.push_str(value);
            continue;
        }

        match missing_variable_mode {
            MissingVariableMode::Error => {
                return Err(ForgeError::message(format!("missing variable `{key}`")));
            }
            MissingVariableMode::Preserve => {
                output.push('{');
                output.push_str(&key);
                output.push('}');
            }
        }
    }

    Ok(output)
}

pub fn build_variable_scope(context: &RunContext) -> BTreeMap<String, String> {
    let mut variables = context.variables.clone();
    for result in context.step_results.values() {
        variables.insert(
            format!("{}.exit_code", result.name),
            result.exit_code.to_string(),
        );
        variables.insert(
            format!("{}_output", result.name),
            join_output(&result.stdout, &result.stderr),
        );
        if let Some(log_file) = &result.log_file {
            variables.insert(format!("{}.log_file", result.name), log_file.clone());
        }
    }
    variables
}

pub fn join_output(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (false, false) => format!("{stdout}\n{stderr}"),
    }
}
