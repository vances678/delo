use std::fs::{read_dir, read_to_string, remove_file, write};
use std::io::{Error, ErrorKind};
use std::process::Command;
use std::env::temp_dir;
use std::time::{SystemTime, UNIX_EPOCH};
use std::path::PathBuf;

// TODO: compile delo code before running it

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
    
    let output = Command::new("./target/debug/delo").arg(temp_path.to_str().unwrap()).output().expect("Failed to run test");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    let _ = remove_file(temp_path);

    // not ideal, but combining stdout and stderr is overly complicated and all tests are either completely stdout or stderr so this works for now 
    let passed = stdout == test.expected || (!stderr.is_empty() && stderr == test.expected);

    passed
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