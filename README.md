# Hydra

A modern, statically-typed programming language because I got bored and liked Rust

***

## ✨ Features

* **Static Typing**: Strong type safety to catch errors at compile time.
* **Clear Syntax**: A clean and familiar C-style syntax that is easy to read and write.
* **Memory Safety**: Explicit control over mutability with `let` and `const` keywords.
* **Rich Primitives**: A comprehensive set of integer, floating-point, character, and boolean types.
* **Powerful Arrays**: Built-in support for fixed-size arrays with control over element mutability.
* **Expressive Slicing**: Native syntax for creating read-only or mutable array slices.
* **Modern Control Flow**: Intuitive range-based `for` loops, `forEach` loops for collections, and standard `while` loops.

***

## 📜 Language Specification

### Primitives

The language provides a set of fundamental data types:

* **Signed Integers**: `i8`, `i16`, `i32`, `i64`
* **Unsigned Integers**: `u8`, `u16`, `u32`, `u64`
* **Floating-Point**: `f32`, `f64`
* **Character**: `char` (delimited by single quotes, e.g., `'a'`)
* **Boolean**: `boolean` (`true` or `false`)
* **String**: `string` (a pseudo-primitive, delimited by double quotes, e.g., `"hello"`)

### Keywords

|           |           |           |           |
| :-------- | :-------- | :-------- | :-------- |
| `let`     | `const`   | `struct`  | `fn`      |
| `return`  | `void`    | `in`      | `for`     |
| `if`      | `else`    | `else if` | `forEach` |
| `while`   | `break`   | `skip`    | `include` |
| `typedef` | `None`    | `as`      |           |

* `skip`: Equivalent to `continue` in other languages.
* `include`: Used for importing modules.
* `typedef`: Used for creating type aliases.

***

## ⚙️ Syntax and Examples

### Variable Declaration

Variables are declared using `let` for mutable bindings and `const` for immutable bindings. Type annotations are mandatory.

```rust
// A mutable 32-bit integer
let x: i32 = 10;
x = 22; // This is valid

// An immutable 32-bit float
const PI: f32 = 3.14;
PI = 3.14159; // This would cause a compile-time error
```

### Arrays

Arrays have a fixed size and a declared element type. You can specify mutability for both the array binding and its elements.

**Declaration Syntax**: `<binding> <name>: [<mutability?> <type>, <size>] = { <elements> };`

```rust
// A mutable array of mutable i32s
let a: [i32, 3] = { 1, 2, 3 };
a[0] = 100; // Valid

// A mutable array of immutable chars
let b: [const char, 5] = { 'h', 'e', 'l', 'l', 'o' };
b[0] = 'j'; // Compile-time error: elements are const

// An immutable array of mutable i64s
const c: [i64, 3] = { 1, 2, 3 };
c = { 4, 5, 6 }; // Compile-time error: binding is const

// A fully immutable array
const d: [const char, 2] = { 'x', 'y' };
```

### Array Slicing

Slicing creates a view or a copy of a portion of an array.

* **Read-only Slice**: Use the `&` prefix to create a reference to a section of the original array.
* **Mutable Slice**: Use the `|array|` syntax with the `:=` operator to create a new, mutable slice on the heap.

```rust
let arr: [i32, 5] = {1, 2, 3, 4, 5};

// Create a read-only slice of the first three elements.
let read_only_slice: [i32, 3] = &arr[0..2];

// Create a new, mutable slice on the heap.
// NOTE: what allocates it on the heap is the bars between arr -> |arr|
let heap_slice: [i32, 2] := |arr|[3..5];
heap_slice[0] = 40; // This is valid
```

### Functions

Functions are declared with the `fn` keyword. The return type is specified after `->`. Use `void` for functions that do not return a value.

```rust
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

// Functions can accept arrays with a generic size parameter 'N'.
// This is resolved at compile-time via monomorphization.
fn print_sum(numbers: [i32, N]) -> void {
    let sum: i32 = 0;
    forEach (num in numbers) {
        sum = sum + num;
    }
    // Assume a println function exists
    println("Sum: {}", sum);
}

fn main() -> void {
    let passer: [i32, 5] = {1, 2, 3, 4, 5};
    let pass_thru: [i32, 10] = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};

    print_sum(passer);
    print_sum(pass_thru); // Compiler will inline 'N' in print_sum to the size of the passer and pass_thru variables
}
```

### Structs

Structs are composite data types that group together variables under a single name.

```rust
// Struct Declaration
struct Student {
    name: string;
    age: i32;
    gpa: f32;
};

// Struct Instantiation
let student: Student = Student {
    name = "Joseph";
    age = 16;
    gpa = 3.92;
};

// Accessing fields
println("{}", student.name); // Prints "Joseph"
```

### Control Flow

#### For Loops

`for` loops iterate over a range. The direction (incrementing or decrementing) is inferred from the start and end values.

* `start..end`: Exclusive range (does not include `end`).
* `start..=end`: Inclusive range (includes `end`).

```rust
// Prints numbers 0 through 9
for (i in 0..10) {
    println("{}", i);
}

// Implicit reversed iteration
// Prints numbers 5 through 0
for (i in 5..=0) {
    println("{}", i);
}
```

#### For Each Loops

`forEach` loops iterate over the elements of a collection, such as an array.

```rust
const letters: [const char, 3] = { 'a', 'b', 'c' };

forEach (letter in letters) {
    println("{}", letter);
}
```

#### While Loops

`while` loops execute as long as a condition is `true`.

```rust
let i: i32 = 0;

while (i < 5) {
    println("{}", i);
    i = i + 1;
}
```

#### Loop Control

* **`break`**: Exits the current loop immediately.
* **`skip`**: Skips the rest of the current iteration and proceeds to the next one (equivalent to `continue`).

***

## 🚀 Getting Started

This project is currently in the language specification phase. The compiler and toolchain are under active development. Stay tuned for updates!

## 🤝 Contributing

Contributions are welcome! If you have ideas for features, syntax improvements, or would like to help with the implementation, please open an issue or submit a pull request.
