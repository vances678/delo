use std::{env::current_dir, fs, path::{Path, PathBuf}, process::Command};

use crate::error::CompilerRunnerError;

pub struct CompilerRunner {
    compilers: Vec<String>,
}

impl CompilerRunner {
    pub fn new() -> Self {
        let compilers = vec![
            "gcc".to_string(),
            "clang".to_string(),
            "cc".to_string(),
            "cl".to_string(),
            "clang-cl".to_string(),
        ];

        Self { compilers }
    }

    pub fn compile(&self, c_src: &str, program_name: &str) -> Result<String, CompilerRunnerError> {
        let current_dir = current_dir().map_err(|_| CompilerRunnerError::FailedToGetCurrentDirectory)?;
        let c_path = current_dir.join(format!("{program_name}.c"));
        fs::write(&c_path, c_src)?;

        let executable_path = self.executable_path(&current_dir, program_name);

        for compiler in &self.compilers {
            match self.run_compiler(compiler, &c_path, &executable_path) {
                Ok(()) => return Ok(executable_path.to_string_lossy().to_string()),
                Err(CompilerRunnerError::IoError(error)) 
                    if error.kind() == std::io::ErrorKind::NotFound => 
                {
                    continue;
                }
                Err(error) => {
                    return Err(error);
                }
            }
        }

        Err(CompilerRunnerError::NoCompilerFound {
            compilers_tried: self.compilers.clone()
        })
    }

    fn run_compiler(&self, compiler: &str, c_path: &Path, executable_path: &Path) -> Result<(), CompilerRunnerError> {
        let mut cmd = Command::new(compiler);
        
        match compiler {
            "gcc" | "clang" | "cc" => {
                cmd.arg("-g").arg(&c_path).arg("-o").arg(&executable_path).arg("-lm");
                if cfg!(windows) {
                    cmd.arg("-lbcrypt");
                }
            }
            _ => {
                cmd.arg("/Zi").arg("/Fe").arg(&executable_path).arg(&c_path);
                if cfg!(windows) {
                    cmd.arg("bcrypt.lib");
                }
            }
        };

        let output = cmd.output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(CompilerRunnerError::CompilationFailed {
                compiler: compiler.to_string(),
                status: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).to_string()
            })
        }
    }

    fn executable_path(&self, base_dir: &Path, program_name: &str) -> PathBuf {
        #[cfg(target_os = "windows")] {
            base_dir.join(format!("{program_name}.exe"))
        }
        #[cfg(not(target_os = "windows"))] {
            base_dir.join(program_name)
        }
    }
}