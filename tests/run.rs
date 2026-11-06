use std::io::ErrorKind;
use std::process::Command;
use std::{fs, str};

#[test]
fn test_compiler_cli_output() {
    let binary_path = env!("CARGO_BIN_EXE_mustc");

    insta::glob!("ok/*.must", |path| {
        match fs::remove_file("a.out") {
            Ok(()) => println!("File deleted successfully"),
            Err(e) if e.kind() == ErrorKind::NotFound => (),
            Err(e) => eprintln!("Error: {}", e),
        }

        let compiler_output = Command::new(binary_path)
            .arg(path)
            .arg("--no-color")
            .output()
            .expect("Failed to execute the must binary");

        let _ = Command::new("cc")
            .arg(path.with_extension("o"))
            .output()
            .expect("Failed to execute cc");

        let binary_output = match Command::new("./a.out").output() {
            Ok(output) => output,
            Err(_) => Command::new("echo")
                .arg("binary not found")
                .output()
                .unwrap(),
        };

        let c_stdout = str::from_utf8(&compiler_output.stdout).unwrap_or("INVALID UTF-8");
        let c_stderr = str::from_utf8(&compiler_output.stderr).unwrap_or("INVALID UTF-8");

        let b_stdout = str::from_utf8(&binary_output.stdout).unwrap_or("INVALID UTF-8");

        let c_exit_code = compiler_output.status.code().unwrap_or(-1);
        let b_exit_code = binary_output.status.code().unwrap_or(-1);

        let combined_output = format!(
            "COMPILER EXIT CODE: {c_exit_code}\n
--- COMPILER STDOUT ---\n{c_stdout}\n--- COMPILER STDERR ---\n{c_stderr}\n
BINARY EXIT CODE: {b_exit_code}\n
--- BINARY STDOUT ---\n{b_stdout}"
        );

        insta::assert_snapshot!(combined_output);
    });

    insta::glob!("err/*.must", |path| {
        let compiler_output = Command::new(binary_path)
            .arg(path)
            .arg("--no-color")
            .output()
            .expect("Failed to execute the must binary");

        let c_stdout = str::from_utf8(&compiler_output.stdout).unwrap_or("INVALID UTF-8");
        let c_stderr = str::from_utf8(&compiler_output.stderr).unwrap_or("INVALID UTF-8");

        let c_exit_code = compiler_output.status.code().unwrap_or(-1);

        let combined_output = format!(
            "COMPILER EXIT CODE: {c_exit_code}\n
--- COMPILER STDOUT ---\n{c_stdout}\n--- COMPILER STDERR ---\n{c_stderr}\n
"
        );

        insta::assert_snapshot!(combined_output);
    });
}
