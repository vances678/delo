>>> OVERVIEW
Delo is a statically typed, compiled to c, time-traveling programming language. It cannot actually go back in time, but it is able to access and mutate variables from previous states of a program.
Fun fact: The name Delo comes from DeLorean, the time-traveling car from Back to the Future.
NOTE: due to complexity of static type checking and compiling to c, the time-traveling features have not been implemented yet. However, arrays, ranges, structs, enums, and pattern matching are all somewhat implemented
The language currently handles basic lox functionality, in addition to supporting scanning, parsing, type checking, and compiling to c of many additional features.

>>> PROJECT STRUCTURE
- src/                          # contains the source code for the delo compiler (minus the test runner)
  - main.rs                     # the entry point of the delo compiler
  - ...
- testing/                      # contains testing-related files
  - test_runner.rs              # the test runner to run the tests
  - delo_tests/                 # contains files of delo code for testing
    - ...
- Cargo.toml                    # a rust project configuration file (ignore)
- Cargo.lock                    # an automatically generated lockfile (ignore)
- README.txt                    # this file
- TEST_RESULTS.txt              # output from running tests (IMPORTANT)

>>> BUILD INSTRUCTIONS
- A c compiler (gcc, clang, cc, cl, clang-cl) must be installed on your system to compile delo programs
- Rust must be installed on your system to build the delo compiler. If not installed, download and run the installer from https://rust-lang.org/tools/install/
- This project was tested with rustc version 1.91.0 (f8297e351 2025-10-28)
- The following commands should be run from the root directory (delo/)

- To build the delo compiler, run the following command:
cargo build --release

- To compile a delo program, ensure you have built the delo compiler, then run the following command, where <program.delo> is the filename of the program to compile:
On Linux/Mac: ./target/release/delo <program.delo>
On Windows: .\target\release\delo.exe <program.delo>

- To run the compiled delo program, run the following command, where <program> is the filename of the program without .delo:
On Linux/Mac: ./<program>
On Windows: <program>.exe

- To run tests, ensure you have built the delo compiler, then run the following command:
cargo run --bin test_runner