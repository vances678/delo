use std::fs::{read_dir, read_to_string, remove_file, write};
use std::io::{Error, ErrorKind};
use std::process::Command;
use std::env::{temp_dir, current_dir};
use std::time::{SystemTime, UNIX_EPOCH};
use std::path::PathBuf;

static PATH_TO_TESTS: &str = "testing/delo_tests";

struct Section {
    name: String,
    subsections: Vec<Subsection>,
}

struct Subsection {
    name: String,
    tests: Vec<Test>,
}

struct Test {
    description: String,
    code: String,
    expected: String,
}

fn main() {
    println!("=============================================");
    println!(">>>>>>>>>> DELO INTERPRETER TESTS <<<<<<<<<<<");
    println!("=============================================");

    let sections = get_tests().expect(&format!("Error getting tests from '{}'", PATH_TO_TESTS));
    let mut num_tests = 0;
    let mut num_passed_tests = 0;

    for section in sections {
        println!("\n---------------------------------------------");
        println!(">>> {}", section.name);
        println!("---------------------------------------------");

        for subsection in section.subsections {
            println!(">>> {}", subsection.name);

            for test in subsection.tests {
                if run_test(&test) {
                    println!("| PASSED: {}", test.description);
                    num_passed_tests += 1;
                } else {
                    println!("X FAILED: {}", test.description);
                }
                num_tests += 1;
            }
        }
    }

    println!("\n>>>>> TEST SUMMARY <<<<<");
    println!("Total Tests: {}", num_tests);
    println!("Pass Rate: {:.2}%", (num_passed_tests as f64 / num_tests as f64) * 100.0);
    println!("Passed: {}", num_passed_tests);
    println!("Failed: {}", num_tests - num_passed_tests);
}

fn run_test(test: &Test) -> bool {
    let temp_dir = temp_dir();
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let temp_path = temp_dir.join(format!("test_{}.delo", timestamp));

    write(&temp_path, &test.code).expect("Failed to write test code to temp file");

    #[cfg(target_os = "windows")]
    let delo_path = "target\\release\\delo.exe";
    #[cfg(not(target_os = "windows"))]
    let delo_path = "./target/release/delo";
    
    let compilation_output = Command::new(delo_path).arg(temp_path.to_str().unwrap()).output().expect("Failed to run delo compiler");

    let program_name = temp_path.file_stem().unwrap().to_string_lossy().to_string();
    let current_dir = current_dir().expect("Failed to get current directory");

    #[cfg(target_os = "windows")]
    let executable_path = current_dir.join(format!("{}.exe", program_name));
    #[cfg(not(target_os = "windows"))]
    let executable_path = current_dir.join(&program_name);

    let c_path = current_dir.join(format!("{program_name}.c"));

    let compilation_stderr_raw = trim(&compilation_output.stderr);
    let compilation_stderr = normalize_output(&normalize_error(&compilation_stderr_raw));
    let expected = normalize_output(test.expected.trim());

    if !compilation_output.status.success() {
        let _ = remove_file(&temp_path);
        let _ = remove_file(&c_path);
        let _ = remove_file(&executable_path);

        if compilation_stderr == expected {
            return true;
        } else {
            println!("> EXPECTED OUTPUT:\n{}\n", expected);
            println!("> ACTUAL OUTPUT:\n{}\n", compilation_stderr);
            return false;
        }
    }

    let program_output = Command::new(&executable_path).output().expect("Failed to run compiled program");
    let program_stdout = normalize_output(&trim(&program_output.stdout));
    let program_stderr = normalize_output(&trim(&program_output.stderr));

    let _ = remove_file(&temp_path);
    let _ = remove_file(&c_path);
    let _ = remove_file(&executable_path);

    if program_stdout == expected {
        true
    } else {
        println!("> EXPECTED OUTPUT:\n{}\n", expected);
        println!("> ACTUAL OUTPUT:\n{}\n", program_stdout);
        if !program_stderr.is_empty() {
            println!("> PROGRAM STDERR:\n{}\n", program_stderr);
        }
        false
    }
}

fn trim(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn normalize_error(string: &str) -> String {
    string.lines()
        .map(|line| {
            if line.trim_start().starts_with("@ ") {
                ""
            } else {
                line
            }
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_output(string: &str) -> String {
    string.replace("\r\n", "\n")
     .lines()
     .map(|line| line.trim_end())
     .collect::<Vec<_>>()
     .join("\n")
}

fn get_tests() -> Result<Vec<Section>, Error> {
    let mut sections = Vec::new();
    for section_entry in read_dir(PATH_TO_TESTS)? {
        let section_path = section_entry?.path();
        let mut section_name = section_path.file_name().unwrap().to_str().unwrap().chars().skip_while(|c| c.is_numeric() || *c == '_').collect::<String>().replace("_", " ");
        if !section_name.is_empty() {
            section_name.make_ascii_uppercase();
        }

        let mut subsections = Vec::new();
        for subsection_entry in read_dir(section_path)? {
            let subsection_path = subsection_entry?.path();
            let mut subsection_name = subsection_path.file_name().unwrap().to_str().unwrap().chars().skip_while(|c| c.is_numeric() || *c == '_').collect::<String>().replace("_", " ").replace(".delo", "");
            if !subsection_name.is_empty() {
                subsection_name.make_ascii_uppercase();
            }

            let tests = parse_test_file(read_to_string(&subsection_path)?, &subsection_path)?;
            subsections.push(Subsection { name: subsection_name, tests });
        } 

        sections.push(Section { name: section_name, subsections });
    }

    Ok(sections)
}

fn parse_test_file(content: String, path: &PathBuf) -> Result<Vec<Test>, Error> {
    let mut tests = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    
    while i < lines.len() {
        if let Some(description) = lines[i].trim().strip_prefix("// TEST:") {
            let description = description.trim().to_string();
            let mut code = String::new();
            let mut expected = String::new();
            
            i += 1;
            
            while i < lines.len() && !lines[i].trim().starts_with("// EXPECT:") {
                code.push_str(lines[i]);
                code.push('\n');
                i += 1;
            }

            if code.trim_end().is_empty() {
                return Err(Error::new(ErrorKind::InvalidData, format!("No code found for test '{}' in '{}'", description, path.display())));
            } else if i >= lines.len() {
                return Err(Error::new(ErrorKind::InvalidData, format!("Missing '// EXPECT:' section for test '{}' in '{}'", description, path.display())));
            }

            i += 1;

            while i < lines.len() && !lines[i].trim().starts_with("// TEST:") {
                expected.push_str(lines[i]);
                expected.push('\n');
                i += 1;
            }

            expected = expected.trim_end().to_string();
            
            tests.push(Test { description, code, expected });
        } else {
            i += 1;
        }
    }

    Ok(tests)
}