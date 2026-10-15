use std::process::Command;
use std::str;

#[test]
fn test_compiler_cli_output() {
    insta::glob!("inputs/*.must", |path| {
        let binary_path = env!("CARGO_BIN_EXE_mustc");

        let output = Command::new(binary_path)
            .arg(path)
            .arg("--no-color")
            .output()
            .expect("Failed to execute the must binary");

        let stdout = str::from_utf8(&output.stdout).unwrap_or("INVALID UTF-8");
        let stderr = str::from_utf8(&output.stderr).unwrap_or("INVALID UTF-8");

        let exit_code = output.status.code().unwrap_or(-1);

        let combined_output =
            format!("EXIT CODE: {exit_code}\n\n--- STDOUT ---\n{stdout}\n--- STDERR ---\n{stderr}");

        insta::assert_snapshot!(combined_output);
    });
}
