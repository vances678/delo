# delo

> A time-traveling programming language.

Delo is a statically typed, compiled-to-C programming language that can time-travel. Every variable retains a mutable assignment history, enabling reassignments of past values. Reassignments ripple forward to redefine the present, hence the name delo, a nod to the DeLorean from _Back to the Future_.

## Example

```delo
// portfolio simulation
var portfolio: Double = 10000.0
portfolio = portfolio * 1.10    // Q1: +10%
portfolio = portfolio * 0.85    // Q2: -15%
portfolio = portfolio * 1.20    // Q3: +20%
portfolio = portfolio * 1.05    // Q4: +5%
print(portfolio)                // 11781   end-of-year value

// time-travel reads
print(portfolio@=0)             // 10000   absolute: state 0
print(portfolio@-4)             // 10000   relative: 4 back from current -> state 0

// time-travel writes
portfolio@=2 = portfolio@=1 * 1.05    // absolute: state 2 = state 1 * 1.05
print(portfolio)                      // 14553   ripples forward through Q3 and Q4
portfolio@-2 = portfolio@-1 * 0.85    // relative: 2 states back = 1 state back from the 2 states back * 0.85
print(portfolio)                      // 11781   reset to original

// try each hypothetical value for Q2 and watch the reassignments ripple forward
for (q2 in [0.85, 0.95, 1.05, 1.15, 1.25]) {
  portfolio@=2 = portfolio@=1 * q2
  print(portfolio)              // 11781, 13167, 14553, 15939, 17325
}
```

See [delo_syntax.txt](delo_syntax.txt) for the full language syntax.

## Features

- Statically and strongly typed with partial type inference (variables and function return types inferred; parameter types required)
- Parametric polymorphism (generic enums, structs, and functions)
- Compiled to native executables by lowering to C and invoking a system C compiler
- Time-traveling: read and mutate variables at previous program states using `x@=N` (absolute) and `x@-N` (relative); changes ripple forward through later states
- Pattern matching expressions with wildcards (`_`), literals, enum-variant destructuring, ranges (inclusive and exclusive), and guard clauses (`if ...`)
- First-class and higher-order functions with required parameter types, inferred or explicit return types, and generic type parameters
- Anonymous functions (lambdas) with full closures
- Enums with associated values (payloads) and generic type parameters
- Structs with fields and generic type parameters; field access (`.`) and instantiation
- Optional types with `Int?` syntactic sugar, pattern-matched destructuring, nil-coalescing operator (`??`), no force unwrapping
- Primitive types: `Int`, `Double`, `Bool`, `String`, `Void`
- Generic built-ins: `Optional<T>`, `Array<T>`, `Map<K, V>`, `Range<T>`, `InclusiveRange<T>`
- Arithmetic operators including exponentiation (`^`), modulus (`%`), and unary negation (`-`)
- Compound assignments (`+=`, `-=`, `*=`, `/=`, `%=`, `^=`) and increment/decrement (`++`, `--`)
- String concatenation (`+`) and repetition (`*`); array concatenation (`+`) and repetition (`*`)
- Comparison operators (`==`, `!=`, `<`, `<=`, `>`, `>=`) and short-circuiting logical operators (`&&`, `||`, `!`)
- Index access (`[]`) and assignment for arrays and maps
- If/else expressions and statements
- While loops and for-loops over ranges, with `break` and `continue`

## Installation

To use delo, you'll need:

- [Rust](https://rust-lang.org/tools/install/) - provides `cargo`, Rust's package manager
- A C compiler (`gcc`, `clang`, `cc`, `cl`, or `clang-cl`) - delo compiles to C, which the C compiler then turns into a native executable

Then install delo:

```sh
cargo install delo
```

## Usage

Compile a delo program:

```sh
delo program.delo
```

Run the compiled executable:

```sh
./program        # Linux/macOS
program.exe      # Windows
```

## Building from source

From a clone of the source repo, you can build the delo compiler with:

```sh
cargo build --release
```

This places the compiler binary at `./target/release/delo` (or `.\target\release\delo.exe` on Windows). Invoke it instead of `delo` in the [Usage](#usage) commands above.

## Running the test suite

From a clone of the source repo, build delo first, then run the tests:

```sh
cargo build --release
cargo run --bin test_runner
```

## License

MIT - see [LICENSE](LICENSE).
