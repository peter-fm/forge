use crate::error::ForgeError;
use crate::model::RunContext;
use std::collections::BTreeMap;

pub fn substitute_text(
    input: &str,
    variables: &BTreeMap<String, String>,
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

        let value = variables
            .get(&key)
            .ok_or_else(|| ForgeError::message(format!("missing variable `{key}`")))?;
        output.push_str(value);
    }

    Ok(output)
}

pub fn build_variable_scope(context: &RunContext) -> BTreeMap<String, String> {
    let mut variables = context.variables.clone();
    for (name, result) in &context.step_results {
        variables.insert(format!("{name}.exit_code"), result.exit_code.to_string());
        variables.insert(
            format!("{name}_output"),
            join_output(&result.stdout, &result.stderr),
        );
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
